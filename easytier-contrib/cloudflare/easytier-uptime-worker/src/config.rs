use worker::Env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub ring_granularity_seconds: u32,
    pub ring_window_hours: i64,
}

impl AppConfig {
    pub fn from_env(_env: &Env) -> Self {
        Self {
            ring_granularity_seconds: 60 * 15,
            ring_window_hours: 24,
        }
    }
}
