use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

pub struct RateLimiter {
    timestamps: Mutex<VecDeque<Instant>>,
    max_per_minute: usize,
}

impl RateLimiter {
    pub fn new(max_per_minute: usize) -> Self {
        Self {
            timestamps: Mutex::new(VecDeque::new()),
            max_per_minute: max_per_minute.max(1),
        }
    }

    pub async fn acquire_slot(&self) {
        loop {
            let wait_secs = {
                let mut ts = self.timestamps.lock().await;
                let now = Instant::now();
                while ts
                    .front()
                    .map_or(false, |t| now.duration_since(*t) > Duration::from_secs(60))
                {
                    ts.pop_front();
                }
                if ts.len() < self.max_per_minute {
                    ts.push_back(now);
                    0.0_f64
                } else {
                    let oldest = *ts.front().unwrap();
                    (60.0_f64 - now.duration_since(oldest).as_secs_f64() + 0.05).max(0.05)
                }
            };
            if wait_secs <= 0.0 {
                return;
            }
            sleep(Duration::from_secs_f64(wait_secs)).await;
        }
    }
}
