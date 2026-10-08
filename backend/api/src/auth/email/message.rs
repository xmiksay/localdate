//! Email texts (plain + minimal HTML), Czech by default.

use lettre::Address;

use crate::mail::Email;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Cs,
    En,
}

impl Lang {
    /// Accept-Language-ish: anything starting with `en` is English, the rest Czech.
    pub fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some(l) if l.trim().to_ascii_lowercase().starts_with("en") => Self::En,
            _ => Self::Cs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Login,
    Signup,
    Link,
    /// Linking an address that already belongs to an account: no link, just a heads-up.
    AlreadyLinked,
    PasswordReset,
}

impl Kind {
    /// Where the mailed link points in the PWA.
    pub fn path(self) -> &'static str {
        match self {
            Self::Link => "/auth/email/link",
            Self::PasswordReset => "/auth/password/reset",
            Self::Login | Self::Signup | Self::AlreadyLinked => "/auth/email",
        }
    }
}

struct Texts {
    subject: &'static str,
    intro: &'static str,
    action: &'static str,
    outro: &'static str,
}

fn texts(kind: Kind, lang: Lang) -> Texts {
    use {Kind::*, Lang::*};
    let (subject, intro, action) = match (kind, lang) {
        (Login, Cs) => (
            "Přihlášení do localdate",
            "Klikni na odkaz a přihlas se:",
            "Přihlásit se",
        ),
        (Login, En) => ("Log in to localdate", "Click the link to log in:", "Log in"),
        (Signup, Cs) => (
            "Dokonči registraci do localdate",
            "K tomuto e-mailu zatím nemáš účet. Založíš si ho tímto odkazem:",
            "Vytvořit účet",
        ),
        (Signup, En) => (
            "Finish signing up for localdate",
            "There is no account for this address yet. Create one with this link:",
            "Create account",
        ),
        (Link, Cs) => (
            "Propojení e-mailu s localdate",
            "Potvrď, že chceš tento e-mail propojit se svým účtem:",
            "Propojit e-mail",
        ),
        (Link, En) => (
            "Link your email to localdate",
            "Confirm that you want to link this address to your account:",
            "Link email",
        ),
        (AlreadyLinked, Cs) => (
            "Tento e-mail už je propojený",
            "Někdo chtěl tento e-mail propojit s účtem localdate, ale už s jedním účtem propojený je. \
             Přihlásit se jím můžeš přes „Přihlásit e-mailem“.",
            "",
        ),
        (AlreadyLinked, En) => (
            "This email is already linked",
            "Someone tried to link this address to a localdate account, but it already belongs to one. \
             You can log in with it via “Log in with email”.",
            "",
        ),
        (PasswordReset, Cs) => (
            "Obnovení hesla do localdate",
            "Někdo požádal o nové heslo k tvému účtu. Nastavíš si ho tímto odkazem:",
            "Nastavit nové heslo",
        ),
        (PasswordReset, En) => (
            "Reset your localdate password",
            "Someone asked for a new password for your account. Set one with this link:",
            "Set a new password",
        ),
    };
    let outro = match lang {
        Cs => {
            "Odkaz platí 15 minut a jde použít jen jednou. Pokud jsi o nic nežádal(a), e-mail ignoruj."
        }
        En => {
            "The link is valid for 15 minutes and works once. If you didn't ask for this, ignore this email."
        }
    };
    Texts {
        subject,
        intro,
        action,
        outro,
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn account_line(lang: Lang, username: &str) -> String {
    match lang {
        Lang::Cs => format!("Pro účet: {username}"),
        Lang::En => format!("For account: {username}"),
    }
}

/// `link` is `None` exactly for [`Kind::AlreadyLinked`]; `username` names the account a login or
/// link mail is for, so the recipient can tell a link meant for someone else.
pub fn render(
    kind: Kind,
    lang: Lang,
    to: Address,
    link: Option<&str>,
    username: Option<&str>,
) -> Email {
    let t = texts(kind, lang);
    let account = username.map(|u| account_line(lang, u));
    let (text, html) = match link {
        Some(link) => (
            format!(
                "{}\n{}\n{link}\n\n{}\n",
                t.intro,
                account
                    .as_deref()
                    .map_or(String::new(), |a| format!("{a}\n")),
                t.outro
            ),
            format!(
                "<p>{}</p>{}<p><a href=\"{href}\">{}</a></p><p>{href}</p><p><small>{}</small></p>",
                escape(t.intro),
                account.as_deref().map_or(String::new(), |a| format!(
                    "<p><strong>{}</strong></p>",
                    escape(a)
                )),
                escape(t.action),
                escape(t.outro),
                href = escape(link),
            ),
        ),
        None => (
            format!("{}\n", t.intro),
            format!("<p>{}</p>", escape(t.intro)),
        ),
    };
    Email {
        to,
        subject: t.subject.to_owned(),
        text,
        html,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_defaults_to_czech() {
        assert_eq!(Lang::parse(None), Lang::Cs);
        assert_eq!(Lang::parse(Some("cs")), Lang::Cs);
        assert_eq!(Lang::parse(Some("de")), Lang::Cs);
        assert_eq!(Lang::parse(Some(" EN-gb")), Lang::En);
    }

    fn to() -> Address {
        "a@b.cz".parse().expect("address")
    }

    #[test]
    fn link_lands_in_both_parts_escaped_in_html() {
        let link = "https://a.cz/auth/email#token=abc&x=<1>";
        let e = render(Kind::Login, Lang::En, to(), Some(link), Some("eva_1"));
        assert!(e.text.contains("For account: eva_1"));
        assert!(e.html.contains("For account: eva_1"));
        assert_eq!(e.subject, "Log in to localdate");
        assert!(e.text.contains(link));
        assert!(e.html.contains("token=abc&amp;x=&lt;1&gt;"));
        assert!(!e.html.contains("<1>"));
    }

    #[test]
    fn every_kind_and_lang_has_text() {
        for kind in [Kind::Login, Kind::Signup, Kind::Link, Kind::PasswordReset] {
            for lang in [Lang::Cs, Lang::En] {
                let e = render(kind, lang, to(), Some("https://a.cz/x"), None);
                assert!(!e.subject.is_empty() && e.text.contains("https://a.cz/x"));
            }
        }
        assert_eq!(Kind::PasswordReset.path(), "/auth/password/reset");
        assert_eq!(Kind::Link.path(), "/auth/email/link");
        assert_eq!(Kind::Signup.path(), "/auth/email");
        let notice = render(Kind::AlreadyLinked, Lang::Cs, to(), None, None);
        assert!(!notice.text.contains("http"));
    }
}
