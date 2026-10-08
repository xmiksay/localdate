use crate::error::AppError;

const USERNAME_LEN: std::ops::RangeInclusive<usize> = 3..=32;
/// Longest valid password; login and password change refuse longer input before argon2 runs.
pub const MAX_PASSWORD_CHARS: usize = 128;
const PASSWORD_LEN: std::ops::RangeInclusive<usize> = 10..=MAX_PASSWORD_CHARS;

/// Trim + lowercase, then enforce `[a-z0-9_]{3,32}`.
pub fn normalize_username(raw: &str) -> Result<String, AppError> {
    let name = raw.trim().to_lowercase();
    let valid_chars = name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !valid_chars || !USERNAME_LEN.contains(&name.len()) {
        return Err(AppError::validation(
            "username must be 3-32 characters of a-z, 0-9 and _",
        ));
    }
    Ok(name)
}

pub fn validate_password(password: &str) -> Result<(), AppError> {
    if PASSWORD_LEN.contains(&password.chars().count()) {
        Ok(())
    } else {
        Err(AppError::validation("password must be 10-128 characters"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_is_trimmed_and_lowercased() {
        assert_eq!(normalize_username("  Alice_01 ").unwrap(), "alice_01");
    }

    #[test]
    fn username_rejects_bad_input() {
        for bad in ["ab", "a b c", "ünïcode", "dash-name", &"x".repeat(33), ""] {
            assert!(normalize_username(bad).is_err(), "{bad:?} should fail");
        }
        assert!(normalize_username(&"x".repeat(32)).is_ok());
    }

    #[test]
    fn password_length_bounds() {
        assert!(validate_password(&"a".repeat(9)).is_err());
        assert!(validate_password(&"a".repeat(10)).is_ok());
        assert!(validate_password(&"a".repeat(128)).is_ok());
        assert!(validate_password(&"a".repeat(129)).is_err());
    }
}
