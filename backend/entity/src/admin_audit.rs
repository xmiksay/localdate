use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "admin_audit")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub admin_id: Option<Uuid>,
    pub target_user_id: Option<Uuid>,
    /// `impersonate` | `impersonated_request` (checked by the table).
    pub action: String,
    pub meta: Json,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::AdminId",
        to = "super::user::Column::Id",
        on_delete = "SetNull"
    )]
    Admin,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::TargetUserId",
        to = "super::user::Column::Id",
        on_delete = "SetNull"
    )]
    Target,
}

impl ActiveModelBehavior for ActiveModel {}
