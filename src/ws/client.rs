/*
  ws/client.rs
*/

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;

pub struct Client {
    pub rate_limit_timestamp: Vec<DateTime<Utc>>,
}

pub type ClientMap = Arc<DashMap<String, Client>>;

pub fn is_rate_limited(client: &mut Client) -> bool {
    let now = Utc::now();
    let window_ms = 15 * 60 * 1000;
    let max_request = 100;

    client.rate_limit_timestamp = client
        .rate_limit_timestamp
        .drain(..)
        .filter(|ts| now.signed_duration_since(*ts).num_milliseconds() < window_ms)
        .collect();

    if client.rate_limit_timestamp.len() >= max_request {
        return true;
    }

    client.rate_limit_timestamp.push(now);
    false
}

#[cfg(test)]
mod test {
    use chrono::Duration;

    use super::*;

    #[test]
    fn test_rate_limit() {
        let mut client = Client {
            rate_limit_timestamp: Vec::new(),
        };

        for _ in 0..99 {
            assert!(!is_rate_limited(&mut client))
        }

        assert!(!is_rate_limited(&mut client));

        assert!(is_rate_limited(&mut client));

        client.rate_limit_timestamp = vec![Utc::now() - Duration::minutes(16)];
        assert!(!is_rate_limited(&mut client));
    }
}
