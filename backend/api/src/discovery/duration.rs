//! How long a new window runs: a preset number of minutes, or until the user's local midnight.

use chrono::{DateTime, Duration, LocalResult, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Deserialize;

use crate::error::AppError;

const MINUTES: [i64; 4] = [30, 60, 120, 240];
/// No window ever reaches further than this from now, whichever way it was started or extended.
/// This, not the presets, is the invariant: end of day yields arbitrary lengths up to it.
pub const MAX_AHEAD: Duration = Duration::hours(12);
/// An end-of-day window shorter than the shortest preset is not worth opening.
const MIN_UNTIL_MIDNIGHT: Duration = Duration::minutes(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Until {
    EndOfDay,
}

pub(crate) fn check_minutes(field: &str, minutes: i64) -> Result<Duration, AppError> {
    if MINUTES.contains(&minutes) {
        Ok(Duration::minutes(minutes))
    } else {
        Err(AppError::validation(format!(
            "{field} must be 30, 60, 120 or 240"
        )))
    }
}

/// A new window's validated length; turned into `ends_at` only once the start time is known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Length {
    Fixed(Duration),
    EndOfDay(Tz),
}

impl Length {
    /// Exactly one of `minutes` or `until` (+ `tz`) is allowed.
    pub(super) fn requested(
        minutes: Option<i64>,
        until: Option<Until>,
        tz: Option<&str>,
    ) -> Result<Self, AppError> {
        match (minutes, until, tz) {
            (Some(m), None, None) => Ok(Self::Fixed(check_minutes("minutes", m)?)),
            (None, Some(Until::EndOfDay), Some(tz)) => Ok(Self::EndOfDay(parse_tz(tz)?)),
            (Some(_), Some(_), _) => Err(AppError::validation(
                "send either minutes or until, not both",
            )),
            (None, None, _) => Err(AppError::validation("minutes or until is required")),
            (None, Some(_), None) => Err(AppError::validation("tz is required with until")),
            (Some(_), None, Some(_)) => Err(AppError::validation("tz is only allowed with until")),
        }
    }

    pub(super) fn ends_at(self, now: DateTime<Utc>) -> Result<DateTime<Utc>, AppError> {
        match self {
            Self::Fixed(length) => Ok(now + length),
            Self::EndOfDay(tz) => end_of_day(now, tz),
        }
    }
}

fn parse_tz(raw: &str) -> Result<Tz, AppError> {
    raw.parse()
        .map_err(|_| AppError::validation("tz must be an IANA time zone name"))
}

/// The next local midnight in `tz`, capped at [`MAX_AHEAD`]; refused when it is under 30 min away.
fn end_of_day(now: DateTime<Utc>, tz: Tz) -> Result<DateTime<Utc>, AppError> {
    let cap = now + MAX_AHEAD;
    // `None` only past chrono's last representable date; the cap is the sane answer there.
    let Some(midnight) = next_local_midnight(now, tz) else {
        return Ok(cap);
    };
    if midnight - now < MIN_UNTIL_MIDNIGHT {
        return Err(AppError::TooCloseToMidnight);
    }
    Ok(midnight.min(cap))
}

/// The first instant of the next local day. A midnight skipped by a DST jump (or, as in Samoa in
/// 2011, a whole skipped day) becomes the first valid local time after it; an ambiguous one its first
/// occurrence after `now`.
pub fn next_local_midnight(now: DateTime<Utc>, tz: Tz) -> Option<DateTime<Utc>> {
    let midnight = now
        .with_timezone(&tz)
        .date_naive()
        .succ_opt()?
        .and_time(NaiveTime::MIN);
    (0..=24 * 60).find_map(|m| {
        let candidates = match tz.from_local_datetime(&(midnight + Duration::minutes(m))) {
            LocalResult::Single(t) => [Some(t), None],
            LocalResult::Ambiguous(first, second) => [Some(first), Some(second)],
            LocalResult::None => [None, None],
        };
        candidates
            .into_iter()
            .flatten()
            .map(|t| t.to_utc())
            .find(|t| *t > now)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().expect("valid RFC 3339 literal")
    }

    fn tz(name: &str) -> Tz {
        name.parse().expect("known zone")
    }

    #[test]
    fn minutes_are_limited_to_presets() {
        for m in MINUTES {
            assert!(check_minutes("minutes", m).is_ok());
        }
        for m in [0, 45, -30, 480] {
            assert!(matches!(
                check_minutes("minutes", m),
                Err(AppError::Validation(_))
            ));
        }
    }

    #[test]
    fn midnight_on_an_ordinary_day() {
        // 20:00 CEST → 00:00 CEST next day.
        let now = utc("2026-06-01T18:00:00Z");
        let next = next_local_midnight(now, tz("Europe/Prague"));
        assert_eq!(next, Some(utc("2026-06-01T22:00:00Z")));
        assert_eq!(
            end_of_day(now, tz("Europe/Prague")).ok(),
            Some(utc("2026-06-01T22:00:00Z"))
        );
    }

    #[test]
    fn midnight_after_spring_forward_in_prague() {
        // 2026-03-29 has 23 h: 01:00 CET → next midnight 00:00 CEST, 22 h later.
        let now = utc("2026-03-29T00:00:00Z");
        assert_eq!(
            next_local_midnight(now, tz("Europe/Prague")),
            Some(utc("2026-03-29T22:00:00Z"))
        );
    }

    #[test]
    fn midnight_after_fall_back_in_prague() {
        // 2026-10-25 has 25 h: 00:00 CEST → next midnight 00:00 CET, 25 h later.
        let now = utc("2026-10-24T22:00:00Z");
        assert_eq!(
            next_local_midnight(now, tz("Europe/Prague")),
            Some(utc("2026-10-25T23:00:00Z"))
        );
    }

    #[test]
    fn midnight_skipped_by_dst_becomes_the_next_valid_instant() {
        // Chile springs forward at 24:00: 2026-09-06 00:00 never happens, 01:00 -03 is the first instant.
        let now = utc("2026-09-05T23:00:00Z");
        assert_eq!(
            next_local_midnight(now, tz("America/Santiago")),
            Some(utc("2026-09-06T04:00:00Z"))
        );
        // Samoa skipped all of 2011-12-30: the next day to start was the 31st (+14).
        let now = utc("2011-12-29T22:00:00Z");
        assert_eq!(
            next_local_midnight(now, tz("Pacific/Apia")),
            Some(utc("2011-12-30T10:00:00Z"))
        );
    }

    #[test]
    fn ambiguous_midnight_takes_the_earlier_instant() {
        // Havana falls back 01:00 CDT → 00:00 CST: 2026-11-01 00:00 happens twice, first at -04.
        let now = utc("2026-11-01T00:00:00Z");
        assert_eq!(
            next_local_midnight(now, tz("America/Havana")),
            Some(utc("2026-11-01T04:00:00Z"))
        );
    }

    #[test]
    fn midnight_in_a_far_east_zone() {
        // Kiritimati is UTC+14: 12:00Z is already 02:00 of the next local day.
        let now = utc("2026-10-08T12:00:00Z");
        assert_eq!(
            next_local_midnight(now, tz("Pacific/Kiritimati")),
            Some(utc("2026-10-09T10:00:00Z"))
        );
    }

    #[test]
    fn end_of_day_is_capped_at_twelve_hours() {
        let now = utc("2026-06-01T06:00:00Z");
        assert_eq!(
            end_of_day(now, tz("Europe/Prague")).ok(),
            Some(now + MAX_AHEAD)
        );
    }

    #[test]
    fn end_of_day_needs_thirty_minutes_left() {
        let prague = tz("Europe/Prague");
        // 23:31 CEST: 29 min to go.
        assert!(matches!(
            end_of_day(utc("2026-06-01T21:31:00Z"), prague),
            Err(AppError::TooCloseToMidnight)
        ));
        // 23:30 CEST: exactly the shortest preset is still fine.
        assert_eq!(
            end_of_day(utc("2026-06-01T21:30:00Z"), prague).ok(),
            Some(utc("2026-06-01T22:00:00Z"))
        );
    }

    #[test]
    fn exactly_one_duration_choice() {
        let now = utc("2026-06-01T18:00:00Z");
        let eod = Some(Until::EndOfDay);
        let fixed = Length::requested(Some(60), None, None);
        assert_eq!(
            fixed.as_ref().ok(),
            Some(&Length::Fixed(Duration::hours(1)))
        );
        assert_eq!(
            fixed.and_then(|l| l.ends_at(now)).ok(),
            Some(now + Duration::hours(1))
        );
        let end_of_day = Length::requested(None, eod, Some("Europe/Prague"));
        assert_eq!(
            end_of_day.as_ref().ok(),
            Some(&Length::EndOfDay(tz("Europe/Prague")))
        );
        assert_eq!(
            end_of_day.and_then(|l| l.ends_at(now)).ok(),
            Some(utc("2026-06-01T22:00:00Z"))
        );
        for (minutes, until, tz) in [
            (None, None, None),
            (Some(60), eod, Some("Europe/Prague")),
            (None, eod, None),
            (Some(60), None, Some("Europe/Prague")),
            (None, eod, Some("Mars/Olympus_Mons")),
            (None, eod, Some("")),
            (Some(45), None, None),
        ] {
            assert!(
                matches!(
                    Length::requested(minutes, until, tz),
                    Err(AppError::Validation(_))
                ),
                "{minutes:?} {until:?} {tz:?}"
            );
        }
    }
}
