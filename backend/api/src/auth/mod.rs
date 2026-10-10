//! Auth: username + password (argon2id), an email magic link or an OAuth provider (Google,
//! Telegram); HS256 access JWT and rotating opaque refresh tokens either way. Forgotten passwords are
//! reset through a linked email or Telegram account.

pub mod email;
pub mod extractor;
pub mod jwt;
pub mod oauth;
pub mod password;
pub mod refresh;
pub mod reset;
mod routes;
pub mod telegram;
pub mod validation;

pub use extractor::{ActingUser, AdminUser, AuthUser};
pub(crate) use routes::{Tokens, session};
pub use routes::{UserDto, router};
