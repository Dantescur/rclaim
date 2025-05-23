/*
  ws/client.rs
*/

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;

pub struct Client {
    pub request_count: usize,
    pub window_start: Option<DateTime<Utc>>,
}

pub type ClientMap = Arc<DashMap<String, Client>>;

pub fn is_rate_limited(client: &mut Client) -> bool {
    let now = Utc::now();
    let window_ms = 15 * 60 * 1000;
    let max_request = 100;

    if let Some(start) = client.window_start {
        if now.signed_duration_since(start).num_milliseconds() >= window_ms {
            client.window_start = Some(now);
            client.request_count = 0;
            return false;
        }
        if client.request_count >= max_request {
            return true;
        }
    } else {
        client.window_start = Some(now);
        client.request_count = 0;
        return false;
    }
    client.request_count += 1;
    false
}

#[cfg(test)]
mod test {
    use chrono::Duration;

    use super::*;

    #[test]
    fn test_rate_limit() {
        let mut client = Client {
            request_count: 0,
            window_start: Some(Utc::now()),
        };

        for _ in 0..99 {
            assert!(!is_rate_limited(&mut client))
        }

        assert!(!is_rate_limited(&mut client));

        assert!(is_rate_limited(&mut client));

        client.window_start = Some(Utc::now() - Duration::minutes(16));
        assert!(!is_rate_limited(&mut client));
    }
}
