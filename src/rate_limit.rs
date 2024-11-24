use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct RateLimiter<T> {
    last_updated: Instant,
    time_per: Duration,
    resource: T,
}

impl<T> RateLimiter<T> {
    pub fn new(time_per: Duration, resource: T) -> Self {
        Self {
            last_updated: Instant::now(),
            time_per: time_per,
            resource: resource,
        }
    }

    pub async fn wait(&mut self) -> &T {
        let start_time = Instant::now();
        let elapsed = start_time.duration_since(self.last_updated);
        match self.time_per.checked_sub(elapsed) {
            None => {
                self.last_updated = start_time;
            }
            Some(sleep_duration) => {
                sleep(sleep_duration).await;
                self.last_updated = Instant::now();
            }
        }
        &self.resource
    }
}
