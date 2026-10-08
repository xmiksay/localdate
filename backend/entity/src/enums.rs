use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "gender")]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    #[sea_orm(string_value = "male")]
    Male,
    #[sea_orm(string_value = "female")]
    Female,
    #[sea_orm(string_value = "other")]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "reason")]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    #[sea_orm(string_value = "date")]
    Date,
    #[sea_orm(string_value = "meet")]
    Meet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "window_kind")]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    #[sea_orm(string_value = "timed")]
    Timed,
    #[sea_orm(string_value = "area")]
    Area,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "area_kind")]
#[serde(rename_all = "snake_case")]
pub enum AreaKind {
    #[sea_orm(string_value = "city_centre")]
    CityCentre,
    #[sea_orm(string_value = "train_station")]
    TrainStation,
    #[sea_orm(string_value = "venue")]
    Venue,
    #[sea_orm(string_value = "other")]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "report_reason")]
#[serde(rename_all = "snake_case")]
pub enum ReportReason {
    #[sea_orm(string_value = "spam")]
    Spam,
    #[sea_orm(string_value = "harassment")]
    Harassment,
    #[sea_orm(string_value = "fake")]
    Fake,
    #[sea_orm(string_value = "underage")]
    Underage,
    #[sea_orm(string_value = "other")]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "report_resolution")]
#[serde(rename_all = "snake_case")]
pub enum ReportResolution {
    #[sea_orm(string_value = "dismissed")]
    Dismissed,
    #[sea_orm(string_value = "banned")]
    Banned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "identity_provider")]
#[serde(rename_all = "snake_case")]
pub enum IdentityProvider {
    #[sea_orm(string_value = "email")]
    Email,
    #[sea_orm(string_value = "google")]
    Google,
    #[sea_orm(string_value = "telegram")]
    Telegram,
    #[sea_orm(string_value = "facebook")]
    Facebook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "oauth_grant_purpose"
)]
#[serde(rename_all = "snake_case")]
pub enum OAuthGrantPurpose {
    /// Callback code for a linked provider account → session.
    #[sea_orm(string_value = "login")]
    Login,
    /// Callback code for an unknown provider account → sign-up token.
    #[sea_orm(string_value = "signup_code")]
    SignupCode,
    /// Sign-up token → new account once the user picked a username.
    #[sea_orm(string_value = "signup")]
    Signup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "email_token_purpose"
)]
#[serde(rename_all = "snake_case")]
pub enum EmailTokenPurpose {
    #[sea_orm(string_value = "login")]
    Login,
    #[sea_orm(string_value = "link")]
    Link,
    #[sea_orm(string_value = "signup")]
    Signup,
    #[sea_orm(string_value = "password_reset")]
    PasswordReset,
}
