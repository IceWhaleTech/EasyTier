use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            message: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            message: None,
        }
    }

    pub fn message(message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: None,
            error: None,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PaginationParams {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

impl PaginationParams {
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn per_page(&self, default: u32, max: u32) -> u32 {
        self.per_page.unwrap_or(default).clamp(1, max)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
    pub host: String,
    pub port: i32,
    pub protocol: String,
    pub description: Option<String>,
    pub max_connections: i32,
    pub allow_relay: bool,
    pub network_name: String,
    pub network_secret: Option<String>,
    pub mail: Option<String>,
}

impl CreateNodeRequest {
    pub fn validate(&self) -> AppResult<()> {
        validate_node_payload(
            &self.name,
            &self.host,
            self.port,
            &self.protocol,
            self.description.as_deref(),
            self.max_connections,
            &self.network_name,
            self.network_secret.as_deref(),
            self.mail.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub protocol: Option<String>,
    pub description: Option<String>,
    pub max_connections: Option<i32>,
    pub is_active: Option<bool>,
    pub allow_relay: Option<bool>,
    pub network_name: Option<String>,
    pub network_secret: Option<String>,
    pub mail: Option<String>,
    pub tags: Option<Vec<String>>,
}

impl UpdateNodeRequest {
    pub fn validate(&self) -> AppResult<()> {
        if let Some(name) = self.name.as_deref() {
            validate_non_empty("name", name, 100)?;
        }
        if let Some(host) = self.host.as_deref() {
            validate_non_empty("host", host, 255)?;
        }
        if let Some(protocol) = self.protocol.as_deref() {
            validate_non_empty("protocol", protocol, 20)?;
        }
        if let Some(description) = self.description.as_deref() {
            validate_optional("description", Some(description), 500)?;
        }
        if let Some(max_connections) = self.max_connections
            && !(1..=10_000).contains(&max_connections)
        {
            return Err(AppError::BadRequest(
                "max_connections must be between 1 and 10000".to_string(),
            ));
        }
        if let Some(port) = self.port
            && !(1..=65_535).contains(&port)
        {
            return Err(AppError::BadRequest(
                "port must be between 1 and 65535".to_string(),
            ));
        }
        validate_optional("network_name", self.network_name.as_deref(), 100)?;
        validate_optional("network_secret", self.network_secret.as_deref(), 100)?;
        validate_optional("mail", self.mail.as_deref(), 255)?;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NodeFilterParams {
    pub is_active: Option<bool>,
    pub protocol: Option<String>,
    pub search: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HealthFilterParams {
    pub status: Option<String>,
    pub since: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HealthStatsParams {
    pub hours: Option<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AdminNodeFilterParams {
    pub is_active: Option<bool>,
    pub is_approved: Option<bool>,
    pub protocol: Option<String>,
    pub search: Option<String>,
    pub tag: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminLoginRequest {
    pub password: String,
}

impl AdminLoginRequest {
    pub fn validate(&self) -> AppResult<()> {
        validate_non_empty("password", &self.password, 255)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminLoginResponse {
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResponse {
    pub id: i32,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub protocol: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub max_connections: i32,
    pub current_connections: i32,
    pub is_active: bool,
    pub is_approved: bool,
    pub allow_relay: bool,
    pub network_name: Option<String>,
    pub network_secret: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub address: String,
    pub usage_percentage: f64,
    pub current_health_status: Option<String>,
    pub last_check_time: Option<String>,
    pub last_response_time: Option<i32>,
    pub health_percentage_24h: Option<f64>,
    pub health_record_total_counter_ring: Vec<u64>,
    pub health_record_healthy_counter_ring: Vec<u64>,
    pub ring_granularity: u32,
    pub qq_number: Option<String>,
    pub wechat: Option<String>,
    pub mail: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthRecordResponse {
    pub id: i32,
    pub node_id: i32,
    pub status: String,
    pub response_time: Option<i32>,
    pub error_message: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatsResponse {
    pub total_checks: u64,
    pub healthy_count: u64,
    pub unhealthy_count: u64,
    pub health_percentage: f64,
    pub average_response_time: Option<f64>,
    pub uptime_percentage: f64,
    pub last_check_time: Option<String>,
    pub last_status: Option<String>,
}

impl Default for HealthStatsResponse {
    fn default() -> Self {
        Self {
            total_checks: 0,
            healthy_count: 0,
            unhealthy_count: 0,
            health_percentage: 0.0,
            average_response_time: None,
            uptime_percentage: 0.0,
            last_check_time: None,
            last_status: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub status: String,
    pub is_active: bool,
    pub response_time: Option<i32>,
    pub error_message: Option<String>,
}

impl NodeResponse {
    pub fn from_probe(request: &CreateNodeRequest, probe: &ProbeOutcome) -> Self {
        let timestamp = now_iso();
        Self {
            id: 0,
            name: request.name.clone(),
            host: request.host.clone(),
            port: request.port,
            protocol: request.protocol.clone(),
            version: None,
            description: request.description.clone(),
            max_connections: request.max_connections,
            current_connections: 0,
            is_active: probe.is_active,
            is_approved: false,
            allow_relay: request.allow_relay,
            network_name: Some(request.network_name.clone()),
            network_secret: request.network_secret.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            address: format!("{}://{}:{}", request.protocol, request.host, request.port),
            usage_percentage: 0.0,
            current_health_status: Some(probe.status.clone()),
            last_check_time: Some(now_iso()),
            last_response_time: probe.response_time,
            health_percentage_24h: None,
            health_record_total_counter_ring: Vec::new(),
            health_record_healthy_counter_ring: Vec::new(),
            ring_granularity: 0,
            qq_number: None,
            wechat: None,
            mail: request.mail.clone(),
            tags: Vec::new(),
        }
    }
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn validate_node_payload(
    name: &str,
    host: &str,
    port: i32,
    protocol: &str,
    description: Option<&str>,
    max_connections: i32,
    network_name: &str,
    network_secret: Option<&str>,
    mail: Option<&str>,
) -> AppResult<()> {
    validate_non_empty("name", name, 100)?;
    validate_non_empty("host", host, 255)?;
    validate_non_empty("protocol", protocol, 20)?;
    validate_non_empty("network_name", network_name, 100)?;
    validate_optional("description", description, 500)?;
    validate_optional("network_secret", network_secret, 100)?;
    validate_optional("mail", mail, 255)?;

    if !(1..=65_535).contains(&port) {
        return Err(AppError::BadRequest(
            "port must be between 1 and 65535".to_string(),
        ));
    }

    if !(1..=10_000).contains(&max_connections) {
        return Err(AppError::BadRequest(
            "max_connections must be between 1 and 10000".to_string(),
        ));
    }

    Ok(())
}

fn validate_non_empty(field: &str, value: &str, max_len: usize) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!("{field} is required")));
    }
    if trimmed.len() > max_len {
        return Err(AppError::BadRequest(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(())
}

fn validate_optional(field: &str, value: Option<&str>, max_len: usize) -> AppResult<()> {
    if let Some(value) = value
        && value.trim().len() > max_len
    {
        return Err(AppError::BadRequest(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(())
}
