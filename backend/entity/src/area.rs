use super::AreaKind;
use sea_orm::entity::prelude::*;

/// Admin-managed circle around a public place (city centre, station, venue).
/// Area windows reference it, which blocks deleting it (the FK has no delete action).
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "area")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    pub kind: AreaKind,
    pub lat: f64,
    pub lon: f64,
    pub radius_m: i32,
    pub active: bool,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::visibility_window::Entity")]
    VisibilityWindow,
}

impl Related<super::visibility_window::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::VisibilityWindow.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
