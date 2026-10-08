//! "A new sign-in method was linked" notice to the account's linked email addresses: if someone
//! else linked their provider account (stolen session), the owner hears about it.

use lettre::Address;
use uuid::Uuid;

use super::Provider;
use crate::auth::email::message::Lang;
use crate::auth::email::username_of;
use crate::auth::reset::linked_addresses;
use crate::mail::Email;
use crate::state::AppState;

pub fn render(provider: Provider, lang: Lang, to: Address, username: &str) -> Email {
    let name = provider.display_name();
    let (subject, body) = match lang {
        Lang::Cs => (
            format!("K účtu localdate bylo propojeno přihlášení přes {name}"),
            format!(
                "K tvému účtu {username} bylo právě propojeno přihlášení přes {name}. \
                 Pokud jsi to nebyl(a) ty, změň si heslo a v Nastavení → Propojené účty \
                 propojení odeber."
            ),
        ),
        Lang::En => (
            format!("{name} sign-in was linked to your localdate account"),
            format!(
                "{name} sign-in was just linked to your account {username}. If this wasn't you, \
                 change your password and remove it under Settings → Linked accounts."
            ),
        ),
    };
    let html = format!(
        "<p>{}</p>",
        body.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    );
    Email {
        to,
        subject,
        text: format!("{body}\n"),
        html,
    }
}

/// Mails every linked address of `user` after the response; nothing when email is off or none
/// is linked. Failures are only logged, like every mail.
pub fn linked(state: &AppState, user: Uuid, provider: Provider, lang: Lang) {
    let Some(service) = state.email.clone() else {
        return;
    };
    let db = state.db.clone();
    let mailer = service.clone();
    state.detached.spawn(async move {
        let found = async {
            let username = username_of(&db, user).await?;
            Ok::<_, crate::error::AppError>((username, linked_addresses(&db, user).await?))
        };
        match found.await {
            Ok((Some(username), addresses)) => {
                for to in addresses {
                    mailer.deliver(render(provider, lang, to, &username)).await;
                }
            }
            Ok((None, _)) => {}
            Err(e) => tracing::error!(error = %e, "link notice lookup failed"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_names_provider_and_account_and_escapes_html() {
        let to: Address = "a@b.cz".parse().expect("address");
        let cs = render(Provider::Google, Lang::Cs, to.clone(), "eva<1>");
        assert!(cs.subject.contains("Google"));
        assert!(cs.text.contains("eva<1>"));
        assert!(cs.html.contains("eva&lt;1&gt;") && !cs.html.contains("eva<1>"));
        let en = render(Provider::Google, Lang::En, to, "eva");
        assert!(
            en.text
                .starts_with("Google sign-in was just linked to your account eva.")
        );
    }
}
