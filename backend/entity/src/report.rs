use super::{ReportReason, ReportResolution};
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "report")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub reporter_id: Option<Uuid>,
    pub reported_id: Uuid,
    pub reason: ReportReason,
    pub note: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub resolved_at: Option<DateTimeWithTimeZone>,
    pub resolved_by: Option<Uuid>,
    pub resolution: Option<ReportResolution>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ReporterId",
        to = "super::user::Column::Id",
        on_delete = "SetNull"
    )]
    Reporter,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ReportedId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    Reported,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ResolvedBy",
        to = "super::user::Column::Id",
        on_delete = "SetNull"
    )]
    ResolvedBy,
}

impl ActiveModelBehavior for ActiveModel {}
