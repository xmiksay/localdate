//! SeaORM entities, one module per table (see docs/architecture/data-model.md).

pub mod admin_audit;
pub mod area;
pub mod block;
pub mod email_token;
pub mod filter;
pub mod interest;
pub mod matches;
pub mod message;
pub mod oauth_grant;
pub mod photo;
pub mod profile;
pub mod push_prefs;
pub mod push_subscription;
pub mod refresh_token;
pub mod report;
pub mod user;
pub mod user_identity;
pub mod user_interest;
pub mod visibility_window;
pub mod wave;

mod enums;
pub use enums::{
    AreaKind, EmailTokenPurpose, Gender, IdentityProvider, OAuthGrantPurpose, Reason, ReportReason,
    ReportResolution, WindowKind,
};
