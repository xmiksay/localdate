use entity::user;
use sea_orm::ColumnTrait;
use sea_orm::sea_query::{Expr, SimpleExpr};
use unicode_normalization::UnicodeNormalization;

use crate::error::AppError;

const USERNAME_LEN: std::ops::RangeInclusive<usize> = 1..=64;
const UNSTORABLE: [char; 3] = ['\0', '\r', '\n'];
/// Longest valid password; login and password change refuse longer input before argon2 runs.
pub const MAX_PASSWORD_CHARS: usize = 128;
const PASSWORD_LEN: std::ops::RangeInclusive<usize> = 1..=MAX_PASSWORD_CHARS;

/// The form a username is stored and compared in: trimmed, NFC. NFC makes a precomposed "á" and
/// "a" + combining acute the same name at login. Invisible and format characters are allowed, so
/// visually identical names (`eva` vs `eva` + U+200B) can still be distinct accounts.
pub fn canonical_username(raw: &str) -> String {
    raw.trim().nfc().collect()
}

/// What makes two names the same account: computed here, not with Postgres `lower()`, whose
/// result depends on the database locale (a `C` locale folds ASCII only). Lowercasing can
/// decompose (`İ` → `i` + dot), hence NFC once more.
pub fn username_key(raw: &str) -> String {
    canonical_username(raw).to_lowercase().nfc().collect()
}

/// A valid username: `name` is stored for display, `key` in `user.username_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username {
    pub name: String,
    pub key: String,
}

/// [`canonical_username`], then 1-64 characters. Anything else goes, inner whitespace included,
/// as typed — except NUL, which a Postgres `text` column cannot store, and CR/LF, which a
/// single-line input drops, so such a name could never be typed at login.
pub fn normalize_username(raw: &str) -> Result<Username, AppError> {
    let name = canonical_username(raw);
    if !USERNAME_LEN.contains(&name.chars().count()) || name.contains(UNSTORABLE) {
        return Err(AppError::validation(
            "username must be 1-64 characters, without NUL or line breaks",
        ));
    }
    let key = username_key(&name);
    Ok(Username { name, key })
}

/// `name` for a plain-text mail or bot message: control, line-separator and bidi characters
/// become `\u{…}` escapes, so a name cannot start a line of its own or reorder the text after it.
/// A backslash is escaped too, so typed escape text never reads as a real control character.
pub fn inline_username(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let escaped = c.is_control()
            || matches!(
                c,
                '\\' | '\u{2028}' | '\u{2029}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
            )
            || ('\u{202A}'..='\u{202E}').contains(&c)
            || ('\u{2066}'..='\u{2069}').contains(&c);
        if escaped {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

/// The account `raw` names, by the same key the unique index enforces. A NUL (never in a stored
/// name) matches nothing rather than reaching Postgres, which refuses it in `text`.
pub fn username_matches(raw: &str) -> SimpleExpr {
    if raw.contains('\0') {
        return Expr::val(false).into();
    }
    user::Column::UsernameKey.eq(username_key(raw))
}

pub fn validate_password(password: &str) -> Result<(), AppError> {
    if PASSWORD_LEN.contains(&password.chars().count()) {
        Ok(())
    } else {
        Err(AppError::validation("password must be 1-128 characters"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_keeps_case_and_is_trimmed() {
        // Inner whitespace stays exactly as typed.
        assert_eq!(
            normalize_username(" Petr  Novák ").expect("valid").name,
            "Petr  Novák"
        );
        let u = normalize_username("  Petr Novák ").expect("valid");
        assert_eq!(u.name, "Petr Novák");
        assert_eq!(u.key, "petr novák");
    }

    #[test]
    fn username_is_free_form() {
        let max = "x".repeat(64);
        for ok in [
            "👨\u{200D}👩\u{200D}👧",
            "Petr  Novák",
            "🦊",
            "a",
            "a@b.cz",
            "dash-name!",
            "\u{0007}",
            "a\tb",
            "\u{200B}",
            "\u{202E}nimda",
            "\u{3164}",
            max.as_str(),
        ] {
            assert!(normalize_username(ok).is_ok(), "{ok:?} should pass");
        }
        // 64 characters, not bytes.
        assert!(normalize_username(&"ř".repeat(64)).is_ok());
    }

    #[test]
    fn username_rejects_empty_too_long_nul_and_line_breaks() {
        let long = "x".repeat(65);
        for bad in [
            "",
            "   ",
            "\u{3000}\u{00A0}",
            long.as_str(),
            "a\0b",
            "\0",
            "a\nb",
            "a\rb",
        ] {
            assert!(normalize_username(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn inline_username_escapes_line_breaks_and_bidi() {
        assert_eq!(inline_username("Petr Novák 🦊"), "Petr Novák 🦊");
        assert_eq!(inline_username("a\u{7}b"), "a\\u{7}b");
        assert_eq!(inline_username("a\\u{7}b"), "a\\u{5c}u{7}b");
        assert_eq!(inline_username("\u{202E}nimda"), "\\u{202e}nimda");
        assert_eq!(
            inline_username("x\u{2028}y\u{2066}"),
            "x\\u{2028}y\\u{2066}"
        );
    }

    #[test]
    fn username_is_nfc() {
        let decomposed = "Nova\u{0301}k";
        assert_eq!(normalize_username(decomposed).expect("valid").name, "Novák");
        assert_eq!(canonical_username(decomposed), canonical_username("Novák"));
    }

    #[test]
    fn key_folds_case_in_every_script() {
        for (a, b) in [
            ("Řeka", "řeka"),
            ("ŽLUŤOUČKÝ", "žluťoučký"),
            ("ΣΟΦΙΑ", "σοφια"),
        ] {
            assert_eq!(username_key(a), username_key(b), "{a} vs {b}");
        }
        assert_eq!(username_key("Nova\u{0301}K"), username_key("novák"));
        assert_ne!(username_key("Petr"), username_key("Petra"));
    }

    #[test]
    fn password_length_bounds() {
        assert!(validate_password("").is_err());
        assert!(validate_password("a").is_ok());
        assert!(validate_password(&"a".repeat(128)).is_ok());
        assert!(validate_password(&"a".repeat(129)).is_err());
    }
}
