use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

struct Rule {
    max: usize,
    period: Duration,
}

/// Per (address, client IP): someone else hammering an address cannot lock its owner out.
const PER_SENDER: Rule = Rule {
    max: 3,
    period: Duration::from_secs(15 * 60),
};
/// Per address from everywhere: bounds mail-bombing one inbox from many IPs.
const PER_ADDRESS: Rule = Rule {
    max: 10,
    period: Duration::from_secs(60 * 60),
};
/// Reset requests per (username or address as typed, client IP).
const RESET_PER_SENDER: Rule = PER_SENDER;
/// Reset mails per account, however it was asked for.
const RESET_PER_ACCOUNT: Rule = Rule {
    max: 5,
    period: Duration::from_secs(60 * 60),
};
/// No rule is longer; older entries can be pruned.
const LONGEST: Duration = Duration::from_secs(60 * 60);
const PRUNE_ABOVE: usize = 10_000;

/// Sliding-window counters by key. In memory and per replica, like the IP limiter: N replicas
/// allow N× the mails, and a restart grants a few extra — both harmless.
#[derive(Clone, Default)]
struct Windows {
    sent: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

fn recent(times: &mut VecDeque<Instant>, rule: &Rule, now: Instant) -> usize {
    while times
        .front()
        .is_some_and(|t| now.saturating_duration_since(*t) >= rule.period)
    {
        times.pop_front();
    }
    times.len()
}

fn sender_key(key: &str, ip: Option<IpAddr>) -> String {
    match ip {
        Some(ip) => format!("{key}|{ip}"),
        None => format!("{key}|-"),
    }
}

impl Windows {
    /// Whether every key has room under its rule; records in all of them only when allowed, so a
    /// refused request does not use up any budget.
    fn allow(&self, checks: &[(String, &Rule)], now: Instant) -> bool {
        let mut map = self.sent.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > PRUNE_ABOVE {
            map.retain(|_, t| {
                t.back()
                    .is_some_and(|t| now.saturating_duration_since(*t) < LONGEST)
            });
        }
        let ok = checks
            .iter()
            .all(|(key, rule)| recent(map.entry(key.clone()).or_default(), rule, now) < rule.max);
        if ok {
            for (key, _) in checks {
                map.entry(key.clone()).or_default().push_back(now);
            }
        }
        ok
    }
}

/// Budget of the magic-link mails (`/auth/email/start`, `POST /me/identities/email`).
#[derive(Clone, Default)]
pub struct EmailLimiter(Windows);

impl EmailLimiter {
    /// Whether a mail to `email` requested from `ip` may go out; records it when allowed.
    pub fn check(&self, email: &str, ip: Option<IpAddr>) -> bool {
        self.check_at(email, ip, Instant::now())
    }

    fn check_at(&self, email: &str, ip: Option<IpAddr>, now: Instant) -> bool {
        self.0.allow(
            &[
                (sender_key(email, ip), &PER_SENDER),
                (email.to_owned(), &PER_ADDRESS),
            ],
            now,
        )
    }
}

/// Budget of the password-reset mails, apart from [`EmailLimiter`] so asking for resets cannot
/// use up an inbox's magic-link mails (and the other way round).
#[derive(Clone, Default)]
pub struct ResetLimiter(Windows);

impl ResetLimiter {
    /// A request for `login` (username or address as typed) from `ip`; checked before the
    /// account is looked up.
    pub fn check_request(&self, login: &str, ip: Option<IpAddr>) -> bool {
        self.check_request_at(login, ip, Instant::now())
    }

    fn check_request_at(&self, login: &str, ip: Option<IpAddr>, now: Instant) -> bool {
        self.0
            .allow(&[(sender_key(login, ip), &RESET_PER_SENDER)], now)
    }

    /// One reset mail to `account`.
    pub fn check_account(&self, account: Uuid) -> bool {
        self.check_account_at(account, Instant::now())
    }

    fn check_account_at(&self, account: Uuid, now: Instant) -> bool {
        self.0
            .allow(&[(format!("account:{account}"), &RESET_PER_ACCOUNT)], now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(last: u8) -> Option<IpAddr> {
        Some(IpAddr::from([10, 0, 0, last]))
    }

    #[test]
    fn three_per_sender_window_then_blocked_until_the_oldest_ages_out() {
        let l = EmailLimiter::default();
        let t = Instant::now();
        let window = PER_SENDER.period;
        assert!(l.check_at("a@b.cz", ip(1), t));
        assert!(l.check_at("a@b.cz", ip(1), t + Duration::from_secs(60)));
        assert!(l.check_at("a@b.cz", ip(1), t + Duration::from_secs(120)));
        assert!(!l.check_at("a@b.cz", ip(1), t + Duration::from_secs(130)));
        assert!(!l.check_at("a@b.cz", ip(1), t + window - Duration::from_secs(1)));
        assert!(l.check_at("a@b.cz", ip(1), t + window));
        assert!(!l.check_at("a@b.cz", ip(1), t + window + Duration::from_secs(1)));
    }

    #[test]
    fn another_ip_is_not_locked_out_by_a_flood() {
        let l = EmailLimiter::default();
        let t = Instant::now();
        for _ in 0..5 {
            l.check_at("a@b.cz", ip(66), t);
        }
        assert!(l.check_at("a@b.cz", ip(1), t));
        assert!(
            l.check_at("c@b.cz", ip(66), t),
            "other addresses are separate"
        );
    }

    #[test]
    fn ten_per_address_per_hour_across_all_ips() {
        let l = EmailLimiter::default();
        let t = Instant::now();
        let sent = (1..=20).filter(|i| l.check_at("a@b.cz", ip(*i), t)).count();
        assert_eq!(sent, 10);
        assert!(!l.check_at("a@b.cz", ip(99), t + Duration::from_secs(59 * 60)));
        assert!(l.check_at("a@b.cz", ip(99), t + PER_ADDRESS.period));
    }

    #[test]
    fn a_refused_request_does_not_use_up_the_budget() {
        let l = EmailLimiter::default();
        let t = Instant::now();
        for i in 1..=10 {
            assert!(l.check_at("a@b.cz", ip(i), t));
        }
        // Refused by the address cap: must not count against ip(1)'s own window later.
        assert!(!l.check_at("a@b.cz", ip(1), t));
        let later = t + PER_ADDRESS.period;
        assert!(l.check_at("a@b.cz", ip(1), later));
        assert!(l.check_at("a@b.cz", ip(1), later));
    }

    #[test]
    fn reset_requests_three_per_sender_and_five_mails_per_account() {
        let l = ResetLimiter::default();
        let t = Instant::now();
        let asked = (0..4)
            .filter(|_| l.check_request_at("user:eva", ip(1), t))
            .count();
        assert_eq!(asked, 3);
        assert!(l.check_request_at("eva@b.cz", ip(1), t), "another spelling");
        assert!(l.check_request_at("user:eva", ip(2), t), "another client");

        let eva = Uuid::new_v4();
        let mailed = (0..7).filter(|_| l.check_account_at(eva, t)).count();
        assert_eq!(mailed, 5);
        assert!(l.check_account_at(Uuid::new_v4(), t), "other accounts");
        assert!(l.check_account_at(eva, t + RESET_PER_ACCOUNT.period));
    }

    #[test]
    fn reset_and_magic_link_budgets_are_separate() {
        let (email, reset) = (EmailLimiter::default(), ResetLimiter::default());
        let t = Instant::now();
        for _ in 0..3 {
            assert!(reset.check_request_at("a@b.cz", ip(1), t));
        }
        assert!(email.check_at("a@b.cz", ip(1), t));
    }
}
