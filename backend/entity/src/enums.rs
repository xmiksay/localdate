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
