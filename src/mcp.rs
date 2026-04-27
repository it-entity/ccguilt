//! MCP (Model Context Protocol) server mode.
//!
//! Exposes ccguilt's data to Claude Code as callable tools so the model can
//! check the user's environmental impact mid-conversation. Also surfaces an
//! occasional "tree fallen" warning when a new tree's worth of CO2 has
//! accumulated since the last warning was issued.
//!
//! Run via: `ccguilt --mcp`
//! Register with: `claude mcp add ccguilt -- ccguilt --mcp`

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::aggregate::aggregate_with;
use crate::cli::Period;
use crate::config::{CO2_KG_PER_KWH, PUE};
use crate::data::db;
use crate::data::discovery::ClaudeDataDir;
use crate::dateparse::parse_natural_date;
use crate::models::TokenRecord;

/// Satirical tree-fallen warning quotes. `{n}` is replaced with the new tree count.
const TREE_WARNING_QUOTES: &[&str] = &[
    "🌳💀 BREAKING: Tree #{n} has fallen due to your AI usage. The forest mourns.",
    "🪓 Tree #{n} just hit the ground. Somewhere a squirrel is filing a complaint.",
    "🌲→🪵 You've now offset the annual CO2 absorption of {n} mature trees. Congrats?",
    "🚨 ALERT: Tree #{n} has been claimed. The remaining trees are nervous.",
    "📢 Update: {n} trees worth of CO2 absorption now needed to undo your AI habit.",
    "🌳 R.I.P. Tree #{n}. It absorbed 22kg of CO2 a year. You did that in tokens.",
    "🍃 Tree #{n} has joined the choir invisible. Cause of death: your prompts.",
    "⚰️ Tree #{n} has been carbon-offset out of existence. The remaining trees are forming a union.",
];

// ── Persistent state ───────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct McpState {
    last_warned_trees_floor: u64,
    last_warned_at: Option<String>,
    initialized: bool,
}

fn state_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("ccguilt")
        .join("mcp_state.json")
}

fn load_state() -> McpState {
    let path = state_path();
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        McpState::default()
    }
}

fn save_state(state: &McpState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(&path, json);
    }
}

// ── Output report shape ────────────────────────────────────────────

#[derive(Serialize)]
struct UsageReport {
    label: String,
    tokens: TokenReport,
    cost_usd: f64,
    energy_wh: f64,
    co2_grams: f64,
    water_liters: f64,
    trees_destroyed: f64,
    guilt_level: String,
    guilt_blurb: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tree_fallen_warning: Option<String>,
}

#[derive(Serialize)]
struct TokenReport {
    input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
    total: u64,
}

// ── Tool argument schemas ──────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RangeArgs {
    /// Start date — accepts YYYY-MM-DD, "7d", "2w", "yesterday", "last-week", "monday", etc.
    pub since: Option<String>,
    /// End date — same formats as `since`.
    pub until: Option<String>,
}

// ── Server state ───────────────────────────────────────────────────

#[derive(Clone)]
pub struct CcguiltServer {
    data_dir: PathBuf,
    tool_router: ToolRouter<CcguiltServer>,
}

impl CcguiltServer {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            tool_router: Self::tool_router(),
        }
    }

    fn load_records(
        &self,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<TokenRecord>> {
        let dd = ClaudeDataDir::new(self.data_dir.clone());
        let files = dd.jsonl_files(None);
        let db_path = self.data_dir.join("ccguilt.db");
        db::load_records(&db_path, &files, since, until, None, false, true)
    }

    fn build_report(&self, records: Vec<TokenRecord>, label: String) -> UsageReport {
        let buckets = aggregate_with(&records, Period::Total, CO2_KG_PER_KWH, PUE);

        let mut input = 0u64;
        let mut output = 0u64;
        let mut cache_create = 0u64;
        let mut cache_read = 0u64;
        let mut cost = 0.0;
        let mut energy = 0.0;
        let mut co2 = 0.0;
        let mut water = 0.0;
        let mut trees = 0.0;
        let mut guilt_level = String::from("DigitalSaint");
        let mut guilt_blurb = String::new();

        for b in &buckets {
            input += b.tokens.input_tokens;
            output += b.tokens.output_tokens;
            cache_create += b.tokens.cache_creation_tokens;
            cache_read += b.tokens.cache_read_tokens;
            cost += b.cost.total_cost_usd;
            energy += b.impact.energy_wh;
            co2 += b.impact.co2_grams;
            water += b.impact.water_ml / 1000.0;
            trees += b.impact.trees_destroyed;
            guilt_level = b.guilt.title.clone();
            guilt_blurb = b.guilt.description.clone();
        }

        let warning = self.compute_tree_warning();

        UsageReport {
            label,
            tokens: TokenReport {
                input,
                output,
                cache_creation: cache_create,
                cache_read,
                total: input + output + cache_create + cache_read,
            },
            cost_usd: cost,
            energy_wh: energy,
            co2_grams: co2,
            water_liters: water,
            trees_destroyed: trees,
            guilt_level,
            guilt_blurb,
            tree_fallen_warning: warning,
        }
    }

    /// Returns a warning message if a new tree has fallen since the last warning.
    /// Otherwise returns None. Persists state in `~/.local/share/ccguilt/mcp_state.json`.
    fn compute_tree_warning(&self) -> Option<String> {
        let all_records = self.load_records(None, None).ok()?;
        let buckets = aggregate_with(&all_records, Period::Total, CO2_KG_PER_KWH, PUE);
        let total_trees: f64 = buckets.iter().map(|b| b.impact.trees_destroyed).sum();
        let current_floor = total_trees.floor() as u64;

        let mut state = load_state();

        // First-run: initialize state to current floor without warning
        if !state.initialized {
            state.last_warned_trees_floor = current_floor;
            state.last_warned_at = Some(Utc::now().to_rfc3339());
            state.initialized = true;
            save_state(&state);
            return None;
        }

        if current_floor > state.last_warned_trees_floor {
            state.last_warned_trees_floor = current_floor;
            state.last_warned_at = Some(Utc::now().to_rfc3339());
            save_state(&state);

            use rand::Rng;
            let mut rng = rand::thread_rng();
            let idx = rng.gen_range(0..TREE_WARNING_QUOTES.len());
            Some(TREE_WARNING_QUOTES[idx].replace("{n}", &current_floor.to_string()))
        } else {
            None
        }
    }
}

// ── MCP tool definitions ───────────────────────────────────────────

#[tool_router]
impl CcguiltServer {
    #[tool(
        description = "Get today's Claude Code environmental impact: tokens used, USD cost, energy (Wh), CO2 (grams), water (liters), trees destroyed (fractional), and guilt level. Returns JSON. May include a 'tree_fallen_warning' field if a new tree's worth of CO2 has accumulated since the last warning."
    )]
    async fn ccguilt_today(&self) -> String {
        let now = Utc::now();
        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
        let tomorrow = today_start + Duration::days(1);
        match self.load_records(Some(today_start), Some(tomorrow)) {
            Ok(records) => {
                let report = self.build_report(records, "today".to_string());
                serde_json::to_string_pretty(&report)
                    .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
            }
            Err(e) => format!("{{\"error\":\"{e}\"}}"),
        }
    }

    #[tool(
        description = "Get all-time Claude Code environmental impact: cumulative tokens, USD cost, energy (Wh), CO2 (grams), water (liters), trees destroyed (fractional), and guilt level. Returns JSON. May include a 'tree_fallen_warning' field if a new tree's worth of CO2 has accumulated since the last warning."
    )]
    async fn ccguilt_total(&self) -> String {
        match self.load_records(None, None) {
            Ok(records) => {
                let report = self.build_report(records, "all-time".to_string());
                serde_json::to_string_pretty(&report)
                    .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
            }
            Err(e) => format!("{{\"error\":\"{e}\"}}"),
        }
    }

    #[tool(
        description = "Get Claude Code environmental impact for a custom date range. Both 'since' and 'until' are optional and accept formats like 'YYYY-MM-DD', '7d' (7 days ago), '2w' (2 weeks ago), 'last-week', 'yesterday', 'monday', etc. If both are omitted this is equivalent to ccguilt_total. Returns JSON."
    )]
    async fn ccguilt_range(&self, Parameters(args): Parameters<RangeArgs>) -> String {
        let since = match args.since.as_deref().map(parse_natural_date).transpose() {
            Ok(s) => s,
            Err(e) => return format!("{{\"error\":\"invalid since: {e}\"}}"),
        };
        let until = match args.until.as_deref().map(parse_natural_date).transpose() {
            Ok(u) => u,
            Err(e) => return format!("{{\"error\":\"invalid until: {e}\"}}"),
        };

        let label = format!(
            "{} to {}",
            since
                .map(|s| s.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "beginning".to_string()),
            until
                .map(|u| u.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "now".to_string()),
        );

        match self.load_records(since, until) {
            Ok(records) => {
                let report = self.build_report(records, label);
                serde_json::to_string_pretty(&report)
                    .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
            }
            Err(e) => format!("{{\"error\":\"{e}\"}}"),
        }
    }
}

// ── ServerHandler implementation ───────────────────────────────────

#[tool_handler]
impl ServerHandler for CcguiltServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "ccguilt — environmental impact tracker for Claude Code usage. \
             Use ccguilt_today, ccguilt_total, or ccguilt_range to check token \
             usage, cost, CO2, water, and trees destroyed. Each response may \
             include a 'tree_fallen_warning' field surfacing satirical milestones."
                .to_string(),
        );
        info
    }
}

// ── Server entrypoint ──────────────────────────────────────────────

pub async fn run_server() -> Result<()> {
    let data_dir = ClaudeDataDir::default_path()?;
    let server = CcguiltServer::new(data_dir);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

// ── One-shot MCP registration with Claude Code ─────────────────────

/// Auto-register ccguilt as an MCP server in Claude Code's user config.
/// Mirrors `--setup-completions` UX: one command, idempotent, friendly errors.
pub fn setup_mcp() -> Result<()> {
    use anyhow::{anyhow, Context};
    use colored::Colorize;
    use std::process::Command;

    eprintln!(
        "  {} Registering ccguilt MCP server with Claude Code...",
        ">>".yellow().bold()
    );

    // Verify `claude` CLI is on PATH
    let check = Command::new("claude").arg("--version").output();
    match check {
        Ok(out) if out.status.success() => {}
        _ => {
            return Err(anyhow!(
                "'claude' CLI not found on PATH.\n   \
                 Install Claude Code first: https://claude.com/code\n   \
                 Then re-run: ccguilt --setup-mcp"
            ));
        }
    }

    // Resolve absolute path to current executable so Claude Code spawns the right binary
    let self_exe =
        std::env::current_exe().context("could not determine current ccguilt executable path")?;

    // Idempotent: remove any existing registration (user OR local scope) so re-running this
    // upgrades a stale path or scope without complaint.
    let _ = Command::new("claude")
        .args(["mcp", "remove", "ccguilt", "-s", "user"])
        .output();
    let _ = Command::new("claude")
        .args(["mcp", "remove", "ccguilt", "-s", "local"])
        .output();

    // Register at user scope so it works in every project
    let output = Command::new("claude")
        .args([
            "mcp",
            "add",
            "--scope",
            "user",
            "ccguilt",
            "--",
            &self_exe.to_string_lossy(),
            "--mcp",
        ])
        .output()
        .context("failed to invoke `claude mcp add`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("`claude mcp add` failed: {}", stderr.trim()));
    }

    eprintln!(
        "  {} Registered: {} {}",
        ">>".green().bold(),
        "ccguilt".bold(),
        format!("({})", self_exe.display()).dimmed()
    );
    eprintln!(
        "  {} Tools: ccguilt_today, ccguilt_total, ccguilt_range",
        ">>".dimmed()
    );
    eprintln!(
        "  {} Open a new Claude Code session and try: \"how much CO2 have I burned today?\"",
        ">>".dimmed()
    );

    Ok(())
}
