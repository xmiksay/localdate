use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTimeWithTimeZone,
    pub is_admin: bool,
    pub banned_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::refresh_token::Entity")]
    RefreshToken,
    #[sea_orm(has_one = "super::profile::Entity")]
    Profile,
    #[sea_orm(has_many = "super::photo::Entity")]
    Photo,
    #[sea_orm(has_one = "super::filter::Entity")]
    Filter,
    #[sea_orm(has_many = "super::visibility_window::Entity")]
    VisibilityWindow,
}

impl Related<super::refresh_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RefreshToken.def()
    }
}
impl Related<super::profile::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Profile.def()
    }
}
impl Related<super::photo::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Photo.def()
    }
}
impl Related<super::filter::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Filter.def()
    }
}
impl Related<super::visibility_window::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::VisibilityWindow.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
