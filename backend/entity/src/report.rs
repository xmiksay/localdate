use super::ReportReason;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "report")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub reported_id: Uuid,
    pub reason: ReportReason,
    pub note: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ReporterId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    Reporter,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ReportedId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    Reported,
}

impl ActiveModelBehavior for ActiveModel {}
