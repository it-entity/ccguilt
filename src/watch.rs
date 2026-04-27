use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Local;
use colored::Colorize;
use crossterm::event::{self, Event, KeyCode};
use crossterm::{cursor, execute, terminal};

use crate::cli::{Args, WatchInterval};
use crate::data::discovery::{ClaudeDataDir, GeminiDataDir, OpenCodeDataDir};
use crate::display::format::*;
use crate::display::DisplayOptions;
use crate::models::UsageBucket;
use crate::runtime::RuntimeConfig;

pub struct WatchContext {
    pub claude_dir: ClaudeDataDir,
    pub opencode_dir: OpenCodeDataDir,
    pub gemini_dir: GeminiDataDir,
    pub include_claude: bool,
    pub include_opencode: bool,
    pub include_gemini: bool,
}

struct WatchState {
    refresh_count: u64,
    prev_tokens: u64,
    prev_cost_usd: f64,
    prev_co2_grams: f64,
    prev_water_ml: f64,
    prev_trees: f64,
    fingerprint: Option<(usize, u64, i64)>,
}

impl WatchState {
    fn new() -> Self {
        Self {
            refresh_count: 0,
            prev_tokens: 0,
            prev_cost_usd: 0.0,
            prev_co2_grams: 0.0,
            prev_water_ml: 0.0,
            prev_trees: 0.0,
            fingerprint: None,
        }
    }

    fn update_from(&mut self, buckets: &[UsageBucket]) {
        let tokens: u64 = buckets.iter().map(|b| b.tokens.total_tokens()).sum();
        let cost: f64 = buckets.iter().map(|b| b.cost.total_cost_usd).sum();
        let co2: f64 = buckets.iter().map(|b| b.impact.co2_grams).sum();
        let water: f64 = buckets.iter().map(|b| b.impact.water_ml).sum();
        let trees: f64 = buckets.iter().map(|b| b.impact.trees_destroyed).sum();

        self.prev_tokens = tokens;
        self.prev_cost_usd = cost;
        self.prev_co2_grams = co2;
        self.prev_water_ml = water;
        self.prev_trees = trees;
        self.refresh_count += 1;
    }
}

fn file_fingerprint(files: &[PathBuf]) -> (usize, u64, i64) {
    use std::path::Path;
    fn meta(p: &Path) -> Option<(u64, i64)> {
        let m = p.metadata().ok()?;
        let sz = m.len();
        let mt = m
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        Some((sz, mt))
    }
    let count = files.len();
    let (total_size, max_mtime) = files
        .iter()
        .filter_map(|p| meta(p))
        .fold((0u64, 0i64), |(sz, mt), (s, m)| (sz + s, mt.max(m)));
    (count, total_size, max_mtime)
}

fn data_fingerprint(ctx: &WatchContext, project: Option<&str>) -> (usize, u64, i64) {
    let mut all_files = Vec::new();

    if ctx.include_claude {
        all_files.extend(ctx.claude_dir.jsonl_files(project));
    }

    if ctx.include_opencode {
        let db = ctx.opencode_dir.db_path();
        if db.exists() {
            all_files.push(db);
        }
    }

    if ctx.include_gemini {
        all_files.extend(ctx.gemini_dir.session_files(project));
    }

    file_fingerprint(&all_files)
}

fn load_records(
    args: &Args,
    ctx: &WatchContext,
    rc: &RuntimeConfig,
) -> anyhow::Result<Vec<crate::models::TokenRecord>> {
    let since = args
        .since
        .as_deref()
        .map(crate::dateparse::parse_natural_date)
        .transpose()?;
    let until = args
        .until
        .as_deref()
        .map(crate::dateparse::parse_natural_date)
        .transpose()?;

    if ctx.include_claude && args.fast {
        return crate::fast_path(args, &ctx.claude_dir, rc);
    }

    if ctx.include_claude || ctx.include_opencode || ctx.include_gemini {
        return crate::unified_scan(
            args,
            &ctx.claude_dir,
            &ctx.opencode_dir,
            &ctx.gemini_dir,
            ctx.include_claude,
            ctx.include_opencode,
            ctx.include_gemini,
            since,
            until,
            rc,
        );
    }

    Ok(Vec::new())
}

fn render_delta(state: &WatchState, buckets: &[UsageBucket]) {
    if state.refresh_count == 0 {
        println!("{}", "  (baseline established)".dimmed());
        return;
    }

    let cur_tokens: u64 = buckets.iter().map(|b| b.tokens.total_tokens()).sum();
    let cur_cost: f64 = buckets.iter().map(|b| b.cost.total_cost_usd).sum();
    let cur_co2: f64 = buckets.iter().map(|b| b.impact.co2_grams).sum();
    let cur_water: f64 = buckets.iter().map(|b| b.impact.water_ml).sum();

    let d_tokens = cur_tokens as i64 - state.prev_tokens as i64;
    let d_cost = cur_cost - state.prev_cost_usd;
    let d_co2 = cur_co2 - state.prev_co2_grams;
    let d_water = cur_water - state.prev_water_ml;

    if d_tokens == 0 && d_cost.abs() < 0.001 {
        println!("{}", "  No new usage since last check.".dimmed());
        return;
    }

    let mut parts = Vec::new();
    if d_tokens != 0 {
        parts.push(format!(
            "{}{} tokens",
            if d_tokens > 0 { "+" } else { "" },
            d_tokens
        ));
    }
    if d_cost.abs() >= 0.001 {
        parts.push(format!(
            "{}{}",
            if d_cost > 0.0 { "+" } else { "" },
            format_cost(d_cost)
        ));
    }
    if d_co2.abs() >= 0.01 {
        parts.push(format!(
            "{}{} CO2",
            if d_co2 > 0.0 { "+" } else { "" },
            format_co2(d_co2)
        ));
    }
    if d_water.abs() >= 1.0 {
        parts.push(format!(
            "{}{} water",
            if d_water > 0.0 { "+" } else { "" },
            format_water(d_water)
        ));
    }

    println!("  {} {}", "Since last check:".bold(), parts.join(" | "));
}

#[allow(clippy::too_many_arguments)]
pub fn run_watch(
    interval: &WatchInterval,
    args: &Args,
    ctx: &WatchContext,
    rc: &RuntimeConfig,
    display_opts: &DisplayOptions,
) -> anyhow::Result<()> {
    let timeout = Duration::from_secs(interval.as_secs());
    let mut state = WatchState::new();

    execute!(io::stdout(), terminal::Clear(terminal::ClearType::All))?;

    loop {
        let fp = if args.fast {
            None
        } else {
            Some(data_fingerprint(ctx, args.project.as_deref()))
        };

        let changed = match (&fp, &state.fingerprint) {
            (Some(cur), Some(prev)) => cur != prev,
            _ => true,
        };

        if changed {
            state.fingerprint = fp;

            let records = load_records(args, ctx, rc)?;

            execute!(
                io::stdout(),
                terminal::Clear(terminal::ClearType::All),
                cursor::MoveTo(0, 0)
            )?;

            let now = Local::now().format("%Y-%m-%d %H:%M:%S");
            println!(
                "  [{}] Refresh #{} — next in {}",
                now,
                state.refresh_count + 1,
                interval.display(),
            );

            if records.is_empty() {
                println!("\n  No data found. Waiting...");
            } else {
                let mut buckets = crate::aggregate::aggregate_with(
                    records,
                    args.period,
                    rc.co2_kg_per_kwh,
                    rc.pue,
                );

                if let Some(field) = args.sort {
                    crate::sort_filter::sort_buckets(&mut buckets, field);
                }
                if let Some(top_n) = args.top {
                    buckets.truncate(top_n);
                }

                render_delta(&state, &buckets);
                state.update_from(&buckets);

                crate::display::print_header();
                crate::display::print_multi_source_metadata(
                    &ctx.claude_dir,
                    &ctx.opencode_dir,
                    &ctx.gemini_dir,
                    ctx.include_claude,
                    ctx.include_opencode,
                    ctx.include_gemini,
                    args.project.as_deref(),
                    args.fast,
                );
                crate::display::table::render_table(&buckets, display_opts);
            }
        } else {
            execute!(
                io::stdout(),
                terminal::Clear(terminal::ClearType::All),
                cursor::MoveTo(0, 0)
            )?;
            let now = Local::now().format("%Y-%m-%d %H:%M:%S");
            println!(
                "  [{}] Refresh #{} — no changes detected",
                now,
                state.refresh_count + 1,
            );
            render_delta(&state, &[]);
        }

        println!();
        println!(
            "  {} (Ctrl+C or q to stop)",
            format!("Refreshing every {}...", interval.display()).dimmed()
        );

        io::stdout().flush()?;

        let start = Instant::now();
        loop {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }
            match event::poll(remaining) {
                Ok(true) => {
                    if let Ok(Event::Key(key)) = event::read() {
                        if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                            println!("\n  Watch stopped.");
                            print_final_summary(&state, interval);
                            return Ok(());
                        }
                    }
                }
                Ok(false) => break,
                Err(_) => break,
            }
        }
    }
}

fn print_final_summary(state: &WatchState, interval: &WatchInterval) {
    if state.refresh_count == 0 {
        return;
    }
    println!();
    println!("{}", "  ── Final Summary ──".bold());
    println!("  Refreshes: {}", state.refresh_count);
    println!("  Interval:  {}", interval.display());
    if state.prev_tokens > 0 {
        println!("  Tokens:    {}", format_tokens(state.prev_tokens));
        println!("  Cost:      {}", format_cost(state.prev_cost_usd));
        println!("  CO2:       {}", format_co2(state.prev_co2_grams));
        println!("  Water:     {}", format_water(state.prev_water_ml));
        println!("  Trees:     {:.4} destroyed", state.prev_trees);
    }
}
