use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "interest")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub key: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        super::user_interest::Relation::User.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::user_interest::Relation::Interest.def().rev())
    }
}

impl ActiveModelBehavior for ActiveModel {}
