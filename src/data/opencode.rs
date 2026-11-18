use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;
use std::path::Path;

use crate::models::{ModelTier, TokenRecord};

pub fn parse_opencode_db(
    db_path: &Path,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    project_filter: Option<&str>,
) -> Result<Vec<TokenRecord>> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open OpenCode database at {}", db_path.display()))?;

    let mut sql = String::from(
        "SELECT m.time_created, m.session_id, m.data, s.directory
         FROM message m
         JOIN session s ON m.session_id = s.id
         WHERE json_extract(m.data, '$.role') = 'assistant'",
    );

    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(s) = since {
        param_values.push(Box::new(s.timestamp_millis()));
        sql.push_str(&format!(" AND m.time_created >= ?{}", param_values.len()));
    }

    if let Some(u) = until {
        param_values.push(Box::new(u.timestamp_millis()));
        sql.push_str(&format!(" AND m.time_created < ?{}", param_values.len()));
    }

    if let Some(pf) = project_filter {
        param_values.push(Box::new(format!("%{}%", pf)));
        sql.push_str(&format!(" AND s.directory LIKE ?{}", param_values.len()));
    }

    sql.push_str(" ORDER BY m.time_created");

    let params: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|v| v.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;

    let rows = stmt.query_map(params.as_slice(), |row| {
        let time_created_ms: i64 = row.get(0)?;
        let session_id: String = row.get(1)?;
        let data_json: String = row.get(2)?;
        let directory: String = row.get(3)?;
        Ok((time_created_ms, session_id, data_json, directory))
    })?;

    let mut records = Vec::new();

    for row_result in rows {
        let (time_created_ms, session_id, data_json, directory) = match row_result {
            Ok(r) => r,
            Err(_) => continue,
        };

        let data: serde_json::Value = match serde_json::from_str(&data_json) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let model_id = match data.get("modelID").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => continue,
        };

        let tier = match ModelTier::from_model_string(model_id) {
            Some(t) => t,
            None => continue,
        };

        let tokens = match data.get("tokens") {
            Some(t) => t,
            None => continue,
        };

        let input = tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
        let output = tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_read = tokens
            .get("cache")
            .and_then(|c| c.get("read"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_write = tokens
            .get("cache")
            .and_then(|c| c.get("write"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
            continue;
        }

        let timestamp = Utc
            .timestamp_millis_opt(time_created_ms)
            .single()
            .unwrap_or_else(Utc::now);

        let project_name = extract_project_name(&directory);

        records.push(TokenRecord {
            timestamp,
            session_id,
            project_name,
            model: tier,
            model_raw: model_id.to_string(),
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: cache_write,
            cache_read_input_tokens: cache_read,
        });
    }

    Ok(records)
}

fn extract_project_name(directory: &str) -> String {
    std::path::Path::new(directory)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| directory.to_string())
}
