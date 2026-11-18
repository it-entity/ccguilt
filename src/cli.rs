use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "ccguilt",
    about = "Claude Code Guilt Trip — see what your AI habit is doing to the planet",
    long_about = "A satirical environmental impact tracker for Claude Code usage.\n\
                  Reads your local Claude Code data and computes energy, CO2, water,\n\
                  and tree destruction metrics. You monster.\n\n\
                  Sources: Jegham et al. 2025, Luccioni et al. 2023, Li et al. 2023,\n\
                  EPA eGRID 2024, USDA Forestry",
    version
)]
pub struct Args {
    /// Report grouping period
    #[arg(value_enum, default_value = "daily")]
    pub period: Period,

    // ── Date & Filter ──
    /// Start date filter (YYYY-MM-DD, 7d, 2w, last-week, yesterday, monday, etc.)
    #[arg(long)]
    pub since: Option<String>,

    /// End date filter (same formats as --since)
    #[arg(long)]
    pub until: Option<String>,

    /// Filter by project path (substring match)
    #[arg(long, group = "project_filter")]
    pub project: Option<String>,

    /// Filter by project path (regex pattern)
    #[arg(long, group = "project_filter")]
    pub project_regex: Option<String>,

    // ── Data Mode ──
    /// Use stats-cache.json for faster but less accurate results
    #[arg(long)]
    pub fast: bool,

    /// Custom Claude data directory (default: ~/.claude)
    #[arg(long, env = "CLAUDE_HOME")]
    pub claude_home: Option<PathBuf>,

    /// Custom OpenCode data directory (default: ~/.local/share/opencode)
    #[arg(long, env = "OPENCODE_HOME")]
    pub opencode_home: Option<PathBuf>,

    /// Custom Gemini CLI data directory (default: ~/.gemini)
    #[arg(long, env = "GEMINI_HOME")]
    pub gemini_home: Option<PathBuf>,

    /// Data source(s) to scan (default: all)
    #[arg(long, value_enum, default_value = "all")]
    pub source: Source,

    // ── Analysis ──
    /// Show per-model token breakdown within each period
    #[arg(long)]
    pub by_model: bool,

    /// Sort periods by metric (default: chronological)
    #[arg(long, value_enum)]
    pub sort: Option<SortField>,

    /// Show only the top N periods
    #[arg(long)]
    pub top: Option<usize>,

    /// Group by dimension instead of time period
    #[arg(long, value_enum)]
    pub group_by: Option<GroupBy>,

    /// Show efficiency metrics ($/Mtok, gCO2/Mtok)
    #[arg(long)]
    pub efficiency: bool,

    /// Show cumulative running totals
    #[arg(long)]
    pub cumulative: bool,

    /// Hide periods below this CO2 threshold (grams)
    #[arg(long)]
    pub min_co2: Option<f64>,

    /// Hide periods below this cost threshold (USD)
    #[arg(long)]
    pub min_cost: Option<f64>,

    /// Carbon budget (e.g., "50kg", "5000g", "1t") — shows progress toward limit
    #[arg(long)]
    pub budget: Option<String>,

    /// Compare projects side-by-side (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub compare: Option<Vec<String>>,

    // ── Output Format ──
    /// Output as JSON instead of a table
    #[arg(long, group = "output_format")]
    pub json: bool,

    /// Output as CSV
    #[arg(long, group = "output_format")]
    pub csv: bool,

    /// Output as Markdown
    #[arg(long, group = "output_format")]
    pub markdown: bool,

    /// Output as standalone HTML report to file
    #[arg(long)]
    pub html: Option<PathBuf>,

    /// Write output to file (format auto-detected from extension: .csv, .json, .html, .md)
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Show a bar chart of CO2/water emissions per period
    #[arg(long)]
    pub chart: bool,

    /// Show sparklines in table/footer
    #[arg(long)]
    pub sparkline: bool,

    // ── Display Control ──
    /// Hide satirical commentary (coward mode)
    #[arg(long)]
    pub no_guilt: bool,

    /// Disable colored output (also respects NO_COLOR env var)
    #[arg(long)]
    pub no_color: bool,

    /// Quiet mode: suppress progress messages on stderr
    #[arg(short, long, group = "verbosity")]
    pub quiet: bool,

    /// Verbose mode: show per-file parsing details
    #[arg(short, long, group = "verbosity")]
    pub verbose: bool,

    // ── Modes ──
    /// Launch interactive TUI mode
    #[arg(short, long)]
    pub interactive: bool,

    /// Watch mode: continuous refresh (interval: 5m, 10m, or 15m, default: 5m)
    #[arg(long, default_missing_value = "5m", num_args = 0..=1, value_name = "INTERVAL")]
    pub watch: Option<WatchInterval>,

    // ── Utility ──
    /// Generate shell completions (bash, zsh, fish, elvish, powershell)
    #[arg(long)]
    pub completions: Option<clap_complete::Shell>,

    /// Install shell completions to the appropriate system location
    #[arg(long, default_missing_value = "auto", num_args = 0..=1)]
    pub setup_completions: Option<String>,

    /// Check for updates and self-update if available
    #[arg(long)]
    pub increase_guilt: bool,

    /// Run as an MCP (Model Context Protocol) server over stdio.
    /// Register with: claude mcp add ccguilt -- ccguilt --mcp
    #[arg(long)]
    pub mcp: bool,

    /// Auto-register ccguilt as an MCP server in Claude Code (one-shot setup)
    #[arg(long)]
    pub setup_mcp: bool,

    // ── New Features ──
    /// Compare two time periods (e.g., --diff last-week this-week)
    #[arg(long, num_args = 2)]
    pub diff: Option<Vec<String>>,

    /// Show model cost/CO2 optimization recommendations
    #[arg(long)]
    pub recommend: bool,

    /// Show the Hall of Shame (all achievements)
    #[arg(long)]
    pub achievements: bool,

    /// Show carbon offset options
    #[arg(long)]
    pub offset: bool,

    /// List all projects ranked by environmental impact
    #[arg(long)]
    pub projects: bool,

    /// Show detailed timeline for a session (substring match on ID)
    #[arg(long)]
    pub session: Option<String>,

    /// Output a single compact line (for git hooks)
    #[arg(long, group = "output_format")]
    pub hook_output: bool,

    /// Show a calendar heatmap of daily CO2 emissions
    #[arg(long)]
    pub heatmap: bool,

    /// Skip the SQLite cache, parse JSONL files directly
    #[arg(long)]
    pub no_db: bool,

    /// Rebuild the SQLite database from scratch
    #[arg(long)]
    pub rebuild_db: bool,
}

#[derive(ValueEnum, Clone, Debug, Copy, PartialEq, Eq)]
pub enum Period {
    Daily,
    Weekly,
    Monthly,
    Session,
    Total,
}

#[derive(ValueEnum, Clone, Debug, Copy, PartialEq, Eq)]
pub enum SortField {
    Co2,
    Cost,
    Tokens,
    Energy,
    Water,
}

#[derive(ValueEnum, Clone, Debug, Copy, PartialEq, Eq)]
pub enum GroupBy {
    Project,
    Model,
}

#[derive(ValueEnum, Clone, Debug, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    All,
    Claude,
    OpenCode,
    Gemini,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum WatchInterval {
    Min5,
    Min10,
    Min15,
}

impl WatchInterval {
    pub fn as_secs(&self) -> u64 {
        match self {
            WatchInterval::Min5 => 300,
            WatchInterval::Min10 => 600,
            WatchInterval::Min15 => 900,
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            WatchInterval::Min5 => "5m",
            WatchInterval::Min10 => "10m",
            WatchInterval::Min15 => "15m",
        }
    }
}

impl FromStr for WatchInterval {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "5m" => Ok(WatchInterval::Min5),
            "10m" => Ok(WatchInterval::Min10),
            "15m" => Ok(WatchInterval::Min15),
            other => Err(format!(
                "invalid value '{other}' for --watch: allowed values are 5m, 10m, 15m"
            )),
        }
    }
}
