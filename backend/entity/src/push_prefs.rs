use sea_orm::entity::prelude::*;

/// Which events may notify the user by push; a missing row means all of them.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "push_prefs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    pub waves: bool,
    pub matches: bool,
    pub messages: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_delete = "Cascade"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
