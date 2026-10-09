use std::fmt::Display;
use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;

/// Delay before retry `attempt` (0-based): 1, 2, 4, 8 s, then 10 s.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.min(4)).min(Duration::from_secs(10))
}

/// Runs `op` until it succeeds, retrying with [`backoff`] while less than `patience` has passed
/// since the first attempt and `transient` holds for the error; otherwise the error is returned.
/// On k8s the API starts together with what it depends on (Postgres, the S3 bucket): waiting beats
/// crash-looping until they are up, but a refusal (bad credentials) will not go away by waiting.
pub async fn patiently<T, E, F, Fut>(
    what: &str,
    patience: Duration,
    transient: impl Fn(&E) -> bool,
    mut op: F,
) -> Result<T, E>
where
    E: Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let started = Instant::now();
    let mut attempt = 0;
    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(e) if started.elapsed() < patience && transient(&e) => {
                let delay = backoff(attempt);
                tracing::warn!(error = %e, retry_in = ?delay, "{what} not reachable yet");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_caps() {
        let secs: Vec<u64> = (0..7).map(|a| backoff(a).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 10, 10, 10]);
    }

    #[tokio::test(start_paused = true)]
    async fn patiently_retries_until_success_or_patience_runs_out() {
        let mut calls = 0;
        let got: Result<u32, String> = patiently(
            "thing",
            Duration::from_secs(60),
            |_| true,
            || {
                calls += 1;
                let n = calls;
                async move {
                    if n < 3 {
                        Err(format!("down {n}"))
                    } else {
                        Ok(n)
                    }
                }
            },
        )
        .await;
        assert_eq!(got, Ok(3));

        let started = tokio::time::Instant::now();
        let got: Result<(), &str> = patiently(
            "thing",
            Duration::from_secs(5),
            |_| true,
            || async { Err("down") },
        )
        .await;
        assert_eq!(got, Err("down"));
        // 1 + 2 + 4 s of backoff: the attempt after the 5 s mark is the last one.
        assert_eq!(started.elapsed(), Duration::from_secs(7));

        let mut calls = 0;
        let got: Result<(), &str> = patiently(
            "thing",
            Duration::from_secs(60),
            |e| *e != "refused",
            || {
                calls += 1;
                async { Err("refused") }
            },
        )
        .await;
        assert_eq!(
            (got, calls),
            (Err("refused"), 1),
            "a permanent error is not retried"
        );
    }
}
