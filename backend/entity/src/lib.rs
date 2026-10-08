//! SeaORM entities, one module per table (see docs/architecture.md "Data model").

pub mod area;
pub mod block;
pub mod filter;
pub mod interest;
pub mod matches;
pub mod message;
pub mod photo;
pub mod profile;
pub mod refresh_token;
pub mod report;
pub mod user;
pub mod user_interest;
pub mod visibility_window;
pub mod wave;

mod enums;
pub use enums::{AreaKind, Gender, Reason, ReportReason, ReportResolution, WindowKind};
