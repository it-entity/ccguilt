use anyhow::Result;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use crate::models::{ModelTier, TokenRecord};

/// Minimal deserialization struct for JSONL lines — we only care about assistant messages with usage
#[derive(Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    model: Option<String>,
    id: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

/// Parse all JSONL files in parallel, returning deduplicated token records
pub fn parse_jsonl_files(
    files: &[PathBuf],
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    project_filter: Option<&str>,
) -> Result<Vec<TokenRecord>> {
    let results: Vec<Vec<TokenRecord>> = files
        .par_iter()
        .filter_map(|path| parse_single_file(path, since, until, project_filter).ok())
        .collect();

    let mut all_records: Vec<TokenRecord> = results.into_iter().flatten().collect();
    all_records.sort_by_key(|r| r.timestamp);
    Ok(all_records)
}

pub fn parse_single_file(
    path: &PathBuf,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    _project_filter: Option<&str>,
) -> Result<Vec<TokenRecord>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::with_capacity(128 * 1024, file);

    // Extract session_id from filename (strip .jsonl)
    let session_id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Extract project name from parent directory
    let project_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Deduplicate by message ID — streaming causes multiple lines per response
    // The last line for a given message ID has the final cumulative token count
    let mut by_message_id: HashMap<String, TokenRecord> = HashMap::new();
    let mut records_without_id: Vec<TokenRecord> = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.is_empty() {
            continue;
        }

        let raw: RawLine = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Only process assistant messages
        if raw.msg_type.as_deref() != Some("assistant") {
            continue;
        }

        let message = match raw.message {
            Some(m) => m,
            None => continue,
        };

        // Skip synthetic model messages
        let model_str = match &message.model {
            Some(m) if m != "<synthetic>" => m.clone(),
            _ => continue,
        };

        let tier = match ModelTier::from_model_string(&model_str) {
            Some(t) => t,
            None => continue,
        };

        let usage = match message.usage {
            Some(u) => u,
            None => continue,
        };

        // Parse timestamp
        let timestamp = match &raw.timestamp {
            Some(ts) => match ts.parse::<DateTime<Utc>>() {
                Ok(dt) => dt,
                Err(_) => continue,
            },
            None => continue,
        };

        // Apply date filters
        if let Some(since) = since {
            if timestamp < since {
                continue;
            }
        }
        if let Some(until) = until {
            if timestamp > until {
                continue;
            }
        }

        let input = usage.input_tokens.unwrap_or(0);
        let output = usage.output_tokens.unwrap_or(0);
        let cache_create = usage.cache_creation_input_tokens.unwrap_or(0);
        let cache_read = usage.cache_read_input_tokens.unwrap_or(0);

        // Skip zero-token messages
        if input == 0 && output == 0 && cache_create == 0 && cache_read == 0 {
            continue;
        }

        let record = TokenRecord {
            timestamp,
            session_id: raw.session_id.clone().unwrap_or_else(|| session_id.clone()),
            project_name: project_name.clone(),
            model: tier,
            model_raw: model_str.clone(),
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: cache_create,
            cache_read_input_tokens: cache_read,
        };

        // Deduplicate by message.id — keep last occurrence (has final token count)
        match message.id {
            Some(id) => {
                by_message_id.insert(id, record);
            }
            None => {
                records_without_id.push(record);
            }
        }
    }

    let mut records: Vec<TokenRecord> = by_message_id.into_values().collect();
    records.append(&mut records_without_id);
    Ok(records)
}
