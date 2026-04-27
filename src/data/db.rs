use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::models::{ModelTier, TokenRecord};

#[allow(dead_code)]
const SCHEMA_VERSION: i64 = 3;

struct FileInfo {
    path: PathBuf,
    mtime_secs: i64,
    file_size: i64,
}

struct IngestedFileInfo {
    mtime_secs: i64,
    file_size: i64,
}

fn open_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open database at {}", db_path.display()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA synchronous=NORMAL;")?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<()> {
    let has_schema: bool = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version'")?
        .exists([])?;

    if !has_schema {
        create_schema_v3(conn)?;
        return Ok(());
    }

    let version: i64 =
        conn.query_row("SELECT version FROM schema_version", [], |row| row.get(0))?;

    if version < 2 {
        migrate_v1_to_v2(conn)?;
    }
    if version < 3 {
        migrate_v2_to_v3(conn)?;
    }

    Ok(())
}

fn create_schema_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version (version) VALUES (3);

        CREATE TABLE ingested_files (
            file_path   TEXT PRIMARY KEY,
            mtime_secs  INTEGER NOT NULL,
            file_size   INTEGER NOT NULL,
            ingested_at TEXT NOT NULL
        );

        CREATE TABLE token_records (
            id                          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp                   TEXT NOT NULL,
            session_id                  TEXT NOT NULL,
            project_name                TEXT NOT NULL,
            model                       TEXT NOT NULL,
            model_raw                   TEXT NOT NULL DEFAULT '',
            input_tokens                INTEGER NOT NULL,
            output_tokens               INTEGER NOT NULL,
            cache_creation_input_tokens INTEGER NOT NULL,
            cache_read_input_tokens     INTEGER NOT NULL,
            source_file                 TEXT NOT NULL,
            source_type                 TEXT NOT NULL DEFAULT 'claude'
        );

        CREATE INDEX idx_records_timestamp ON token_records(timestamp);
        CREATE INDEX idx_records_session ON token_records(session_id);
        CREATE INDEX idx_records_project ON token_records(project_name);
        CREATE INDEX idx_records_source ON token_records(source_file);
        CREATE INDEX idx_records_source_type ON token_records(source_type);

        CREATE TABLE ingested_opencode (
            db_mtime_secs  INTEGER NOT NULL,
            db_file_size   INTEGER NOT NULL,
            message_count  INTEGER NOT NULL,
            ingested_at    TEXT NOT NULL
        );

        CREATE TABLE opencode_ingested_messages (
            message_id TEXT PRIMARY KEY
        );
        ",
    )?;
    Ok(())
}

fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        ALTER TABLE token_records ADD COLUMN source_type TEXT NOT NULL DEFAULT 'claude';
        CREATE INDEX idx_records_source_type ON token_records(source_type);

        CREATE TABLE ingested_opencode (
            db_mtime_secs  INTEGER NOT NULL,
            db_file_size   INTEGER NOT NULL,
            message_count  INTEGER NOT NULL,
            ingested_at    TEXT NOT NULL
        );

        CREATE TABLE opencode_ingested_messages (
            message_id TEXT PRIMARY KEY
        );

        UPDATE schema_version SET version = 2;
        ",
    )?;
    Ok(())
}

/// v2 → v3: add `model_raw` column for LiteLLM-style per-model pricing lookup.
/// Existing rows get empty string — cost calc will fall back to tier-based pricing
/// for old rows, and new rows (re-ingested from JSONL) will have the raw name.
fn migrate_v2_to_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        ALTER TABLE token_records ADD COLUMN model_raw TEXT NOT NULL DEFAULT '';
        UPDATE schema_version SET version = 3;
        ",
    )?;
    Ok(())
}

fn stat_files(paths: &[PathBuf]) -> Vec<FileInfo> {
    paths
        .iter()
        .filter_map(|p| {
            let meta = std::fs::metadata(p).ok()?;
            use std::os::unix::fs::MetadataExt;
            Some(FileInfo {
                path: p.clone(),
                mtime_secs: meta.mtime(),
                file_size: meta.size() as i64,
            })
        })
        .collect()
}

fn classify_files(conn: &Connection, disk_files: &[FileInfo]) -> Result<Vec<usize>> {
    let mut stmt = conn.prepare("SELECT file_path, mtime_secs, file_size FROM ingested_files")?;
    let db_files: std::collections::HashMap<String, IngestedFileInfo> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                IngestedFileInfo {
                    mtime_secs: row.get(1)?,
                    file_size: row.get(2)?,
                },
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut need_ingestion: Vec<usize> = Vec::new();

    for (i, fi) in disk_files.iter().enumerate() {
        let path_str = fi.path.to_string_lossy().to_string();

        match db_files.get(&path_str) {
            None => need_ingestion.push(i),
            Some(db_info) => {
                if db_info.mtime_secs != fi.mtime_secs || db_info.file_size != fi.file_size {
                    need_ingestion.push(i);
                }
            }
        }
    }

    Ok(need_ingestion)
}

fn insert_records(
    conn: &Connection,
    source_file: &str,
    records: &[TokenRecord],
    mtime_secs: i64,
    file_size: i64,
    source_type: &str,
) -> Result<()> {
    let sql = format!(
        "INSERT INTO token_records (
            timestamp, session_id, project_name, model, model_raw,
            input_tokens, output_tokens,
            cache_creation_input_tokens, cache_read_input_tokens,
            source_file, source_type
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, '{}')",
        source_type
    );
    let mut stmt = conn.prepare_cached(&sql)?;

    for r in records {
        stmt.execute(params![
            r.timestamp.to_rfc3339(),
            r.session_id,
            r.project_name,
            r.model.as_db_str(),
            r.model_raw,
            r.input_tokens,
            r.output_tokens,
            r.cache_creation_input_tokens,
            r.cache_read_input_tokens,
            source_file,
        ])?;
    }

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO ingested_files (file_path, mtime_secs, file_size, ingested_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![source_file, mtime_secs, file_size, now],
    )?;

    Ok(())
}

fn query_records(
    conn: &Connection,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    project_filter: Option<&str>,
    source_type_filter: Option<&str>,
) -> Result<Vec<TokenRecord>> {
    let mut sql = String::from(
        "SELECT timestamp, session_id, project_name, model, model_raw,
                input_tokens, output_tokens,
                cache_creation_input_tokens, cache_read_input_tokens
         FROM token_records WHERE 1=1",
    );
    let mut param_values: Vec<String> = Vec::new();

    if let Some(s) = since {
        param_values.push(s.to_rfc3339());
        sql.push_str(&format!(" AND timestamp >= ?{}", param_values.len()));
    }
    if let Some(u) = until {
        param_values.push(u.to_rfc3339());
        sql.push_str(&format!(" AND timestamp <= ?{}", param_values.len()));
    }
    if let Some(pf) = project_filter {
        param_values.push(format!("%{}%", pf));
        sql.push_str(&format!(" AND project_name LIKE ?{}", param_values.len()));
    }
    if let Some(st) = source_type_filter {
        param_values.push(st.to_string());
        sql.push_str(&format!(" AND source_type = ?{}", param_values.len()));
    }

    sql.push_str(" ORDER BY timestamp");

    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();

    let records = stmt
        .query_map(params.as_slice(), |row| {
            let ts_str: String = row.get(0)?;
            let timestamp = ts_str
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            Ok(TokenRecord {
                timestamp,
                session_id: row.get(1)?,
                project_name: row.get(2)?,
                model: ModelTier::from_db_str(&row.get::<_, String>(3)?),
                model_raw: row.get::<_, String>(4).unwrap_or_default(),
                input_tokens: row.get(5)?,
                output_tokens: row.get(6)?,
                cache_creation_input_tokens: row.get(7)?,
                cache_read_input_tokens: row.get(8)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(records)
}

fn ingest_claude_files(conn: &Connection, jsonl_files: &[PathBuf], quiet: bool) -> Result<()> {
    let disk_files = stat_files(jsonl_files);
    let need_ingestion_indices = classify_files(conn, &disk_files)?;

    if !need_ingestion_indices.is_empty() {
        if !quiet {
            use colored::Colorize;
            eprintln!(
                "  {} Ingesting {} new/changed files into SQLite cache...",
                ">>".green().bold(),
                need_ingestion_indices.len()
            );
        }

        let files_to_parse: Vec<&FileInfo> = need_ingestion_indices
            .iter()
            .map(|&i| &disk_files[i])
            .collect();

        let parsed: Vec<(&FileInfo, Vec<TokenRecord>)> = files_to_parse
            .par_iter()
            .filter_map(|fi| {
                let records =
                    crate::data::jsonl::parse_single_file(&fi.path, None, None, None).ok()?;
                Some((*fi, records))
            })
            .collect();

        conn.execute_batch("BEGIN")?;

        for (fi, records) in &parsed {
            let path_str = fi.path.to_string_lossy().to_string();
            conn.execute(
                "DELETE FROM token_records WHERE source_file = ?1",
                params![path_str],
            )?;
            conn.execute(
                "DELETE FROM ingested_files WHERE file_path = ?1",
                params![path_str],
            )?;
            insert_records(
                conn,
                &path_str,
                records,
                fi.mtime_secs,
                fi.file_size,
                "claude",
            )?;
        }

        conn.execute_batch("COMMIT")?;
    }

    Ok(())
}

fn ingest_gemini_files(conn: &Connection, gemini_files: &[PathBuf], quiet: bool) -> Result<()> {
    let disk_files = stat_files(gemini_files);
    let need_ingestion_indices = classify_files(conn, &disk_files)?;

    if !need_ingestion_indices.is_empty() {
        if !quiet {
            use colored::Colorize;
            eprintln!(
                "  {} Ingesting {} new/changed Gemini session files...",
                ">>".green().bold(),
                need_ingestion_indices.len()
            );
        }

        let files_to_parse: Vec<&FileInfo> = need_ingestion_indices
            .iter()
            .map(|&i| &disk_files[i])
            .collect();

        let parsed: Vec<(&FileInfo, Vec<TokenRecord>)> = files_to_parse
            .par_iter()
            .filter_map(|fi| {
                let records =
                    crate::data::gemini::parse_gemini_session(&fi.path, None, None, None).ok()?;
                Some((*fi, records))
            })
            .collect();

        conn.execute_batch("BEGIN")?;

        for (fi, records) in &parsed {
            let path_str = fi.path.to_string_lossy().to_string();
            conn.execute(
                "DELETE FROM token_records WHERE source_file = ?1",
                params![path_str],
            )?;
            conn.execute(
                "DELETE FROM ingested_files WHERE file_path = ?1",
                params![path_str],
            )?;
            insert_records(
                conn,
                &path_str,
                records,
                fi.mtime_secs,
                fi.file_size,
                "gemini",
            )?;
        }

        conn.execute_batch("COMMIT")?;
    }

    Ok(())
}

pub fn ingest_opencode(conn: &Connection, opencode_db_path: &Path, quiet: bool) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::metadata(opencode_db_path)?;
    let current_mtime = meta.mtime();
    let current_size = meta.size() as i64;

    let needs_full_ingestion = {
        let mut stmt = conn.prepare("SELECT db_mtime_secs, db_file_size FROM ingested_opencode")?;
        let existing: Option<(i64, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .next();

        match existing {
            None => true,
            Some((m, s)) => m != current_mtime || s != current_size,
        }
    };

    if !needs_full_ingestion {
        return Ok(());
    }

    let oc_conn = Connection::open(opencode_db_path).with_context(|| {
        format!(
            "Failed to open OpenCode database at {}",
            opencode_db_path.display()
        )
    })?;

    let already_ingested: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT message_id FROM opencode_ingested_messages")?;
        let ids: std::collections::HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        ids
    };

    let mut oc_stmt = oc_conn.prepare(
        "SELECT m.id, m.time_created, m.session_id, m.data, s.directory
         FROM message m
         JOIN session s ON m.session_id = s.id
         WHERE json_extract(m.data, '$.role') = 'assistant'
         ORDER BY m.time_created",
    )?;

    let mut new_count = 0usize;

    conn.execute_batch("BEGIN")?;

    let rows = oc_stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let time_ms: i64 = row.get(1)?;
        let session_id: String = row.get(2)?;
        let data_json: String = row.get(3)?;
        let directory: String = row.get(4)?;
        Ok((id, time_ms, session_id, data_json, directory))
    })?;

    let mut insert_stmt = conn.prepare_cached(
        "INSERT INTO token_records (
            timestamp, session_id, project_name, model, model_raw,
            input_tokens, output_tokens,
            cache_creation_input_tokens, cache_read_input_tokens,
            source_file, source_type
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'opencode')",
    )?;

    let mut msg_stmt = conn.prepare_cached(
        "INSERT OR IGNORE INTO opencode_ingested_messages (message_id) VALUES (?1)",
    )?;

    for row_result in rows {
        let (msg_id, time_ms, session_id, data_json, directory) = match row_result {
            Ok(r) => r,
            Err(_) => continue,
        };

        if already_ingested.contains(&msg_id) {
            continue;
        }

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
            .timestamp_millis_opt(time_ms)
            .single()
            .unwrap_or_else(Utc::now);

        let project_name = std::path::Path::new(&directory)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| directory.clone());

        let source_file = format!("opencode:{}", msg_id);

        insert_stmt.execute(params![
            timestamp.to_rfc3339(),
            session_id,
            project_name,
            tier.as_db_str(),
            model_id,
            input,
            output,
            cache_write,
            cache_read,
            source_file,
        ])?;

        msg_stmt.execute(params![msg_id])?;
        new_count += 1;
    }

    let now = Utc::now().to_rfc3339();
    drop(msg_stmt);
    drop(insert_stmt);

    conn.execute("DELETE FROM ingested_opencode", [])?;
    conn.execute(
        "INSERT INTO ingested_opencode (db_mtime_secs, db_file_size, message_count, ingested_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![current_mtime, current_size, new_count as i64, now],
    )?;

    conn.execute_batch("COMMIT")?;

    if new_count > 0 && !quiet {
        use colored::Colorize;
        eprintln!(
            "  {} Ingested {} new OpenCode messages into cache",
            ">>".green().bold(),
            new_count
        );
    }

    Ok(())
}

pub fn load_records(
    db_path: &Path,
    jsonl_files: &[PathBuf],
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    project_filter: Option<&str>,
    rebuild: bool,
    quiet: bool,
) -> Result<Vec<TokenRecord>> {
    if rebuild && db_path.exists() {
        std::fs::remove_file(db_path)?;
    }

    let conn = open_db(db_path)?;
    ensure_schema(&conn)?;

    ingest_claude_files(&conn, jsonl_files, quiet)?;

    query_records(&conn, since, until, project_filter, None)
}

#[allow(clippy::too_many_arguments)]
pub fn load_all_records(
    ccguilt_db_path: &Path,
    jsonl_files: &[PathBuf],
    opencode_db_path: Option<&Path>,
    gemini_files: &[PathBuf],
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    project_filter: Option<&str>,
    rebuild: bool,
    quiet: bool,
    source_type_filter: Option<&str>,
) -> Result<Vec<TokenRecord>> {
    if rebuild && ccguilt_db_path.exists() {
        std::fs::remove_file(ccguilt_db_path)?;
    }

    let conn = open_db(ccguilt_db_path)?;
    ensure_schema(&conn)?;

    if source_type_filter != Some("opencode") && source_type_filter != Some("gemini") {
        ingest_claude_files(&conn, jsonl_files, quiet)?;
    }

    if source_type_filter != Some("claude") && source_type_filter != Some("gemini") {
        if let Some(oc_path) = opencode_db_path {
            if oc_path.exists() {
                ingest_opencode(&conn, oc_path, quiet)?;
            }
        }
    }

    if source_type_filter != Some("claude") && source_type_filter != Some("opencode") {
        ingest_gemini_files(&conn, gemini_files, quiet)?;
    }

    query_records(&conn, since, until, project_filter, source_type_filter)
}

#[allow(dead_code)]
pub fn count_opencode_cached(conn: &Connection) -> Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM token_records WHERE source_type = 'opencode'",
        [],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

#[allow(dead_code)]
pub fn count_claude_cached(conn: &Connection) -> Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM token_records WHERE source_type = 'claude'",
        [],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}
