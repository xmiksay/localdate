use sea_orm::entity::prelude::*;

/// Table `match` (reserved word, hence the module name). Invariant: `user_a < user_b`.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "match")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_a: Uuid,
    pub user_b: Uuid,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserA",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    UserA,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserB",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    UserB,
    #[sea_orm(has_many = "super::message::Entity")]
    Message,
}

impl Related<super::message::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Message.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
