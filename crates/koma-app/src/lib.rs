pub mod config;
pub mod domain;
pub mod ghostty_config;
pub mod persistence;

#[path = "runtime/profiles.rs"]
pub mod profiles;
#[path = "runtime/web_notifications.rs"]
pub mod web_notifications;

#[cfg(test)]
#[allow(dead_code)]
#[path = "runtime/web_views/js_bridge.rs"]
mod js_bridge;
