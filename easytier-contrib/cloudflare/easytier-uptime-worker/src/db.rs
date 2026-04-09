use std::{cmp::Ordering, collections::BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use worker::{D1Database, query};

use crate::{
    config::AppConfig,
    error::{AppError, AppResult},
    models::{
        AdminNodeFilterParams, CreateNodeRequest, HealthFilterParams, HealthRecordResponse,
        HealthStatsResponse, NodeFilterParams, NodeResponse, PaginatedResponse, PaginationParams,
        UpdateNodeRequest, now_iso,
    },
};

#[derive(Debug, Clone, Deserialize)]
struct SharedNodeRow {
    id: i32,
    name: String,
    host: String,
    port: i32,
    protocol: String,
    version: Option<String>,
    allow_relay: i32,
    network_name: Option<String>,
    network_secret: Option<String>,
    description: Option<String>,
    max_connections: i32,
    current_connections: i32,
    is_active: i32,
    is_approved: i32,
    qq_number: Option<String>,
    wechat: Option<String>,
    mail: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct HealthRecordRow {
    id: i32,
    node_id: i32,
    status: String,
    response_time: Option<i32>,
    error_message: Option<String>,
    checked_at: String,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    total: i64,
}

#[derive(Debug, Deserialize)]
struct TagRow {
    tag: String,
}

const DB_BINDING: &str = "UPTIME_DB";

pub async fn create_node(
    env: &worker::Env,
    request: &CreateNodeRequest,
) -> AppResult<NodeResponse> {
    if node_exists(env, &request.host, request.port, &request.protocol).await? {
        return Err(AppError::Conflict(
            "a node with the same host, port and protocol already exists".to_string(),
        ));
    }

    let db = env.d1(DB_BINDING)?;
    let timestamp = now_iso();
    let description = request.description.clone().unwrap_or_default();
    let network_secret = request.network_secret.clone().unwrap_or_default();
    let qq_number = request.qq_number.clone().unwrap_or_default();
    let wechat = request.wechat.clone().unwrap_or_default();
    let mail = request.mail.clone().unwrap_or_default();

    let statement = query!(
        &db,
        "INSERT INTO shared_nodes (
            name,
            host,
            port,
            protocol,
            version,
            allow_relay,
            network_name,
            network_secret,
            description,
            max_connections,
            current_connections,
            is_active,
            is_approved,
            qq_number,
            wechat,
            mail,
            created_at,
            updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &request.name,
        &request.host,
        &request.port,
        &request.protocol,
        &Value::Null,
        &request.allow_relay,
        &request.network_name,
        &network_secret,
        &description,
        &request.max_connections,
        &0,
        &false,
        &false,
        &qq_number,
        &wechat,
        &mail,
        &timestamp,
        &timestamp
    )?;

    let result = statement.run().await?;
    let row_id = result
        .meta()?
        .and_then(|meta| meta.last_row_id)
        .ok_or_else(|| AppError::Internal("failed to get inserted node id".to_string()))?;

    get_node(env, row_id as i32, true).await
}

pub async fn get_node(
    env: &worker::Env,
    node_id: i32,
    public_only: bool,
) -> AppResult<NodeResponse> {
    let row = fetch_node_row(env, node_id, public_only).await?;
    build_node_response(env, &AppConfig::from_env(env), row, public_only).await
}

pub async fn list_nodes(
    env: &worker::Env,
    config: &AppConfig,
    pagination: &PaginationParams,
    filters: &NodeFilterParams,
    public_only: bool,
) -> AppResult<PaginatedResponse<NodeResponse>> {
    let page = pagination.page();
    let per_page = pagination.per_page(20, 200);
    let offset = (page - 1) * per_page;

    let (where_sql, params) = build_node_filters(filters, public_only);
    let count_sql = format!("SELECT COUNT(*) AS total FROM shared_nodes n{where_sql}");
    let total = fetch_count(env, &count_sql, &params).await? as u64;

    let mut list_params = params.clone();
    list_params.push(json!(per_page));
    list_params.push(json!(offset));

    let list_sql = format!(
        "SELECT
            n.id,
            n.name,
            n.host,
            n.port,
            n.protocol,
            n.version,
            n.allow_relay,
            n.network_name,
            n.network_secret,
            n.description,
            n.max_connections,
            n.current_connections,
            n.is_active,
            n.is_approved,
            n.qq_number,
            n.wechat,
            n.mail,
            n.created_at,
            n.updated_at
        FROM shared_nodes n
        {where_sql}
        ORDER BY n.id ASC
        LIMIT ? OFFSET ?"
    );

    let rows: Vec<SharedNodeRow> = query_all(env, &list_sql, &list_params).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(build_node_response(env, config, row, public_only).await?);
    }

    let total_pages = if total == 0 {
        0
    } else {
        ((total + u64::from(per_page) - 1) / u64::from(per_page)) as u32
    };

    Ok(PaginatedResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
    })
}

pub async fn list_admin_nodes(
    env: &worker::Env,
    config: &AppConfig,
    pagination: &PaginationParams,
    filters: &AdminNodeFilterParams,
) -> AppResult<PaginatedResponse<NodeResponse>> {
    let page = pagination.page();
    let per_page = pagination.per_page(200, 500);
    let offset = (page - 1) * per_page;

    let (where_sql, params) = build_admin_node_filters(filters);
    let count_sql = format!("SELECT COUNT(*) AS total FROM shared_nodes n{where_sql}");
    let total = fetch_count(env, &count_sql, &params).await? as u64;

    let mut list_params = params.clone();
    list_params.push(json!(per_page));
    list_params.push(json!(offset));

    let list_sql = format!(
        "SELECT
            n.id,
            n.name,
            n.host,
            n.port,
            n.protocol,
            n.version,
            n.allow_relay,
            n.network_name,
            n.network_secret,
            n.description,
            n.max_connections,
            n.current_connections,
            n.is_active,
            n.is_approved,
            n.qq_number,
            n.wechat,
            n.mail,
            n.created_at,
            n.updated_at
        FROM shared_nodes n
        {where_sql}
        ORDER BY n.created_at DESC
        LIMIT ? OFFSET ?"
    );

    let rows: Vec<SharedNodeRow> = query_all(env, &list_sql, &list_params).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(build_node_response(env, config, row, false).await?);
    }

    let total_pages = if total == 0 {
        0
    } else {
        ((total + u64::from(per_page) - 1) / u64::from(per_page)) as u32
    };

    Ok(PaginatedResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
    })
}

pub async fn get_all_tags(env: &worker::Env) -> AppResult<Vec<String>> {
    let rows: Vec<TagRow> = query_all(
        env,
        "SELECT DISTINCT tag FROM node_tags ORDER BY tag ASC",
        &[],
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.tag).collect())
}

pub async fn get_node_health(
    env: &worker::Env,
    node_id: i32,
    pagination: &PaginationParams,
    filters: &HealthFilterParams,
) -> AppResult<PaginatedResponse<HealthRecordResponse>> {
    fetch_node_row(env, node_id, true).await?;

    let page = pagination.page();
    let per_page = pagination.per_page(20, 200);
    let offset = (page - 1) * per_page;

    let (where_sql, params) = build_health_filters(node_id, filters);
    let count_sql = format!("SELECT COUNT(*) AS total FROM health_records h{where_sql}");
    let total = fetch_count(env, &count_sql, &params).await? as u64;

    let mut list_params = params.clone();
    list_params.push(json!(per_page));
    list_params.push(json!(offset));

    let list_sql = format!(
        "SELECT
            h.id,
            h.node_id,
            h.status,
            h.response_time,
            h.error_message,
            h.checked_at
        FROM health_records h
        {where_sql}
        ORDER BY h.checked_at DESC
        LIMIT ? OFFSET ?"
    );

    let rows: Vec<HealthRecordRow> = query_all(env, &list_sql, &list_params).await?;
    let items = rows.into_iter().map(Into::into).collect::<Vec<_>>();

    let total_pages = if total == 0 {
        0
    } else {
        ((total + u64::from(per_page) - 1) / u64::from(per_page)) as u32
    };

    Ok(PaginatedResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
    })
}

pub async fn get_node_health_stats(
    env: &worker::Env,
    node_id: i32,
    hours: i64,
) -> AppResult<HealthStatsResponse> {
    fetch_node_row(env, node_id, true).await?;
    let records = get_recent_health_records(env, node_id, hours).await?;
    Ok(build_health_stats(&records))
}

pub async fn get_node_connect_url(env: &worker::Env, node_id: i32) -> AppResult<String> {
    let row = fetch_node_row(env, node_id, true).await?;
    Ok(format!("{}://{}:{}", row.protocol, row.host, row.port))
}

pub async fn set_node_approval(
    env: &worker::Env,
    node_id: i32,
    approved: bool,
) -> AppResult<NodeResponse> {
    fetch_node_row(env, node_id, false).await?;
    let db = env.d1(DB_BINDING)?;
    let timestamp = now_iso();
    query!(
        &db,
        "UPDATE shared_nodes SET is_approved = ?, updated_at = ? WHERE id = ?",
        &approved,
        &timestamp,
        &node_id
    )?
    .run()
    .await?;
    get_node(env, node_id, false).await
}

pub async fn update_node(
    env: &worker::Env,
    node_id: i32,
    request: &UpdateNodeRequest,
) -> AppResult<NodeResponse> {
    let current = fetch_node_row(env, node_id, false).await?;

    let name = request.name.clone().unwrap_or(current.name.clone());
    let host = request.host.clone().unwrap_or(current.host.clone());
    let port = request.port.unwrap_or(current.port);
    let protocol = request.protocol.clone().unwrap_or(current.protocol.clone());
    let description = request
        .description
        .clone()
        .or(current.description.clone())
        .unwrap_or_default();
    let max_connections = request.max_connections.unwrap_or(current.max_connections);
    let is_active = request.is_active.unwrap_or(current.is_active != 0);
    let allow_relay = request.allow_relay.unwrap_or(current.allow_relay != 0);
    let network_name = request
        .network_name
        .clone()
        .or(current.network_name.clone())
        .unwrap_or_default();
    let network_secret = request
        .network_secret
        .clone()
        .or(current.network_secret.clone())
        .unwrap_or_default();
    let qq_number = request
        .qq_number
        .clone()
        .or(current.qq_number.clone())
        .unwrap_or_default();
    let wechat = request
        .wechat
        .clone()
        .or(current.wechat.clone())
        .unwrap_or_default();
    let mail = request
        .mail
        .clone()
        .or(current.mail.clone())
        .unwrap_or_default();

    let changed_identity =
        host != current.host || port != current.port || protocol != current.protocol;
    if changed_identity && node_exists(env, &host, port, &protocol).await? {
        return Err(AppError::Conflict(
            "a node with the same host, port and protocol already exists".to_string(),
        ));
    }

    let db = env.d1(DB_BINDING)?;
    let timestamp = now_iso();
    query!(
        &db,
        "UPDATE shared_nodes
         SET
            name = ?,
            host = ?,
            port = ?,
            protocol = ?,
            description = ?,
            max_connections = ?,
            is_active = ?,
            allow_relay = ?,
            network_name = ?,
            network_secret = ?,
            qq_number = ?,
            wechat = ?,
            mail = ?,
            updated_at = ?
         WHERE id = ?",
        &name,
        &host,
        &port,
        &protocol,
        &description,
        &max_connections,
        &is_active,
        &allow_relay,
        &network_name,
        &network_secret,
        &qq_number,
        &wechat,
        &mail,
        &timestamp,
        &node_id
    )?
    .run()
    .await?;

    if let Some(tags) = &request.tags {
        replace_node_tags(env, node_id, tags).await?;
    }

    get_node(env, node_id, false).await
}

pub async fn delete_node(env: &worker::Env, node_id: i32) -> AppResult<()> {
    fetch_node_row(env, node_id, false).await?;
    let db = env.d1(DB_BINDING)?;
    query!(&db, "DELETE FROM shared_nodes WHERE id = ?", &node_id)?
        .run()
        .await?;
    Ok(())
}

fn build_node_filters(filters: &NodeFilterParams, public_only: bool) -> (String, Vec<Value>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();

    if public_only {
        clauses.push("n.is_approved = 1".to_string());
    }

    if let Some(is_active) = filters.is_active {
        clauses.push("n.is_active = ?".to_string());
        params.push(json!(if is_active { 1 } else { 0 }));
    }

    if let Some(protocol) = filters
        .protocol
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        clauses.push("LOWER(n.protocol) = LOWER(?)".to_string());
        params.push(json!(protocol));
    }

    if let Some(search) = filters
        .search
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        let search = format!("%{}%", search.trim());
        clauses.push(
            "(n.name LIKE ? OR n.host LIKE ? OR COALESCE(n.description, '') LIKE ?)".to_string(),
        );
        params.push(json!(search));
        params.push(json!(search));
        params.push(json!(search));
    }

    if !filters.tags.is_empty() {
        clauses.push(format!(
            "n.id IN (SELECT DISTINCT node_id FROM node_tags WHERE tag IN ({}))",
            placeholders(filters.tags.len())
        ));
        for tag in &filters.tags {
            params.push(json!(tag));
        }
    }

    if clauses.is_empty() {
        (String::new(), params)
    } else {
        (format!(" WHERE {}", clauses.join(" AND ")), params)
    }
}

fn build_admin_node_filters(filters: &AdminNodeFilterParams) -> (String, Vec<Value>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();

    if let Some(is_active) = filters.is_active {
        clauses.push("n.is_active = ?".to_string());
        params.push(json!(if is_active { 1 } else { 0 }));
    }

    if let Some(is_approved) = filters.is_approved {
        clauses.push("n.is_approved = ?".to_string());
        params.push(json!(if is_approved { 1 } else { 0 }));
    }

    if let Some(protocol) = filters
        .protocol
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        clauses.push("LOWER(n.protocol) = LOWER(?)".to_string());
        params.push(json!(protocol));
    }

    if let Some(search) = filters
        .search
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        let search = format!("%{}%", search.trim());
        clauses.push(
            "(n.name LIKE ? OR n.host LIKE ? OR COALESCE(n.description, '') LIKE ?)".to_string(),
        );
        params.push(json!(search));
        params.push(json!(search));
        params.push(json!(search));
    }

    let tags = normalize_filter_tags(
        filters.tag.clone(),
        filters.tags.clone().unwrap_or_default(),
    );
    if !tags.is_empty() {
        clauses.push(format!(
            "n.id IN (SELECT DISTINCT node_id FROM node_tags WHERE tag IN ({}))",
            placeholders(tags.len())
        ));
        for tag in tags {
            params.push(json!(tag));
        }
    }

    if clauses.is_empty() {
        (String::new(), params)
    } else {
        (format!(" WHERE {}", clauses.join(" AND ")), params)
    }
}

fn build_health_filters(node_id: i32, filters: &HealthFilterParams) -> (String, Vec<Value>) {
    let mut clauses = vec!["h.node_id = ?".to_string()];
    let mut params = vec![json!(node_id)];

    if let Some(status) = filters
        .status
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        clauses.push("LOWER(h.status) = LOWER(?)".to_string());
        params.push(json!(status));
    }

    if let Some(since) = filters
        .since
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        clauses.push("h.checked_at >= ?".to_string());
        params.push(json!(since));
    }

    (format!(" WHERE {}", clauses.join(" AND ")), params)
}

async fn build_node_response(
    env: &worker::Env,
    config: &AppConfig,
    row: SharedNodeRow,
    public_view: bool,
) -> AppResult<NodeResponse> {
    let latest = get_latest_health_record(env, row.id).await?;
    let recent_records = get_recent_health_records(env, row.id, config.ring_window_hours).await?;
    let stats = build_health_stats(&recent_records);
    let ring_reference = latest
        .as_ref()
        .and_then(|record| parse_timestamp(&record.checked_at).ok())
        .unwrap_or_else(Utc::now);
    let (total_ring, healthy_ring) = build_ring(
        &recent_records,
        ring_reference,
        config.ring_granularity_seconds,
        config.ring_window_hours,
    );
    let tags = get_node_tags(env, row.id).await?;

    let usage_percentage = if row.max_connections > 0 {
        (f64::from(row.current_connections) / f64::from(row.max_connections)) * 100.0
    } else {
        0.0
    };

    let (current_connections, max_connections) = if public_view {
        if row.max_connections > 0 {
            (
                row.current_connections.saturating_mul(100) / row.max_connections,
                100,
            )
        } else {
            (0, 0)
        }
    } else {
        (row.current_connections, row.max_connections)
    };

    Ok(NodeResponse {
        id: row.id,
        name: row.name,
        host: row.host.clone(),
        port: row.port,
        protocol: row.protocol.clone(),
        version: empty_to_none(row.version),
        description: empty_to_none(row.description),
        max_connections,
        current_connections,
        is_active: row.is_active != 0,
        is_approved: row.is_approved != 0,
        allow_relay: row.allow_relay != 0,
        network_name: if public_view {
            None
        } else {
            empty_to_none(row.network_name)
        },
        network_secret: if public_view {
            None
        } else {
            empty_to_none(row.network_secret)
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
        address: format!("{}://{}:{}", row.protocol, row.host, row.port),
        usage_percentage,
        current_health_status: latest.as_ref().map(|record| record.status.clone()),
        last_check_time: latest.as_ref().map(|record| record.checked_at.clone()),
        last_response_time: latest.as_ref().and_then(|record| record.response_time),
        health_percentage_24h: if recent_records.is_empty() {
            None
        } else {
            Some(stats.health_percentage)
        },
        health_record_total_counter_ring: total_ring,
        health_record_healthy_counter_ring: healthy_ring,
        ring_granularity: config.ring_granularity_seconds,
        qq_number: if public_view {
            None
        } else {
            empty_to_none(row.qq_number)
        },
        wechat: if public_view {
            None
        } else {
            empty_to_none(row.wechat)
        },
        mail: if public_view {
            None
        } else {
            empty_to_none(row.mail)
        },
        tags,
    })
}

fn build_health_stats(records: &[HealthRecordRow]) -> HealthStatsResponse {
    if records.is_empty() {
        return HealthStatsResponse::default();
    }

    let total_checks = records.len() as u64;
    let healthy_records = records
        .iter()
        .filter(|record| record.status.eq_ignore_ascii_case("healthy"))
        .collect::<Vec<_>>();
    let healthy_count = healthy_records.len() as u64;
    let unhealthy_count = total_checks.saturating_sub(healthy_count);

    let average_response_time = if healthy_records.is_empty() {
        None
    } else {
        let sum = healthy_records
            .iter()
            .filter_map(|record| record.response_time)
            .sum::<i32>();
        Some(f64::from(sum) / healthy_records.len() as f64)
    };

    let health_percentage = if total_checks == 0 {
        0.0
    } else {
        (healthy_count as f64 / total_checks as f64) * 100.0
    };

    let latest = records.first();

    HealthStatsResponse {
        total_checks,
        healthy_count,
        unhealthy_count,
        health_percentage,
        average_response_time,
        uptime_percentage: health_percentage,
        last_check_time: latest.map(|record| record.checked_at.clone()),
        last_status: latest.map(|record| record.status.clone()),
    }
}

fn build_ring(
    records: &[HealthRecordRow],
    reference: DateTime<Utc>,
    granularity_seconds: u32,
    window_hours: i64,
) -> (Vec<u64>, Vec<u64>) {
    let bucket_count = ((window_hours * 3600) / i64::from(granularity_seconds)).max(1) as usize;
    let mut total_ring = vec![0_u64; bucket_count];
    let mut healthy_ring = vec![0_u64; bucket_count];

    for record in records {
        let Ok(timestamp) = parse_timestamp(&record.checked_at) else {
            continue;
        };
        match reference.timestamp().cmp(&timestamp.timestamp()) {
            Ordering::Less => continue,
            Ordering::Equal | Ordering::Greater => {}
        }

        let diff_seconds = reference.timestamp() - timestamp.timestamp();
        let bucket = (diff_seconds / i64::from(granularity_seconds)) as usize;
        if bucket >= bucket_count {
            continue;
        }

        total_ring[bucket] += 1;
        if record.status.eq_ignore_ascii_case("healthy") {
            healthy_ring[bucket] += 1;
        }
    }

    (total_ring, healthy_ring)
}

async fn fetch_node_row(
    env: &worker::Env,
    node_id: i32,
    public_only: bool,
) -> AppResult<SharedNodeRow> {
    let db = env.d1(DB_BINDING)?;
    let sql = if public_only {
        "SELECT
            id,
            name,
            host,
            port,
            protocol,
            version,
            allow_relay,
            network_name,
            network_secret,
            description,
            max_connections,
            current_connections,
            is_active,
            is_approved,
            qq_number,
            wechat,
            mail,
            created_at,
            updated_at
         FROM shared_nodes
         WHERE id = ? AND is_approved = 1"
    } else {
        "SELECT
            id,
            name,
            host,
            port,
            protocol,
            version,
            allow_relay,
            network_name,
            network_secret,
            description,
            max_connections,
            current_connections,
            is_active,
            is_approved,
            qq_number,
            wechat,
            mail,
            created_at,
            updated_at
         FROM shared_nodes
         WHERE id = ?"
    };

    let statement = query!(&db, sql, &node_id)?;
    statement
        .first::<SharedNodeRow>(None)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("node {node_id} not found")))
}

async fn get_node_tags(env: &worker::Env, node_id: i32) -> AppResult<Vec<String>> {
    let rows: Vec<TagRow> = query_all(
        env,
        "SELECT tag FROM node_tags WHERE node_id = ? ORDER BY tag ASC",
        &[json!(node_id)],
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.tag).collect())
}

async fn replace_node_tags(env: &worker::Env, node_id: i32, tags: &[String]) -> AppResult<()> {
    let db = env.d1(DB_BINDING)?;
    query!(&db, "DELETE FROM node_tags WHERE node_id = ?", &node_id)?
        .run()
        .await?;

    let timestamp = now_iso();
    for tag in normalize_tags(tags) {
        query!(
            &db,
            "INSERT OR IGNORE INTO node_tags (node_id, tag, created_at) VALUES (?, ?, ?)",
            &node_id,
            &tag,
            &timestamp
        )?
        .run()
        .await?;
    }

    Ok(())
}

async fn get_latest_health_record(
    env: &worker::Env,
    node_id: i32,
) -> AppResult<Option<HealthRecordRow>> {
    query_first(
        env,
        "SELECT
            id,
            node_id,
            status,
            response_time,
            error_message,
            checked_at
         FROM health_records
         WHERE node_id = ?
         ORDER BY checked_at DESC
         LIMIT 1",
        &[json!(node_id)],
    )
    .await
}

async fn get_recent_health_records(
    env: &worker::Env,
    node_id: i32,
    hours: i64,
) -> AppResult<Vec<HealthRecordRow>> {
    let since =
        (Utc::now() - Duration::hours(hours)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    query_all(
        env,
        "SELECT
            id,
            node_id,
            status,
            response_time,
            error_message,
            checked_at
         FROM health_records
         WHERE node_id = ? AND checked_at >= ?
         ORDER BY checked_at DESC",
        &[json!(node_id), json!(since)],
    )
    .await
}

async fn node_exists(env: &worker::Env, host: &str, port: i32, protocol: &str) -> AppResult<bool> {
    let row: Option<CountRow> = query_first(
        env,
        "SELECT COUNT(*) AS total
         FROM shared_nodes
         WHERE host = ? AND port = ? AND protocol = ?",
        &[json!(host), json!(port), json!(protocol)],
    )
    .await?;
    Ok(row.is_some_and(|row| row.total > 0))
}

async fn fetch_count(env: &worker::Env, sql: &str, params: &[Value]) -> AppResult<i64> {
    let row: Option<CountRow> = query_first(env, sql, params).await?;
    Ok(row.map_or(0, |row| row.total))
}

async fn query_first<T>(env: &worker::Env, sql: &str, params: &[Value]) -> AppResult<Option<T>>
where
    T: DeserializeOwned,
{
    let db = env.d1(DB_BINDING)?;
    let statement = bind_dynamic(&db, sql, params)?;
    statement.first(None).await.map_err(AppError::from)
}

async fn query_all<T>(env: &worker::Env, sql: &str, params: &[Value]) -> AppResult<Vec<T>>
where
    T: DeserializeOwned,
{
    let db = env.d1(DB_BINDING)?;
    let statement = bind_dynamic(&db, sql, params)?;
    let result = statement.all().await?;
    result.results().map_err(AppError::from)
}

fn bind_dynamic(
    db: &D1Database,
    sql: &str,
    params: &[Value],
) -> AppResult<worker::D1PreparedStatement> {
    let serializer =
        worker::d1::serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
    let mut bindings = Vec::with_capacity(params.len());
    for value in params {
        bindings.push(
            value
                .serialize(&serializer)
                .map_err(|error| AppError::Internal(error.to_string()))?,
        );
    }

    db.prepare(sql).bind(&bindings).map_err(AppError::from)
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_timestamp(value: &str) -> AppResult<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| AppError::Internal(error.to_string()))
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for tag in tags {
        let trimmed = tag.trim();
        if !trimmed.is_empty() {
            set.insert(trimmed.to_string());
        }
    }
    set.into_iter().collect()
}

fn normalize_filter_tags(single_tag: Option<String>, tags: Vec<String>) -> Vec<String> {
    let mut merged = tags;
    if let Some(single_tag) = single_tag {
        merged.push(single_tag);
    }
    normalize_tags(&merged)
}

impl From<HealthRecordRow> for HealthRecordResponse {
    fn from(value: HealthRecordRow) -> Self {
        Self {
            id: value.id,
            node_id: value.node_id,
            status: value.status,
            response_time: value.response_time,
            error_message: empty_to_none(value.error_message),
            checked_at: value.checked_at,
        }
    }
}
