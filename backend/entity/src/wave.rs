use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "wave")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub window_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    /// Equals the sender window's `ends_at`.
    pub expires_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::FromUserId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    FromUser,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ToUserId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    ToUser,
    #[sea_orm(
        belongs_to = "super::visibility_window::Entity",
        from = "Column::WindowId",
        to = "super::visibility_window::Column::Id",
        on_delete = "Cascade"
    )]
    VisibilityWindow,
}

impl Related<super::visibility_window::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::VisibilityWindow.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
