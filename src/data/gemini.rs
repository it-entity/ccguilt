use anyhow::Result;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;

use crate::models::{ModelTier, TokenRecord};

#[derive(serde::Deserialize)]
struct GeminiSession {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "projectHash")]
    project_hash: String,
    #[allow(dead_code)]
    #[serde(rename = "startTime")]
    start_time: String,
    messages: Vec<GeminiMessage>,
}

#[derive(serde::Deserialize)]
struct GeminiMessage {
    #[serde(rename = "type")]
    msg_type: String,
    timestamp: String,
    model: Option<String>,
    tokens: Option<GeminiTokens>,
    #[allow(dead_code)]
    id: Option<String>,
}

#[derive(serde::Deserialize)]
struct GeminiTokens {
    input: u64,
    output: u64,
    cached: u64,
    thoughts: u64,
    tool: u64,
    total: u64,
}

fn resolve_project_name(project_hash: &str, hash_to_project: &HashMap<String, String>) -> String {
    if let Some(name) = hash_to_project.get(project_hash) {
        return name.clone();
    }
    project_hash.chars().take(8).collect()
}

pub fn parse_gemini_session(
    path: &Path,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    project_filter: Option<&str>,
) -> Result<Vec<TokenRecord>> {
    let content = std::fs::read_to_string(path)?;
    let session: GeminiSession = serde_json::from_str(&content)?;

    let hash_to_project = HashMap::new();
    let project_name = resolve_project_name(&session.project_hash, &hash_to_project);

    if let Some(filter) = project_filter {
        if !project_name.contains(filter) {
            return Ok(Vec::new());
        }
    }

    let mut records = Vec::new();

    for msg in &session.messages {
        if msg.msg_type != "gemini" {
            continue;
        }

        let tokens = match &msg.tokens {
            Some(t) if t.total > 0 => t,
            _ => continue,
        };

        let model_str = match &msg.model {
            Some(m) => m.as_str(),
            None => continue,
        };

        let tier = match ModelTier::from_model_string(model_str) {
            Some(t) => t,
            None => continue,
        };

        let timestamp: DateTime<Utc> = match msg.timestamp.parse() {
            Ok(t) => t,
            Err(_) => continue,
        };

        if let Some(s) = since {
            if timestamp < s {
                continue;
            }
        }
        if let Some(u) = until {
            if timestamp > u {
                continue;
            }
        }

        let output_with_thoughts = tokens.output + tokens.thoughts + tokens.tool;

        records.push(TokenRecord {
            timestamp,
            session_id: session.session_id.clone(),
            project_name: project_name.clone(),
            model: tier,
            model_raw: model_str.to_string(),
            input_tokens: tokens.input,
            output_tokens: output_with_thoughts,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: tokens.cached,
        });
    }

    Ok(records)
}

pub fn parse_gemini_files(
    files: &[std::path::PathBuf],
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    project_filter: Option<&str>,
) -> Result<Vec<TokenRecord>> {
    let results: Vec<Vec<TokenRecord>> = files
        .par_iter()
        .filter_map(|file| parse_gemini_session(file, since, until, project_filter).ok())
        .collect();

    let mut all_records: Vec<TokenRecord> = results.into_iter().flatten().collect();
    all_records.sort_by_key(|r| r.timestamp);
    Ok(all_records)
}
