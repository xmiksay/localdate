//! Outgoing email behind the [`Mailer`] trait: SMTP in production, a log line in dev,
//! an in-memory outbox in tests.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use lettre::message::{Mailbox, MultiPart};
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::{EmailConfig, EmailTransport};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    /// Already validated (`normalize_email`); sent as a bare mailbox, never re-parsed.
    pub to: Address,
    pub subject: String,
    pub text: String,
    pub html: String,
}

pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Boxed future instead of `async fn` so the mailer can live in `AppState` as `dyn Mailer`.
pub trait Mailer: Send + Sync {
    fn send(&self, email: Email) -> SendFuture<'_>;
}

/// The mailer for `cfg`; fails on an unparsable `SMTP_URL` or `EMAIL_FROM`. Nothing connects yet,
/// but the SMTP pool spawns its reaper task, so call it inside the Tokio runtime.
pub fn from_config(cfg: &EmailConfig) -> Result<Arc<dyn Mailer>> {
    let from: Mailbox = cfg
        .from
        .parse()
        .context("EMAIL_FROM is not a valid mailbox")?;
    Ok(match &cfg.transport {
        // The URL holds the SMTP password: keep it out of the error context.
        EmailTransport::Smtp(url) => Arc::new(SmtpMailer {
            transport: AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
                .context("SMTP_URL is not a valid smtp:// or smtps:// URL")?
                .build(),
            from,
        }),
        EmailTransport::DevLog => {
            tracing::warn!("EMAIL_DEV_LOG is on: login links are logged, not sent (dev only)");
            Arc::new(LogMailer)
        }
    })
}

struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl Mailer for SmtpMailer {
    fn send(&self, email: Email) -> SendFuture<'_> {
        Box::pin(async move {
            let message = Message::builder()
                .from(self.from.clone())
                .to(Mailbox::new(None, email.to))
                .subject(email.subject)
                .multipart(MultiPart::alternative_plain_html(email.text, email.html))
                .context("building email")?;
            self.transport
                .send(message)
                .await
                .context("sending email via SMTP")?;
            Ok(())
        })
    }
}

/// Dev only (`EMAIL_DEV_LOG`): the text body, link included, goes to the log.
struct LogMailer;

impl Mailer for LogMailer {
    fn send(&self, email: Email) -> SendFuture<'_> {
        tracing::info!(to = %email.to, subject = %email.subject, "dev email\n{}", email.text);
        Box::pin(async { Ok(()) })
    }
}

/// Captures every email; integration tests read the links out of it.
#[derive(Default)]
pub struct MemoryMailer {
    sent: Mutex<Vec<Email>>,
}

impl MemoryMailer {
    pub fn sent(&self) -> Vec<Email> {
        self.sent.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Mailer for MemoryMailer {
    fn send(&self, email: Email) -> SendFuture<'_> {
        self.sent
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(email);
        Box::pin(async { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(transport: EmailTransport, from: &str) -> EmailConfig {
        EmailConfig {
            base_url: "https://a.cz".into(),
            from: from.into(),
            transport,
        }
    }

    #[tokio::test]
    async fn from_config_validates_from_and_url() {
        let smtp = |u: &str| EmailTransport::Smtp(u.into());
        assert!(
            from_config(&cfg(
                smtp("smtps://u:p@mail.a.cz:465"),
                "localdate <no@a.cz>"
            ))
            .is_ok()
        );
        assert!(from_config(&cfg(EmailTransport::DevLog, "not a mailbox")).is_err());
        let err = from_config(&cfg(smtp("ftp://u:secret@x"), "no@a.cz"))
            .err()
            .expect("bad scheme");
        assert!(!format!("{err:?}").contains("secret"));
    }

    #[tokio::test]
    async fn memory_mailer_captures() {
        let m = MemoryMailer::default();
        let email = Email {
            to: "a@b.cz".parse().expect("address"),
            subject: "s".into(),
            text: "t".into(),
            html: "h".into(),
        };
        m.send(email.clone()).await.expect("send");
        assert_eq!(m.sent(), vec![email]);
    }
}
