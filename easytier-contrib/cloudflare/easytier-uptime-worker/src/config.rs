use worker::Env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub admin_password: String,
    pub token_ttl_seconds: u64,
    #[allow(dead_code)]
    pub health_retention_days: i64,
    pub ring_granularity_seconds: u32,
    pub ring_window_hours: i64,
}

impl AppConfig {
    pub fn from_env(env: &Env) -> Self {
        Self {
            admin_password: read_var(env, "ADMIN_PASSWORD", "change-me"),
            token_ttl_seconds: read_var(env, "TOKEN_TTL_SECONDS", "86400")
                .parse()
                .unwrap_or(86_400),
            health_retention_days: read_var(env, "HEALTH_RETENTION_DAYS", "30")
                .parse()
                .unwrap_or(30),
            ring_granularity_seconds: 60 * 15,
            ring_window_hours: 24,
        }
    }
}

fn read_var(env: &Env, name: &str, default: &str) -> String {
    env.var(name)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| default.to_string())
}
