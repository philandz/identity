use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct FixedWindowLimiter {
    max_requests: u32,
    window: Duration,
    buckets: HashMap<String, (u32, Instant)>,
}

impl FixedWindowLimiter {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            buckets: HashMap::new(),
        }
    }

    pub fn allow(&mut self, key: &str) -> bool {
        let now = Instant::now();
        let entry = self.buckets.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) > self.window {
            *entry = (0, now);
        }

        if entry.0 >= self.max_requests {
            return false;
        }
        entry.0 += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_window_limits_requests() {
        let mut limiter = FixedWindowLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.allow("u1"));
        assert!(limiter.allow("u1"));
        assert!(!limiter.allow("u1"));
    }
}
