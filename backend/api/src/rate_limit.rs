use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Extension, Request};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::AppError;

const BURST: f64 = 5.0;
const REFILL_PER_SEC: f64 = 10.0 / 60.0;
/// Entries idle this long are full buckets anyway; dropping them bounds memory.
const PRUNE_ABOVE: usize = 10_000;
const FULL_AFTER: Duration = Duration::from_secs(60);

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// In-memory token bucket per IP: burst 5, refill 10 per minute.
/// Keyed on the peer address; behind a reverse proxy this is the proxy's address.
#[derive(Clone)]
pub struct RateLimiter {
    enabled: bool,
    buckets: Arc<Mutex<HashMap<IpAddr, Bucket>>>,
}

impl RateLimiter {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            buckets: Arc::default(),
        }
    }

    pub fn check(&self, ip: IpAddr) -> bool {
        self.check_at(ip, Instant::now())
    }

    fn check_at(&self, ip: IpAddr, now: Instant) -> bool {
        if !self.enabled {
            return true;
        }
        // A poisoned lock only means another request panicked mid-update; the map is still usable.
        let mut map = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > PRUNE_ABOVE {
            map.retain(|_, b| now.saturating_duration_since(b.last) < FULL_AFTER);
        }
        let bucket = map.entry(ip).or_insert(Bucket {
            tokens: BURST,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * REFILL_PER_SEC).min(BURST);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Route middleware; the limiter arrives as an `Extension` layered in `app()`.
pub async fn limit_by_ip(
    Extension(limiter): Extension<RateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let peer = req.extensions().get::<ConnectInfo<SocketAddr>>();
    if let Some(ConnectInfo(addr)) = peer
        && !limiter.check(addr.ip())
    {
        return AppError::RateLimited.into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(last: u8) -> IpAddr {
        IpAddr::from([10, 0, 0, last])
    }

    #[test]
    fn allows_burst_then_blocks() {
        let l = RateLimiter::new(true);
        let t = Instant::now();
        for _ in 0..5 {
            assert!(l.check_at(ip(1), t));
        }
        assert!(!l.check_at(ip(1), t));
    }

    #[test]
    fn refills_over_time() {
        let l = RateLimiter::new(true);
        let t = Instant::now();
        for _ in 0..5 {
            l.check_at(ip(1), t);
        }
        assert!(!l.check_at(ip(1), t + Duration::from_secs(3)));
        assert!(l.check_at(ip(1), t + Duration::from_secs(7)));
    }

    #[test]
    fn ips_are_independent() {
        let l = RateLimiter::new(true);
        let t = Instant::now();
        for _ in 0..5 {
            l.check_at(ip(1), t);
        }
        assert!(l.check_at(ip(2), t));
    }

    #[test]
    fn disabled_never_blocks() {
        let l = RateLimiter::new(false);
        let t = Instant::now();
        assert!((0..100).all(|_| l.check_at(ip(1), t)));
    }
}
