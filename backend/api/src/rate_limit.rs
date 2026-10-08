use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Extension, FromRequestParts, Request};
use axum::http::HeaderMap;
use axum::http::request::Parts;
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

/// In-memory token bucket per client IP: burst 5, refill 10 per minute.
/// The client IP is the peer address, or the proxy-reported one when `trust_proxy_headers` is set
/// (see [`client_ip`]).
#[derive(Clone)]
pub struct RateLimiter {
    enabled: bool,
    trust_proxy_headers: bool,
    buckets: Arc<Mutex<HashMap<IpAddr, Bucket>>>,
}

impl RateLimiter {
    pub fn new(enabled: bool, trust_proxy_headers: bool) -> Self {
        Self {
            enabled,
            trust_proxy_headers,
            buckets: Arc::default(),
        }
    }

    /// `ip`'s bucket: IPv4 as is (IPv4-mapped IPv6 included), IPv6 by /64 prefix — one end site
    /// usually gets a whole /64, so per-address buckets would let a single client rotate freely.
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
        let bucket = map.entry(bucket_key(ip)).or_insert(Bucket {
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

pub(crate) fn bucket_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(
            u128::from(v6) & 0xffff_ffff_ffff_ffff_0000_0000_0000_0000,
        )),
        v4 => v4,
    }
}

/// The IP to rate-limit on. With `trust_proxy_headers` it is the first `X-Forwarded-For` entry:
/// ingress-nginx with its defaults (`use-forwarded-headers` and `compute-full-forwarded-for` off)
/// replaces any client-sent header with the address it saw, so the first entry is the real client.
/// A proxy that *appends* instead would make the first entry client-controlled. Without the flag
/// the header is ignored, since any client could set it to dodge the limit.
fn client_ip(
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    trust_proxy_headers: bool,
) -> Option<IpAddr> {
    let forwarded = trust_proxy_headers
        .then(|| headers.get("x-forwarded-for")?.to_str().ok())
        .flatten()
        .and_then(|v| v.split(',').next()?.trim().parse().ok());
    forwarded.or(peer)
}

/// The caller's rate-limit address ([`client_ip`], then [`bucket_key`]); `None` without a socket
/// (only in `oneshot` tests).
pub struct ClientIp(pub Option<IpAddr>);

impl FromRequestParts<crate::state::AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::state::AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip());
        let ip = client_ip(&parts.headers, peer, state.limiter.trust_proxy_headers);
        Ok(Self(ip.map(bucket_key)))
    }
}

/// Route middleware; the limiter arrives as an `Extension` layered in `app()`.
pub async fn limit_by_ip(
    Extension(limiter): Extension<RateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());
    if let Some(ip) = client_ip(req.headers(), peer, limiter.trust_proxy_headers)
        && !limiter.check(ip)
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
        let l = RateLimiter::new(true, false);
        let t = Instant::now();
        for _ in 0..5 {
            assert!(l.check_at(ip(1), t));
        }
        assert!(!l.check_at(ip(1), t));
    }

    #[test]
    fn refills_over_time() {
        let l = RateLimiter::new(true, false);
        let t = Instant::now();
        for _ in 0..5 {
            l.check_at(ip(1), t);
        }
        assert!(!l.check_at(ip(1), t + Duration::from_secs(3)));
        assert!(l.check_at(ip(1), t + Duration::from_secs(7)));
    }

    #[test]
    fn ips_are_independent() {
        let l = RateLimiter::new(true, false);
        let t = Instant::now();
        for _ in 0..5 {
            l.check_at(ip(1), t);
        }
        assert!(l.check_at(ip(2), t));
    }

    #[test]
    fn disabled_never_blocks() {
        let l = RateLimiter::new(false, false);
        let t = Instant::now();
        assert!((0..100).all(|_| l.check_at(ip(1), t)));
    }

    fn v6(s: &str) -> IpAddr {
        s.parse().expect("ipv6")
    }

    #[test]
    fn ipv6_shares_a_bucket_per_64() {
        let l = RateLimiter::new(true, false);
        let t = Instant::now();
        for i in 1..=5 {
            assert!(l.check_at(v6(&format!("2001:db8:1:2::{i}")), t));
        }
        assert!(!l.check_at(v6("2001:db8:1:2:ffff:ffff:ffff:ffff"), t));
        assert!(l.check_at(v6("2001:db8:1:3::1"), t));
    }

    #[test]
    fn bucket_key_masks_v6_and_unmaps_v4() {
        assert_eq!(
            bucket_key(v6("2001:db8:1:2:aaaa:bbbb:cccc:dddd")),
            v6("2001:db8:1:2::")
        );
        assert_eq!(
            bucket_key(v6("::ffff:203.0.113.7")),
            IpAddr::from([203, 0, 113, 7])
        );
        assert_eq!(bucket_key(ip(1)), ip(1));
    }

    fn xff(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", value.parse().expect("header value"));
        h
    }

    #[test]
    fn client_ip_uses_first_forwarded_entry_only_when_trusted() {
        let peer = Some(ip(9));
        let h = xff(" 203.0.113.7 , 10.0.0.2");
        assert_eq!(
            client_ip(&h, peer, true),
            Some(IpAddr::from([203, 0, 113, 7]))
        );
        assert_eq!(client_ip(&h, peer, false), peer);
        assert_eq!(
            client_ip(&xff("2001:db8::1"), peer, true),
            "2001:db8::1".parse().ok()
        );
    }

    #[test]
    fn client_ip_falls_back_to_peer() {
        let peer = Some(ip(9));
        assert_eq!(client_ip(&HeaderMap::new(), peer, true), peer);
        for bad in ["", "unknown", "203.0.113.7:4000", ", 203.0.113.7"] {
            assert_eq!(client_ip(&xff(bad), peer, true), peer, "{bad:?}");
        }
        assert_eq!(client_ip(&HeaderMap::new(), None, true), None);
    }
}
