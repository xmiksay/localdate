use sea_orm::entity::prelude::*;

use crate::{EmailTokenPurpose, IdentityProvider};

/// A single-use token sent by mail (or, for password reset, by Telegram); only its sha256 is
/// stored. `user_id` is NULL for sign-up tokens. `email` is the identity subject of `provider`:
/// an address, or a Telegram user id.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "email_token")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub token_hash: String,
    pub purpose: EmailTokenPurpose,
    pub user_id: Option<Uuid>,
    pub email: String,
    pub provider: IdentityProvider,
    pub expires_at: DateTimeWithTimeZone,
    pub used_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
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
