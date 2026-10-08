//! Username + password auth: argon2id, HS256 access JWT, rotating opaque refresh tokens.

pub mod extractor;
pub mod jwt;
pub mod password;
pub mod refresh;
mod routes;
pub mod validation;

pub use extractor::{AdminUser, AuthUser};
pub use routes::{UserDto, router};
