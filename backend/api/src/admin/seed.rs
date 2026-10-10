//! `localdate-api admin seed-test-users --count N`: plausible Czech test profiles in bulk.

use anyhow::{Context, Result, bail};
use chrono::{Days, NaiveDate, Utc};
use entity::Gender;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use super::test_users::{self, NewTestUser};
use crate::error::AppError;
use crate::media::PhotoStore;

pub const MAX_COUNT: u32 = 200;
const AGES: std::ops::RangeInclusive<u64> = 18..=45;
const INTEREST_IDS: std::ops::RangeInclusive<i32> = 1..=40;
/// Retries when a generated username is already taken.
const NAME_ATTEMPTS: usize = 5;

/// (display name, ASCII username stem)
const FEMALE: &[(&str, &str)] = &[
    ("Jana", "jana"),
    ("Tereza", "tereza"),
    ("Eliška", "eliska"),
    ("Anna", "anna"),
    ("Kateřina", "katerina"),
    ("Lucie", "lucie"),
    ("Veronika", "veronika"),
    ("Barbora", "barbora"),
    ("Markéta", "marketa"),
    ("Klára", "klara"),
    ("Adéla", "adela"),
    ("Zuzana", "zuzana"),
];
const MALE: &[(&str, &str)] = &[
    ("Jan", "jan"),
    ("Jakub", "jakub"),
    ("Tomáš", "tomas"),
    ("Lukáš", "lukas"),
    ("Petr", "petr"),
    ("Martin", "martin"),
    ("Ondřej", "ondrej"),
    ("Jiří", "jiri"),
    ("Vojtěch", "vojtech"),
    ("Matěj", "matej"),
    ("Filip", "filip"),
    ("David", "david"),
];
const OTHER: &[(&str, &str)] = &[
    ("Alex", "alex"),
    ("Saša", "sasa"),
    ("Robin", "robin"),
    ("Nikola", "nikola"),
];
const BIOS: &[&str] = &[
    "Ráda chodím po horách a piju dobrou kávu.",
    "Víkendy trávím na kole, večery u knížky.",
    "Hledám parťáka na koncerty a festivaly.",
    "Vařím, fotím a občas i běhám.",
    "Nový ve městě, rád poznám lidi kolem.",
    "Deskovky, pivo a dlouhé procházky.",
    "Testovací profil — ahoj!",
    "",
];

/// xorshift64*: plenty for picking names, and seedable so tests are repeatable.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Zero is xorshift's fixed point.
        Self(seed.max(1))
    }

    pub fn from_entropy() -> Self {
        Self::new(Uuid::new_v4().as_u128() as u64)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-enough integer in `range` (inclusive).
    fn within(&mut self, range: std::ops::RangeInclusive<u64>) -> u64 {
        range.start() + self.next() % (range.end() - range.start() + 1)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.within(0..=(items.len() as u64 - 1)) as usize]
    }
}

/// A random profile, born `AGES` years before `today`.
pub fn profile(rng: &mut Rng, today: NaiveDate) -> NewTestUser {
    let (gender, names) = match rng.within(0..=9) {
        0..=4 => (Gender::Female, FEMALE),
        5..=8 => (Gender::Male, MALE),
        _ => (Gender::Other, OTHER),
    };
    let (display, stem) = *rng.pick(names);
    // 365 days per year plus 10 covers the leap days, so the age is at least `age` and stays
    // below `age + 1` with at most 340 more days.
    let age = rng.within(AGES);
    let days_back = age * 365 + 10 + rng.within(0..=340);
    let birth_date = today
        .checked_sub_days(Days::new(days_back))
        .unwrap_or(today);
    let mut interest_ids: Vec<i32> = Vec::new();
    let wanted = rng.within(3..=6) as usize;
    while interest_ids.len() < wanted {
        let span = (INTEREST_IDS.end() - INTEREST_IDS.start()) as u64;
        let id = INTEREST_IDS.start() + rng.within(0..=span) as i32;
        if !interest_ids.contains(&id) {
            interest_ids.push(id);
        }
    }
    interest_ids.sort_unstable();
    NewTestUser {
        username: format!("test_{stem}_{:04}", rng.within(0..=9999)),
        display_name: display.to_owned(),
        gender,
        birth_date,
        bio: (*rng.pick(BIOS)).to_owned(),
        interest_ids,
        placeholder_photo: true,
    }
}

/// Creates `count` onboarded test users with placeholder avatars; returns their usernames.
pub async fn seed_test_users(
    db: &DatabaseConnection,
    store: &PhotoStore,
    count: u32,
    rng: &mut Rng,
) -> Result<Vec<String>> {
    if !(1..=MAX_COUNT).contains(&count) {
        bail!("count must be 1-{MAX_COUNT}");
    }
    let today = Utc::now().date_naive();
    let mut created = Vec::new();
    for _ in 0..count {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let new = profile(rng, today);
            let username = new.username.clone();
            let photo = test_users::placeholder_webp(rng.next()).await?;
            match test_users::create(db, store, new, Some(photo)).await {
                Ok(_) => {
                    created.push(username);
                    break;
                }
                Err(AppError::UsernameTaken) if attempt < NAME_ATTEMPTS => {}
                Err(e) => {
                    return Err(anyhow::Error::new(e))
                        .with_context(|| format!("creating test user {username}"));
                }
            }
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use crate::auth::validation::normalize_username;
    use crate::me::profile::age_on;

    use super::*;

    #[test]
    fn profiles_are_valid_and_varied() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 10).expect("date");
        let mut rng = Rng::new(42);
        let mut genders = Vec::new();
        for _ in 0..300 {
            let p = profile(&mut rng, today);
            assert_eq!(
                normalize_username(&p.username)
                    .ok()
                    .map(|u| u.name)
                    .as_deref(),
                Some(p.username.as_str())
            );
            let age = age_on(p.birth_date, today);
            assert!(AGES.contains(&(age as u64)), "age {age}");
            assert!((3..=6).contains(&p.interest_ids.len()));
            assert!(p.interest_ids.windows(2).all(|w| w[0] < w[1]), "distinct");
            assert!(p.interest_ids.iter().all(|i| INTEREST_IDS.contains(i)));
            if !genders.contains(&p.gender) {
                genders.push(p.gender);
            }
        }
        assert_eq!(genders.len(), 3, "every gender shows up");
    }

    #[test]
    fn same_seed_same_profiles() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).expect("date");
        let a = profile(&mut Rng::new(7), today);
        let b = profile(&mut Rng::new(7), today);
        assert_eq!((a.username, a.birth_date), (b.username, b.birth_date));
    }

    #[test]
    fn zero_seed_still_moves() {
        let mut rng = Rng::new(0);
        assert_ne!(rng.next(), rng.next());
    }
}
