//! Where a password-reset link goes: every email address and Telegram account linked to the
//! account whose username was typed, and an address typed that is an email identity (an address
//! only ever reaches itself).

use entity::{EmailTokenPurpose, IdentityProvider, user, user_identity};
use lettre::Address;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::auth::email::message::{Kind, Lang};
use crate::auth::email::{EmailService, ResetLimiter, identity_for, token, username_of};
use crate::auth::telegram::TelegramService;
use crate::auth::validation;
use crate::error::AppError;

/// What the user typed into "forgot password": a username, an email address, or both at once —
/// usernames may contain `@`, so `a@b.cz` can name an account and an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Login {
    pub username: Option<String>,
    pub email: Option<Address>,
}

impl Login {
    /// `400 validation` only when the input is neither a valid username nor a valid address.
    pub(super) fn parse(raw: &str) -> Result<Self, AppError> {
        let email = token::normalize_email(raw).ok();
        let username = match validation::normalize_username(raw) {
            Ok(name) => Some(name.name),
            Err(e) if email.is_none() => return Err(e),
            Err(_) => None,
        };
        Ok(Self { username, email })
    }

    /// Limiter key, case-insensitive like the lookup; the prefix keeps usernames apart from addresses.
    pub(super) fn limit_key(&self) -> String {
        match (&self.email, &self.username) {
            (Some(addr), _) => addr.to_string(),
            (None, Some(name)) => format!("user:{}", validation::username_key(name)),
            (None, None) => String::new(),
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
    /// From a stored identity; `None` for a provider that cannot carry a link (Google, Facebook: there is
    /// no way to message the account).
    fn of(identity: &user_identity::Model) -> Option<Self> {
        match identity.provider {
            // Stored subjects were validated on the way in; re-parsing only rebuilds the Address.
            IdentityProvider::Email => token::normalize_email(&identity.subject)
                .ok()
                .map(Self::Email),
            IdentityProvider::Telegram => identity.subject.parse().ok().map(Self::Telegram),
            IdentityProvider::Google | IdentityProvider::Facebook => None,
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
    // One account's failure must not cost the other account (username vs address) its link.
    for (user_id, channels) in targets(db, login).await? {
        if let Err(e) = send_to(db, senders, limiter, user_id, channels, lang).await {
            tracing::error!(error = ?e, %user_id, "password reset delivery failed");
        }
    }
    Ok(())
}

/// The accounts `login` names and their channels: the username's account gets every linked
/// channel, an address that is an email identity gets itself. The two can be one account (merged,
/// no channel twice) or two different ones (both get theirs).
async fn targets(
    db: &impl ConnectionTrait,
    login: Login,
) -> Result<Vec<(Uuid, Vec<Channel>)>, AppError> {
    let mut targets: Vec<(Uuid, Vec<Channel>)> = Vec::new();
    if let Some(name) = &login.username {
        let found = user::Entity::find()
            .filter(validation::username_matches(name))
            .one(db)
            .await?;
        if let Some(found) = found {
            targets.push((found.id, linked_channels(db, found.id).await?));
        }
    }
    if let Some(addr) = login.email
        && let Some(identity) = identity_for(db, addr.as_ref()).await?
    {
        let channel = Channel::Email(addr);
        match targets.iter_mut().find(|(id, _)| *id == identity.user_id) {
            Some((_, channels)) if channels.contains(&channel) => {}
            Some((_, channels)) => channels.push(channel),
            None => targets.push((identity.user_id, vec![channel])),
        }
    }
    Ok(targets)
}

async fn send_to(
    db: &impl ConnectionTrait,
    senders: &Senders,
    limiter: &ResetLimiter,
    user_id: Uuid,
    channels: Vec<Channel>,
    lang: Lang,
) -> Result<(), AppError> {
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
    fn login_that_is_an_address_is_also_a_username() {
        let login = Login::parse("  Eva@Example.CZ ").expect("valid");
        assert_eq!(
            login,
            Login {
                username: Some("Eva@Example.CZ".into()),
                email: Some("eva@example.cz".parse().expect("address")),
            }
        );
        assert_eq!(login.limit_key(), "eva@example.cz");
    }

    #[test]
    fn login_that_is_no_address_is_a_username() {
        for raw in [" Eva Nová ", "eva@", "a@b@c.cz"] {
            let login = Login::parse(raw).expect("valid");
            assert_eq!(login.username.as_deref(), Some(raw.trim()));
            assert_eq!(login.email, None);
        }
        let login = Login::parse(" Eva Nová ").expect("valid");
        assert_eq!(login.limit_key(), "user:eva nová");
    }

    #[test]
    fn malformed_login_is_rejected() {
        for bad in ["", "   ", "a\nb", &"x".repeat(65)] {
            assert!(Login::parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn long_address_is_only_an_address() {
        let raw = format!("{}@example.cz", "x".repeat(60));
        let login = Login::parse(&raw).expect("valid");
        assert_eq!(login.username, None);
        assert!(login.email.is_some());
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
