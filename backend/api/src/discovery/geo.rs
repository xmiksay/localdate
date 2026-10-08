//! Pure geometry helpers: coordinate rounding, haversine distance, distance bands.

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Same radius the SQL haversine in `nearby.rs` uses.
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Coordinates are stored at ~100 m precision.
pub fn round_coord(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

pub fn valid_coords(lat: f64, lon: f64) -> bool {
    (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

/// [`valid_coords`] as the contract's `400 validation`.
pub fn ensure_valid(lat: f64, lon: f64) -> Result<(), AppError> {
    if valid_coords(lat, lon) {
        Ok(())
    } else {
        Err(AppError::validation(
            "lat must be within -90..90 and lon within -180..180",
        ))
    }
}

pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
}

/// An area window survives at least this far past the circle, so GPS jitter at the edge does not
/// end it.
pub const AREA_EXIT_MARGIN_M: f64 = 100.0;
/// A worse reported accuracy is not trusted further: past this a fix is too vague to keep anyone in.
pub const MAX_ACCURACY_M: f64 = 500.0;

/// Leave margin for a fix with the reported `accuracy_m` (`None` = not reported).
pub fn exit_margin(accuracy_m: Option<f64>) -> f64 {
    accuracy_m.map_or(AREA_EXIT_MARGIN_M, |a| {
        a.clamp(AREA_EXIT_MARGIN_M, MAX_ACCURACY_M)
    })
}

/// A circle on the globe: an area's centre and radius.
#[derive(Debug, Clone, Copy)]
pub struct Circle {
    pub lat: f64,
    pub lon: f64,
    pub radius_m: f64,
}

impl Circle {
    pub fn distance_m(&self, lat: f64, lon: f64) -> f64 {
        haversine_m(self.lat, self.lon, lat, lon)
    }

    /// Boundary inclusive: where an area window may start and which areas `/areas` lists.
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        self.distance_m(lat, lon) <= self.radius_m
    }

    /// Where a running area window ends: beyond the radius plus `margin_m` ([`exit_margin`]).
    pub fn left(&self, lat: f64, lon: f64, margin_m: f64) -> bool {
        self.distance_m(lat, lon) > self.radius_m + margin_m
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DistanceBand {
    #[serde(rename = "lt_200m")]
    Lt200m,
    #[serde(rename = "lt_500m")]
    Lt500m,
    #[serde(rename = "lt_1km")]
    Lt1km,
    #[serde(rename = "lt_2km")]
    Lt2km,
    #[serde(rename = "lt_5km")]
    Lt5km,
    #[serde(rename = "lt_10km")]
    Lt10km,
}

/// Smallest band whose limit the distance is below; max filter distance is 10 km inclusive,
/// so anything at or past it still lands in the last band.
pub fn distance_band(distance_m: f64) -> DistanceBand {
    match distance_m {
        d if d < 200.0 => DistanceBand::Lt200m,
        d if d < 500.0 => DistanceBand::Lt500m,
        d if d < 1000.0 => DistanceBand::Lt1km,
        d if d < 2000.0 => DistanceBand::Lt2km,
        d if d < 5000.0 => DistanceBand::Lt5km,
        _ => DistanceBand::Lt10km,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_boundaries() {
        let cases = [
            (0.0, DistanceBand::Lt200m),
            (199.9, DistanceBand::Lt200m),
            (200.0, DistanceBand::Lt500m),
            (499.9, DistanceBand::Lt500m),
            (500.0, DistanceBand::Lt1km),
            (999.9, DistanceBand::Lt1km),
            (1000.0, DistanceBand::Lt2km),
            (1999.9, DistanceBand::Lt2km),
            (2000.0, DistanceBand::Lt5km),
            (4999.9, DistanceBand::Lt5km),
            (5000.0, DistanceBand::Lt10km),
            (9999.9, DistanceBand::Lt10km),
            (10_000.0, DistanceBand::Lt10km),
        ];
        for (d, band) in cases {
            assert_eq!(distance_band(d), band, "{d} m");
        }
    }

    #[test]
    fn bands_serialize_as_contract_strings() {
        let json = serde_json::to_string(&DistanceBand::Lt1km).expect("serialize");
        assert_eq!(json, "\"lt_1km\"");
        assert!(DistanceBand::Lt200m < DistanceBand::Lt10km);
    }

    #[test]
    fn haversine_known_city_pair() {
        // Prague -> Vienna is about 252 km.
        let d = haversine_m(50.0755, 14.4378, 48.2082, 16.3738);
        assert!((d - 252_000.0).abs() < 3_000.0, "{d}");
        assert_eq!(haversine_m(10.0, 10.0, 10.0, 10.0), 0.0);
    }

    #[test]
    fn haversine_is_symmetric_and_handles_antipodes() {
        let ab = haversine_m(10.0, 20.0, -30.0, 70.0);
        let ba = haversine_m(-30.0, 70.0, 10.0, 20.0);
        assert!((ab - ba).abs() < 1e-6);
        let half = std::f64::consts::PI * EARTH_RADIUS_M;
        assert!((haversine_m(0.0, 0.0, 0.0, 180.0) - half).abs() < 1.0);
    }

    #[test]
    fn rounds_to_three_decimals() {
        assert_eq!(round_coord(50.123_456), 50.123);
        assert_eq!(round_coord(-14.4378), -14.438);
        assert_eq!(round_coord(0.0004), 0.0);
    }

    /// One metre north of `lat`, in degrees.
    const M_LAT: f64 = 1.0 / 111_194.93;

    fn station() -> Circle {
        Circle {
            lat: 50.083,
            lon: 14.435,
            radius_m: 300.0,
        }
    }

    #[test]
    fn circle_contains_its_boundary_and_not_beyond() {
        let c = station();
        assert!(c.contains(c.lat, c.lon));
        assert!(c.contains(c.lat + 299.0 * M_LAT, c.lon));
        assert!(!c.contains(c.lat + 301.0 * M_LAT, c.lon));
        assert!(!c.contains(c.lat, c.lon + 0.01));
    }

    #[test]
    fn leaving_needs_the_hysteresis_margin() {
        let c = station();
        // Just outside the circle: cannot start here, but a running window stays.
        let edge = c.lat + 350.0 * M_LAT;
        let m = AREA_EXIT_MARGIN_M;
        assert!(!c.contains(edge, c.lon));
        assert!(!c.left(edge, c.lon, m));
        assert!(!c.left(c.lat + 399.0 * M_LAT, c.lon, m));
        assert!(c.left(c.lat + 401.0 * M_LAT, c.lon, m));
        assert!(c.left(-c.lat, c.lon, m));
        assert!(!c.left(c.lat + 401.0 * M_LAT, c.lon, 200.0));
    }

    #[test]
    fn margin_follows_accuracy_between_floor_and_cap() {
        assert_eq!(exit_margin(None), 100.0);
        assert_eq!(exit_margin(Some(0.0)), 100.0);
        assert_eq!(exit_margin(Some(99.9)), 100.0);
        assert_eq!(exit_margin(Some(250.0)), 250.0);
        assert_eq!(exit_margin(Some(500.0)), 500.0);
        assert_eq!(exit_margin(Some(5000.0)), 500.0);
    }

    #[test]
    fn ensure_valid_reports_validation() {
        assert!(ensure_valid(50.0, 14.0).is_ok());
        assert!(matches!(
            ensure_valid(f64::NAN, 0.0),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn coordinate_ranges() {
        assert!(valid_coords(90.0, 180.0));
        assert!(valid_coords(-90.0, -180.0));
        assert!(!valid_coords(90.1, 0.0));
        assert!(!valid_coords(0.0, -180.1));
        assert!(!valid_coords(f64::NAN, 0.0));
    }
}
