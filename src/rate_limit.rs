use std::future::Future;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

pub struct RateLimiter<T: Clone> {
    time_per: Duration,
    resource: Mutex<(Instant, T)>,
}

impl<T: Clone> RateLimiter<T> {
    #[allow(dead_code)]
    pub fn new(time_per: Duration, resource: T) -> Self {
        Self {
            time_per: time_per,
            resource: Mutex::new((Instant::now(), resource)),
        }
    }

    pub fn new_fast(time_per: Duration, resource: T) -> Self {
        let now = Instant::now();
        Self {
            time_per: time_per,
            resource: Mutex::new((now.checked_sub(time_per).unwrap_or(now), resource)),
        }
    }

    pub async fn use_with<Fut: Future>(&self, f: impl FnOnce(T) -> Fut) -> <Fut as Future>::Output {
        let mut lock = self.resource.lock().await;
        let elapsed = Instant::now().duration_since(lock.0);
        if let Some(sleep_duration) = self.time_per.checked_sub(elapsed) {
            sleep(sleep_duration).await;
        }
        // I tried very hard to get away without this clone, but I couldn't figure it out
        let result = f(lock.1.clone()).await;
        lock.0 = Instant::now();
        result
    }
}
