//! Where a password-reset link goes: every email address and Telegram account linked to the
//! account the user named (an address only ever reaches itself).

use entity::{EmailTokenPurpose, IdentityProvider, user, user_identity};
use lettre::Address;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::auth::email::message::{Kind, Lang};
use crate::auth::email::{EmailService, ResetLimiter, identity_for, token, username_of};
use crate::auth::telegram::TelegramService;
use crate::auth::validation;
use crate::error::AppError;

/// What the user typed into "forgot password".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Login {
    Username(String),
    Email(Address),
}

impl Login {
    /// Usernames cannot contain `@`, so anything with one is meant as an address.
    pub(super) fn parse(raw: &str) -> Result<Self, AppError> {
        if raw.contains('@') {
            Ok(Self::Email(token::normalize_email(raw)?))
        } else {
            Ok(Self::Username(validation::normalize_username(raw)?))
        }
    }

    /// Limiter key; the prefix keeps usernames apart from addresses.
    pub(super) fn limit_key(&self) -> String {
        match self {
            Self::Username(name) => format!("user:{name}"),
            Self::Email(addr) => addr.to_string(),
        }
    }
}

/// One way to reach the account's owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Channel {
    Email(Address),
    /// Telegram user id = the private chat with the bot.
    Telegram(i64),
}

impl Channel {
    /// From a stored identity; `None` for a provider that cannot carry a link (Google: there is
    /// no way to message the account).
    fn of(identity: &user_identity::Model) -> Option<Self> {
        match identity.provider {
            // Stored subjects were validated on the way in; re-parsing only rebuilds the Address.
            IdentityProvider::Email => token::normalize_email(&identity.subject)
                .ok()
                .map(Self::Email),
            IdentityProvider::Telegram => identity.subject.parse().ok().map(Self::Telegram),
            IdentityProvider::Google => None,
        }
    }

    fn provider(&self) -> IdentityProvider {
        match self {
            Self::Email(_) => IdentityProvider::Email,
            Self::Telegram(_) => IdentityProvider::Telegram,
        }
    }

    fn subject(&self) -> String {
        match self {
            Self::Email(addr) => addr.to_string(),
            Self::Telegram(id) => id.to_string(),
        }
    }
}

/// Every identity of `user` that can carry a reset link, oldest first.
pub async fn linked_channels(
    db: &impl ConnectionTrait,
    user: Uuid,
) -> Result<Vec<Channel>, AppError> {
    let identities = user_identity::Entity::find()
        .filter(user_identity::Column::UserId.eq(user))
        .order_by_asc(user_identity::Column::CreatedAt)
        .order_by_asc(user_identity::Column::Id)
        .all(db)
        .await?;
    Ok(identities.iter().filter_map(Channel::of).collect())
}

/// Every email address linked to `user`, oldest first.
pub async fn linked_addresses(
    db: &impl ConnectionTrait,
    user: Uuid,
) -> Result<Vec<Address>, AppError> {
    Ok(linked_channels(db, user)
        .await?
        .into_iter()
        .filter_map(|c| match c {
            Channel::Email(addr) => Some(addr),
            Channel::Telegram(_) => None,
        })
        .collect())
}

/// The configured senders; a channel without one is skipped.
pub(super) struct Senders {
    pub email: Option<EmailService>,
    pub telegram: Option<TelegramService>,
}

pub(super) async fn deliver(
    db: &impl ConnectionTrait,
    senders: &Senders,
    limiter: &ResetLimiter,
    login: Login,
    lang: Lang,
) -> Result<(), AppError> {
    let (user_id, channels) = match login {
        Login::Email(addr) => match identity_for(db, addr.as_ref()).await? {
            Some(identity) => (identity.user_id, vec![Channel::Email(addr)]),
            None => return Ok(()),
        },
        Login::Username(name) => {
            let found = user::Entity::find()
                .filter(user::Column::Username.eq(name))
                .one(db)
                .await?;
            let Some(found) = found else { return Ok(()) };
            (found.id, linked_channels(db, found.id).await?)
        }
    };
    let Some(username) = username_of(db, user_id).await? else {
        return Ok(());
    };
    for channel in channels {
        let configured = match &channel {
            Channel::Email(_) => senders.email.is_some(),
            Channel::Telegram(_) => senders.telegram.is_some(),
        };
        if !configured {
            continue;
        }
        if !limiter.check_account(user_id) {
            break;
        }
        let token = token::issue_for(
            db,
            EmailTokenPurpose::PasswordReset,
            Some(user_id),
            channel.provider(),
            &channel.subject(),
        )
        .await?;
        match (channel, &senders.email, &senders.telegram) {
            (Channel::Email(addr), Some(email), _) => {
                email
                    .send(
                        Kind::PasswordReset,
                        lang,
                        addr,
                        Some(&token),
                        Some(&username),
                    )
                    .await;
            }
            (Channel::Telegram(chat), _, Some(telegram)) => {
                telegram.send_reset(chat, lang, &token, &username).await;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn login_with_at_sign_is_an_address() {
        let login = Login::parse("  Eva@Example.CZ ").expect("valid");
        assert_eq!(
            login,
            Login::Email("eva@example.cz".parse().expect("address"))
        );
        assert_eq!(login.limit_key(), "eva@example.cz");
    }

    #[test]
    fn login_without_at_sign_is_a_username() {
        let login = Login::parse(" Eva_1 ").expect("valid");
        assert_eq!(login, Login::Username("eva_1".into()));
        assert_eq!(login.limit_key(), "user:eva_1");
    }

    #[test]
    fn malformed_login_is_rejected() {
        for bad in ["", "a b", "x", "eva@", "@b.cz", "a@b@c.cz"] {
            assert!(Login::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    fn identity(provider: IdentityProvider, subject: &str) -> user_identity::Model {
        let now = Utc::now().fixed_offset();
        user_identity::Model {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            provider,
            subject: subject.into(),
            verified_at: now,
            created_at: now,
        }
    }

    #[test]
    fn channels_from_identities() {
        let mail = Channel::of(&identity(IdentityProvider::Email, "eva@example.cz"));
        assert_eq!(
            mail,
            Some(Channel::Email("eva@example.cz".parse().expect("address")))
        );
        let tg = Channel::of(&identity(IdentityProvider::Telegram, "42")).expect("telegram");
        assert_eq!(tg, Channel::Telegram(42));
        assert_eq!(
            (tg.provider(), tg.subject()),
            (IdentityProvider::Telegram, "42".into())
        );
        assert_eq!(
            Channel::of(&identity(IdentityProvider::Telegram, "x")),
            None
        );
        assert_eq!(
            Channel::of(&identity(IdentityProvider::Google, "1093")),
            None
        );
    }
}
