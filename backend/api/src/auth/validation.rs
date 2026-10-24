use entity::user;
use sea_orm::ColumnTrait;
use sea_orm::sea_query::SimpleExpr;
use unicode_normalization::UnicodeNormalization;

use crate::error::AppError;

const USERNAME_LEN: std::ops::RangeInclusive<usize> = 1..=64;
/// Longest valid password; login and password change refuse longer input before argon2 runs.
pub const MAX_PASSWORD_CHARS: usize = 128;
const PASSWORD_LEN: std::ops::RangeInclusive<usize> = 1..=MAX_PASSWORD_CHARS;

/// The form a username is stored and compared in: trimmed, NFC. NFC makes a precomposed "á" and
/// "a" + combining acute the same name, so look-alikes cannot coexist or miss each other at login.
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
/// as typed.
pub fn normalize_username(raw: &str) -> Result<Username, AppError> {
    let name = canonical_username(raw);
    if !USERNAME_LEN.contains(&name.chars().count()) {
        return Err(AppError::validation("username must be 1-64 characters"));
    }
    let key = username_key(&name);
    Ok(Username { name, key })
}

/// The account `raw` names, by the same key the unique index enforces.
pub fn username_matches(raw: &str) -> SimpleExpr {
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
            "a\nb",
            "\u{0007}",
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
    fn username_rejects_empty_and_too_long() {
        let long = "x".repeat(65);
        for bad in ["", "   ", "\u{3000}\u{00A0}", long.as_str()] {
            assert!(normalize_username(bad).is_err(), "{bad:?} should fail");
        }
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
