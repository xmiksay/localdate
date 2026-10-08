//! `GET /nearby` and the single SQL definition of "mutually visible" shared with waves.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use entity::{Gender, Reason, interest, matches, photo, user_interest, wave};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveEnum, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder, Statement,
};
use serde::Serialize;
use uuid::Uuid;

use super::geo::{DistanceBand, distance_band};
use super::window;
use crate::auth::AuthUser;
use crate::error::AppError;
use crate::interests::InterestDto;
use crate::me::PhotoDto;
use crate::state::AppState;

/// $1 = viewer, $2 = candidate ids (empty = everyone). Mirrors `rules::mutually_visible` plus
/// blocks, active windows and the photo requirement. The bounding box (viewer's max distance,
/// with an antimeridian-safe longitude test) only narrows candidates; the haversine check decides.
const SQL: &str = r#"
SELECT ow.user_id, ow.starts_at, dist.d AS distance_m, op.display_name,
       op.gender::text AS gender, op.bio, ages.their_age AS age, ofl.reasons::text[] AS reasons
FROM visibility_window mw
JOIN filter mf ON mf.user_id = mw.user_id
JOIN profile mp ON mp.user_id = mw.user_id
JOIN visibility_window ow
  ON ow.user_id <> mw.user_id AND ow.ended_at IS NULL AND ow.ends_at > now()
JOIN filter ofl ON ofl.user_id = ow.user_id
JOIN profile op ON op.user_id = ow.user_id
CROSS JOIN LATERAL (SELECT
    mf.max_distance_m / 110000.0 AS dlat,
    mf.max_distance_m / 110000.0 / GREATEST(cos(radians(mw.lat)), 1e-6) AS dlon) bb
CROSS JOIN LATERAL (SELECT
    2 * 6371000 * asin(sqrt(least(1.0,
        sin(radians(ow.lat - mw.lat) / 2) ^ 2
        + cos(radians(mw.lat)) * cos(radians(ow.lat)) * sin(radians(ow.lon - mw.lon) / 2) ^ 2
    ))) AS d) dist
CROSS JOIN LATERAL (SELECT
    date_part('year', age((now() AT TIME ZONE 'UTC')::date, mp.birth_date))::int AS my_age,
    date_part('year', age((now() AT TIME ZONE 'UTC')::date, op.birth_date))::int AS their_age) ages
WHERE mw.user_id = $1 AND mw.ended_at IS NULL AND mw.ends_at > now()
  AND (cardinality($2::uuid[]) = 0 OR ow.user_id = ANY($2::uuid[]))
  AND ow.lat BETWEEN mw.lat - bb.dlat AND mw.lat + bb.dlat
  AND (abs(ow.lon - mw.lon) <= bb.dlon OR abs(ow.lon - mw.lon) >= 360 - bb.dlon)
  AND dist.d <= LEAST(mf.max_distance_m, ofl.max_distance_m)
  AND (cardinality(mf.genders) = 0 OR op.gender = ANY(mf.genders))
  AND (cardinality(ofl.genders) = 0 OR mp.gender = ANY(ofl.genders))
  AND ages.their_age BETWEEN mf.age_min AND mf.age_max
  AND ages.my_age BETWEEN ofl.age_min AND ofl.age_max
  AND mf.reasons && ofl.reasons
  AND EXISTS (SELECT 1 FROM photo ph WHERE ph.user_id = ow.user_id)
  AND NOT EXISTS (
    SELECT 1 FROM block b
    WHERE (b.blocker_id = mw.user_id AND b.blocked_id = ow.user_id)
       OR (b.blocker_id = ow.user_id AND b.blocked_id = mw.user_id))
"#;

#[derive(Debug, FromQueryResult)]
pub struct NearbyRow {
    pub user_id: Uuid,
    pub starts_at: DateTimeWithTimeZone,
    pub distance_m: f64,
    pub display_name: String,
    pub gender: String,
    pub bio: String,
    pub age: i32,
    pub reasons: Vec<String>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaveState {
    None,
    Sent,
    Received,
    Matched,
}

#[derive(Serialize)]
pub struct NearbyProfile {
    user_id: Uuid,
    display_name: String,
    age: i32,
    gender: Gender,
    bio: String,
    interests: Vec<InterestDto>,
    shared_interests: Vec<i32>,
    photos: Vec<PhotoDto>,
    reasons: Vec<Reason>,
    distance_band: DistanceBand,
    wave_state: WaveState,
    match_id: Option<Uuid>,
}

/// Users the viewer can currently see, restricted to `ids` unless that is empty.
/// Returns nothing when the viewer has no active window.
pub async fn rows(
    db: &impl ConnectionTrait,
    me: Uuid,
    ids: &[Uuid],
) -> Result<Vec<NearbyRow>, AppError> {
    let stmt =
        Statement::from_sql_and_values(DbBackend::Postgres, SQL, [me.into(), ids.to_vec().into()]);
    Ok(NearbyRow::find_by_statement(stmt).all(db).await?)
}

/// Same rows with photos, interests and wave state attached, ordered for display.
pub async fn profiles(
    db: &impl ConnectionTrait,
    me: Uuid,
    rows: Vec<NearbyRow>,
) -> Result<Vec<NearbyProfile>, AppError> {
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<Uuid> = rows.iter().map(|r| r.user_id).collect();
    let now = Utc::now().fixed_offset();

    let mut photos: HashMap<Uuid, Vec<PhotoDto>> = HashMap::new();
    for p in photo::Entity::find()
        .filter(photo::Column::UserId.is_in(ids.clone()))
        .order_by_asc(photo::Column::Position)
        .all(db)
        .await?
    {
        photos.entry(p.user_id).or_default().push(p.into());
    }

    // The viewer rides along in the same query so the overlap needs no extra round trip.
    let links = user_interest::Entity::find()
        .filter(user_interest::Column::UserId.is_in(ids.iter().copied().chain([me])))
        .all(db)
        .await?;
    let mine: BTreeSet<i32> = links
        .iter()
        .filter(|l| l.user_id == me)
        .map(|l| l.interest_id)
        .collect();
    let catalog: HashMap<i32, interest::Model> = interest::Entity::find()
        .filter(interest::Column::Id.is_in(links.iter().map(|l| l.interest_id)))
        .all(db)
        .await?
        .into_iter()
        .map(|i| (i.id, i))
        .collect();
    let mut interests: HashMap<Uuid, Vec<interest::Model>> = HashMap::new();
    for l in links {
        if let Some(i) = catalog.get(&l.interest_id) {
            interests.entry(l.user_id).or_default().push(i.clone());
        }
    }

    let sent: HashSet<Uuid> = wave::Entity::find()
        .filter(wave::Column::FromUserId.eq(me))
        .filter(wave::Column::ToUserId.is_in(ids.clone()))
        .filter(wave::Column::ExpiresAt.gt(now))
        .all(db)
        .await?
        .into_iter()
        .map(|w| w.to_user_id)
        .collect();
    let received: HashSet<Uuid> = wave::Entity::find()
        .filter(wave::Column::ToUserId.eq(me))
        .filter(wave::Column::FromUserId.is_in(ids.clone()))
        .filter(wave::Column::ExpiresAt.gt(now))
        .all(db)
        .await?
        .into_iter()
        .map(|w| w.from_user_id)
        .collect();
    let matched: HashMap<Uuid, Uuid> = matches::Entity::find()
        .filter(
            sea_orm::Condition::any()
                .add(
                    sea_orm::Condition::all()
                        .add(matches::Column::UserA.eq(me))
                        .add(matches::Column::UserB.is_in(ids.clone())),
                )
                .add(
                    sea_orm::Condition::all()
                        .add(matches::Column::UserB.eq(me))
                        .add(matches::Column::UserA.is_in(ids)),
                ),
        )
        .all(db)
        .await?
        .into_iter()
        .map(|m| (if m.user_a == me { m.user_b } else { m.user_a }, m.id))
        .collect();

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let match_id = matched.get(&r.user_id).copied();
        let wave_state = if match_id.is_some() {
            WaveState::Matched
        } else if sent.contains(&r.user_id) {
            WaveState::Sent
        } else if received.contains(&r.user_id) {
            WaveState::Received
        } else {
            WaveState::None
        };
        let theirs = interests.remove(&r.user_id).unwrap_or_default();
        out.push((
            r.starts_at,
            NearbyProfile {
                distance_band: distance_band(r.distance_m),
                gender: Gender::try_from_value(&r.gender)?,
                reasons: r
                    .reasons
                    .iter()
                    .filter_map(|s| Reason::try_from_value(s).ok())
                    .collect(),
                shared_interests: shared_ids(&mine, theirs.iter().map(|i| i.id)),
                interests: theirs.into_iter().map(Into::into).collect(),
                photos: photos.remove(&r.user_id).unwrap_or_default(),
                user_id: r.user_id,
                display_name: r.display_name,
                age: r.age,
                bio: r.bio,
                wave_state,
                match_id,
            },
        ));
    }
    sort_for_display(&mut out);
    Ok(out.into_iter().map(|(_, p)| p).collect())
}

/// Interest ids present on both sides, ascending.
fn shared_ids(mine: &BTreeSet<i32>, theirs: impl Iterator<Item = i32>) -> Vec<i32> {
    let theirs: BTreeSet<i32> = theirs.collect();
    mine.intersection(&theirs).copied().collect()
}

/// Most shared interests first, then nearest band, newest window, and `user_id` for stability.
fn sort_for_display(items: &mut [(DateTimeWithTimeZone, NearbyProfile)]) {
    items.sort_by_key(|(starts_at, p)| {
        (
            Reverse(p.shared_interests.len()),
            p.distance_band,
            Reverse(*starts_at),
            p.user_id,
        )
    });
}

/// Visible profiles restricted to `ids` (empty = all); 409 without an own active window.
pub async fn list(
    db: &impl ConnectionTrait,
    me: Uuid,
    ids: &[Uuid],
) -> Result<Vec<NearbyProfile>, AppError> {
    window::active(db, me)
        .await?
        .ok_or(AppError::NoActiveWindow)?;
    let found = rows(db, me, ids).await?;
    profiles(db, me, found).await
}

pub async fn get_nearby(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<NearbyProfile>>, AppError> {
    Ok(Json(list(&state.db, auth.id, &[]).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        id: u128,
        band: DistanceBand,
        shared: &[i32],
        age_min: i64,
    ) -> (DateTimeWithTimeZone, NearbyProfile) {
        let starts_at = (Utc::now() - chrono::Duration::minutes(age_min)).fixed_offset();
        let p = NearbyProfile {
            user_id: Uuid::from_u128(id),
            display_name: String::new(),
            age: 30,
            gender: Gender::Female,
            bio: String::new(),
            interests: vec![],
            shared_interests: shared.to_vec(),
            photos: vec![],
            reasons: vec![],
            distance_band: band,
            wave_state: WaveState::None,
            match_id: None,
        };
        (starts_at, p)
    }

    #[test]
    fn shared_ids_is_sorted_intersection() {
        let mine = BTreeSet::from([9, 2, 5]);
        assert_eq!(shared_ids(&mine, [5, 1, 9, 9].into_iter()), vec![5, 9]);
        assert!(shared_ids(&mine, [1, 3].into_iter()).is_empty());
        assert!(shared_ids(&BTreeSet::new(), [1].into_iter()).is_empty());
    }

    #[test]
    fn display_order_shared_then_band_then_window_then_id() {
        use DistanceBand::*;
        let mut items = vec![
            item(1, Lt200m, &[], 0),
            item(2, Lt5km, &[1, 2], 0),
            item(3, Lt500m, &[1], 0),
            item(4, Lt200m, &[1], 30),
            item(5, Lt200m, &[1], 5),
            item(7, Lt1km, &[], 10),
            item(6, Lt1km, &[], 10),
        ];
        // Same instant for the last two so only the id can break the tie.
        items[6].0 = items[5].0;
        sort_for_display(&mut items);
        let order: Vec<u128> = items.iter().map(|(_, p)| p.user_id.as_u128()).collect();
        assert_eq!(order, [2, 5, 4, 3, 1, 6, 7]);
    }

    #[test]
    fn wave_state_serializes_snake_case() {
        let json = serde_json::to_string(&[WaveState::None, WaveState::Matched]).expect("json");
        assert_eq!(json, r#"["none","matched"]"#);
    }
}
