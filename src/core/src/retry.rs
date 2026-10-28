use std::future::Future;
use std::time::Duration;

/// Whether a failed operation should be retried with backoff.
pub trait Classify {
    fn retry(&self) -> bool;
}

/// Retry `op` while [`Classify::retry`] is true, sleeping `delays_ms[i]` after
/// the i-th retryable failure. `on_retry` runs immediately before each sleep.
pub async fn retry_classified<T, E, F, Fut, R>(
    delays_ms: &[u64],
    mut op: F,
    mut on_retry: R,
) -> Result<T, E>
where
    E: Classify,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    R: FnMut(usize, &E),
{
    let mut last = op().await;
    for (i, delay) in delays_ms.iter().enumerate() {
        match &last {
            Ok(_) => return last,
            Err(e) if e.retry() => {
                on_retry(i + 1, e);
                if *delay > 0 {
                    tokio::time::sleep(Duration::from_millis(*delay)).await;
                }
                last = op().await;
            }
            Err(_) => return last,
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, PartialEq, Eq)]
    enum Sample {
        Transient,
        Fatal,
    }

    impl Classify for Sample {
        fn retry(&self) -> bool {
            matches!(self, Self::Transient)
        }
    }

    #[tokio::test]
    async fn retries_transient_then_succeeds() {
        let attempts = AtomicUsize::new(0);
        let result = retry_classified(
            &[1, 1],
            || async {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(Sample::Transient)
                } else {
                    Ok("ok")
                }
            },
            |_, _| {},
        )
        .await;
        assert_eq!(result, Ok("ok"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn does_not_retry_fatal() {
        let attempts = AtomicUsize::new(0);
        let result: Result<(), Sample> = retry_classified(
            &[1, 1],
            || async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(Sample::Fatal)
            },
            |_, _| {},
        )
        .await;
        assert_eq!(result, Err(Sample::Fatal));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
