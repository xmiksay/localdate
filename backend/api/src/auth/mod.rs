//! Auth: username + password (argon2id) or an email magic link; HS256 access JWT and rotating
//! opaque refresh tokens either way. Forgotten passwords are reset through a linked email.

pub mod email;
pub mod extractor;
pub mod jwt;
pub mod password;
pub mod refresh;
pub mod reset;
mod routes;
pub mod validation;

pub use extractor::{AdminUser, AuthUser};
pub(crate) use routes::{Tokens, session};
pub use routes::{UserDto, router};
