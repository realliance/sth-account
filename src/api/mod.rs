pub mod auth;
pub mod bots;
pub mod lobbies;
pub mod matches;
pub mod matchmaking;
pub mod rooms;
pub mod users;

#[cfg(test)]
mod login_tests;

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use std::str::FromStr;

pub fn add_rate_limit_headers(headers: &mut HeaderMap) {
    headers.insert(
        HeaderName::from_str("X-Rate-Limit-Limit").unwrap(),
        HeaderValue::from_static("100"),
    );
    headers.insert(
        HeaderName::from_str("X-Rate-Limit-Remaining").unwrap(),
        HeaderValue::from_static("99"),
    );
    headers.insert(
        HeaderName::from_str("X-Rate-Limit-Reset").unwrap(),
        HeaderValue::from_static("3600"),
    );
}
