use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "block")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub blocker_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub blocked_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::BlockerId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    Blocker,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::BlockedId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    Blocked,
}

impl ActiveModelBehavior for ActiveModel {}
