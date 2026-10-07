//! The mutual-filter rule of docs/architecture.md as a pure function.
//!
//! Production paths evaluate the same rule in SQL (`nearby::SQL`) so it can run over many
//! candidates at once; `tests/nearby.rs` cross-checks the two. Blocks are not modelled here —
//! SQL excludes them separately.

use entity::{Gender, Reason};

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
}

impl Side {
    /// Does this side's filter (gender + age) accept `other`?
    fn accepts(&self, other: &Side) -> bool {
        (self.genders.is_empty() || self.genders.contains(&other.gender))
            && (self.age_min..=self.age_max).contains(&other.age)
    }
}

pub fn mutually_visible(a: &Side, b: &Side, distance_m: f64) -> bool {
    distance_m <= f64::from(a.max_distance_m.min(b.max_distance_m))
        && a.accepts(b)
        && b.accepts(a)
        && a.reasons.iter().any(|r| b.reasons.contains(r))
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
}
