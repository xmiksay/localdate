use super::WindowKind;
use sea_orm::entity::prelude::*;

/// Active = `ended_at IS NULL AND ends_at > now()`. The DB only guarantees one *open*
/// (`ended_at IS NULL`) row per user, so writers must set `ended_at` when replacing or
/// expiring a window.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "visibility_window")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: WindowKind,
    pub lat: f64,
    pub lon: f64,
    pub location_updated_at: DateTimeWithTimeZone,
    pub starts_at: DateTimeWithTimeZone,
    pub ends_at: DateTimeWithTimeZone,
    pub ended_at: Option<DateTimeWithTimeZone>,
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
    #[sea_orm(has_many = "super::wave::Entity")]
    Wave,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}
impl Related<super::wave::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Wave.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
