use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

pub struct RateLimiter<T> {
    time_per: Duration,
    resource: Mutex<(Instant, T)>,
}

impl<T> RateLimiter<T> {
    pub fn new(time_per: Duration, resource: T) -> Self {
        Self {
            time_per: time_per,
            resource: Mutex::new((Instant::now(), resource)),
        }
    }

    #[allow(dead_code)]
    pub fn new_fast(time_per: Duration, resource: T) -> Self {
        let now = Instant::now();
        Self {
            time_per: time_per,
            resource: Mutex::new((now.checked_sub(time_per).unwrap_or(now), resource)),
        }
    }

    pub async fn use_with<O>(&self, f: impl AsyncFnOnce(&T) -> O) -> O {
        let mut lock = self.resource.lock().await;
        let elapsed = Instant::now().duration_since(lock.0);
        if let Some(sleep_duration) = self.time_per.checked_sub(elapsed) {
            sleep(sleep_duration).await;
        }
        let result = f(&lock.1).await;
        lock.0 = Instant::now();
        result
    }
}
