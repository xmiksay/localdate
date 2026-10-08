use sea_orm::entity::prelude::*;

use crate::{IdentityProvider, OAuthGrantPurpose};

/// A one-time OAuth callback code or a pending sign-up token; only its sha256 is stored.
/// `user_id` is set for `login` only; `binding` (sha256 of the flow state) for codes only;
/// `photo` (an imported profile picture, already WebP) for sign-up grants only.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_grant")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub token_hash: String,
    pub purpose: OAuthGrantPurpose,
    pub provider: IdentityProvider,
    pub subject: String,
    pub user_id: Option<Uuid>,
    pub binding: Option<String>,
    pub expires_at: DateTimeWithTimeZone,
    pub used_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub photo: Option<Vec<u8>>,
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
