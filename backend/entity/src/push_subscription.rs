use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "push_subscription")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    #[sea_orm(unique)]
    pub endpoint: String,
    /// Browser's P-256 public key, base64url.
    pub p256dh: String,
    /// Browser's auth secret, base64url.
    pub auth: String,
    pub user_agent: Option<String>,
    /// `cs` or `en`: language of the notification text for this device.
    pub lang: String,
    pub created_at: DateTimeWithTimeZone,
    pub last_success_at: Option<DateTimeWithTimeZone>,
    /// Consecutive rejected deliveries; reset on success, the row is dropped at 3.
    pub failure_count: i32,
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
