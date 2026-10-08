//! The mutual-filter rule of docs/architecture.md as a pure function.
//!
//! Production paths evaluate the same rule in SQL (`nearby::SQL`) so it can run over many
//! candidates at once; `tests/visibility_agreement.rs` cross-checks the two. Blocks are not
//! modelled here — SQL excludes them separately.

use entity::{Gender, Reason};
use uuid::Uuid;

/// An area window whose location is older than this is stale: it neither sees nor is seen. The
/// client reports at least every 2 min, so this only catches a phone that stopped reporting.
pub const AREA_STALE_SECS: i64 = 600;

/// One user's side of the match: who they are and what they filter for.
#[derive(Debug, Clone)]
pub struct Side {
    pub gender: Gender,
    pub age: i32,
    pub max_distance_m: i32,
    /// Empty = any.
    pub genders: Vec<Gender>,
    pub age_min: i32,
    pub age_max: i32,
    pub reasons: Vec<Reason>,
    /// The area of an area window; `None` for a timed window.
    pub area: Option<Uuid>,
    /// Location older than [`AREA_STALE_SECS`]; only matters for area windows.
    pub stale: bool,
}

impl Side {
    /// Does this side's filter (gender + age) accept `other`?
    fn accepts(&self, other: &Side) -> bool {
        (self.genders.is_empty() || self.genders.contains(&other.gender))
            && (self.age_min..=self.age_max).contains(&other.age)
    }
}

/// Area windows meet only fresh area windows in the same area (the area replaces distance); timed
/// windows meet only timed windows within both max distances.
pub fn mutually_visible(a: &Side, b: &Side, distance_m: f64) -> bool {
    let close = match (a.area, b.area) {
        (None, None) => distance_m <= f64::from(a.max_distance_m.min(b.max_distance_m)),
        (Some(x), Some(y)) => x == y && !a.stale && !b.stale,
        _ => false,
    };
    close && a.accepts(b) && b.accepts(a) && a.reasons.iter().any(|r| b.reasons.contains(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side() -> Side {
        Side {
            gender: Gender::Female,
            age: 30,
            max_distance_m: 2000,
            genders: vec![],
            age_min: 18,
            age_max: 99,
            reasons: vec![Reason::Date, Reason::Meet],
            area: None,
            stale: false,
        }
    }

    fn with(edit: impl FnOnce(&mut Side)) -> Side {
        let mut s = side();
        edit(&mut s);
        s
    }

    fn visible(a: &Side, b: &Side, d: f64) -> bool {
        let ab = mutually_visible(a, b, d);
        assert_eq!(ab, mutually_visible(b, a, d), "rule must be symmetric");
        ab
    }

    #[test]
    fn defaults_see_each_other() {
        assert!(visible(&side(), &side(), 300.0));
    }

    #[test]
    fn distance_uses_the_smaller_max_inclusive() {
        let small = with(|s| s.max_distance_m = 500);
        assert!(visible(&side(), &small, 500.0));
        assert!(!visible(&side(), &small, 500.1));
        assert!(!visible(&small, &side(), 1500.0));
    }

    #[test]
    fn gender_filter_applies_in_both_directions() {
        let wants_male = with(|s| s.genders = vec![Gender::Male]);
        let male = with(|s| s.gender = Gender::Male);
        // Female wants male, male accepts anyone: visible.
        assert!(visible(&wants_male, &male, 100.0));
        // The other female is rejected by wants_male.
        assert!(!visible(&wants_male, &side(), 100.0));
        // One-sided rejection hides both: male only wants male.
        let male_wants_male = with(|s| {
            s.gender = Gender::Male;
            s.genders = vec![Gender::Male];
        });
        assert!(!visible(&wants_male, &male_wants_male, 100.0));
    }

    #[test]
    fn empty_genders_means_any_and_lists_may_hold_several() {
        let any = side();
        let other = with(|s| s.gender = Gender::Other);
        assert!(visible(&any, &other, 100.0));
        let two = with(|s| s.genders = vec![Gender::Male, Gender::Other]);
        assert!(visible(&two, &other, 100.0));
    }

    #[test]
    fn age_bounds_are_inclusive_and_checked_both_ways() {
        let picky = with(|s| {
            s.age_min = 25;
            s.age_max = 35;
        });
        for (age, ok) in [(24, false), (25, true), (35, true), (36, false)] {
            let other = with(|s| s.age = age);
            assert_eq!(visible(&picky, &other, 100.0), ok, "age {age}");
        }
        // The reverse direction: other's own bounds exclude picky (age 30).
        let young_only = with(|s| {
            s.age = 20;
            s.age_max = 29;
        });
        assert!(!visible(&picky, &young_only, 100.0));
    }

    #[test]
    fn one_shared_reason_is_enough() {
        let date = with(|s| s.reasons = vec![Reason::Date]);
        let meet = with(|s| s.reasons = vec![Reason::Meet]);
        assert!(!visible(&date, &meet, 100.0));
        assert!(visible(&date, &side(), 100.0));
        assert!(visible(&meet, &side(), 100.0));
    }

    #[test]
    fn same_area_ignores_distance_other_area_and_timed_do_not_meet() {
        let here = Some(Uuid::from_u128(1));
        let in_area = with(|s| s.area = here);
        let small = with(|s| {
            s.area = here;
            s.max_distance_m = 200;
        });
        assert!(visible(&in_area, &small, 4000.0));
        let elsewhere = with(|s| s.area = Some(Uuid::from_u128(2)));
        assert!(!visible(&in_area, &elsewhere, 10.0));
        assert!(!visible(&in_area, &side(), 10.0));
        // Filters still apply inside an area.
        let date = with(|s| {
            s.area = here;
            s.reasons = vec![Reason::Date];
        });
        let meet = with(|s| {
            s.area = here;
            s.reasons = vec![Reason::Meet];
        });
        assert!(!visible(&date, &meet, 10.0));
    }

    #[test]
    fn a_stale_area_window_neither_sees_nor_is_seen() {
        let here = Some(Uuid::from_u128(1));
        let fresh = with(|s| s.area = here);
        let stale = with(|s| {
            s.area = here;
            s.stale = true;
        });
        assert!(!visible(&fresh, &stale, 10.0));
        assert!(!visible(&stale, &stale, 10.0));
        // Timed windows have no staleness rule.
        let old_timed = with(|s| s.stale = true);
        assert!(visible(&side(), &old_timed, 10.0));
    }
}
