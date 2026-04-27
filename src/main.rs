mod achievements;
mod aggregate;
mod calc;
mod cli;
mod completions;
mod config;
mod config_file;
mod data;
mod dateparse;
mod display;
mod forecast;
mod interactive;
mod mcp;
mod models;
mod recommend;
mod runtime;
mod sort_filter;
mod update;
mod watch;

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Parser;
use colored::Colorize;

use cli::{Args, Period, Source};
use data::discovery::{ClaudeDataDir, GeminiDataDir, OpenCodeDataDir};
use runtime::RuntimeConfig;

fn main() -> Result<()> {
    let args = Args::parse();

    // Handle self-update early exit
    if args.increase_guilt {
        return update::self_update();
    }

    // Handle MCP server early exit (spins up tokio runtime only for this path)
    if args.mcp {
        return tokio::runtime::Runtime::new()?.block_on(mcp::run_server());
    }

    // Handle one-shot MCP registration with Claude Code
    if args.setup_mcp {
        return mcp::setup_mcp();
    }

    // Handle shell completions early exit
    if let Some(shell) = args.completions {
        clap_complete::generate(
            shell,
            &mut <Args as clap::CommandFactory>::command(),
            "ccguilt",
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    // Handle completion installation early exit
    if let Some(ref shell_arg) = args.setup_completions {
        return completions::setup_completions(shell_arg);
    }

    // Load config file and merge with CLI
    let user_config = config_file::load_config();
    let rc = RuntimeConfig::from_args_and_config(&args, &user_config);

    // Apply NO_COLOR
    if rc.no_color {
        colored::control::set_override(false);
    }

    let claude_home = match &args.claude_home {
        Some(p) => p.clone(),
        None => ClaudeDataDir::default_path()?,
    };

    let opencode_home = match &args.opencode_home {
        Some(p) => p.clone(),
        None => OpenCodeDataDir::default_path()?,
    };

    let gemini_home = match &args.gemini_home {
        Some(p) => p.clone(),
        None => GeminiDataDir::default_path()?,
    };

    let claude_available = claude_home.exists();
    let opencode_data_dir = OpenCodeDataDir::new(opencode_home);
    let opencode_available = opencode_data_dir.exists();
    let gemini_data_dir = GeminiDataDir::new(gemini_home);
    let gemini_available = gemini_data_dir.exists();

    if !claude_available && !opencode_available && !gemini_available {
        eprintln!("{} No data sources found. Checked:", "Error:".red().bold());
        eprintln!("  - Claude Code: {}", claude_home.display());
        eprintln!("  - OpenCode:    {}", opencode_data_dir.db_path().display());
        eprintln!("  - Gemini CLI:  {}", gemini_data_dir.tmp_dir().display());
        eprintln!("Have you used any AI coding tool? Lucky planet.");
        std::process::exit(1);
    }

    let include_claude =
        args.source != Source::OpenCode && args.source != Source::Gemini && claude_available;
    let include_opencode =
        args.source != Source::Claude && args.source != Source::Gemini && opencode_available;
    let include_gemini =
        args.source != Source::Claude && args.source != Source::OpenCode && gemini_available;

    let data_dir = ClaudeDataDir::new(claude_home.clone());

    let claude_session_count = if include_claude && !args.fast {
        if let Some(ref pat) = args.project_regex {
            match data_dir.jsonl_files_regex(pat) {
                Ok(f) => f.len(),
                Err(_) => 0,
            }
        } else {
            data_dir.jsonl_files(args.project.as_deref()).len()
        }
    } else if include_claude {
        1
    } else {
        0
    };

    let since = args
        .since
        .as_deref()
        .map(dateparse::parse_natural_date)
        .transpose()?;
    let until = args
        .until
        .as_deref()
        .map(dateparse::parse_natural_date)
        .transpose()?;

    // Validate flag combinations
    if args.fast && args.group_by == Some(cli::GroupBy::Project) {
        eprintln!(
            "{} --group-by project is not available in fast mode (no project info).",
            "Error:".red().bold()
        );
        std::process::exit(1);
    }

    let records = if include_claude && args.fast {
        fast_path(&args, &data_dir, &rc)?
    } else if include_claude || include_opencode || include_gemini {
        unified_scan(
            &args,
            &data_dir,
            &opencode_data_dir,
            &gemini_data_dir,
            include_claude,
            include_opencode,
            include_gemini,
            since,
            until,
            &rc,
        )?
    } else {
        Vec::new()
    };

    if records.is_empty() {
        if !rc.quiet {
            eprintln!(
                "No data found. Either you're a Digital Saint or your Claude data is elsewhere."
            );
        }
        return Ok(());
    }

    // Early-exit: session detail
    if let Some(ref session_id) = args.session {
        display::session_detail::render_session_detail(&records, session_id);
        return Ok(());
    }

    // Early-exit: achievements listing
    if args.achievements {
        let buckets = aggregate::aggregate_with(&records, args.period, rc.co2_kg_per_kwh, rc.pue);
        achievements::check_and_announce(&buckets);
        achievements::show_all();
        return Ok(());
    }

    // Early-exit: projects ranking
    if args.projects {
        let mut buckets = aggregate::aggregate_by_project(&records, rc.co2_kg_per_kwh, rc.pue);
        sort_filter::sort_buckets(&mut buckets, cli::SortField::Co2);
        if args.json {
            let json = display::json::render_json(&buckets)?;
            println!("{json}");
        } else {
            display::print_header();
            println!("  {}", "Project Ranking (by CO2 impact)".bold());
            println!();
            display::table::render_table(
                &buckets,
                &display::DisplayOptions {
                    no_guilt: rc.no_guilt,
                    no_color: rc.no_color,
                    by_model: false,
                    show_trends: false,
                    show_sparklines: false,
                    show_cumulative: false,
                    show_efficiency: false,
                    budget_co2_grams: None,
                    show_offset: false,
                },
            );
        }
        return Ok(());
    }

    // Early-exit: heatmap
    if args.heatmap {
        let buckets =
            aggregate::aggregate_with(&records, cli::Period::Daily, rc.co2_kg_per_kwh, rc.pue);
        display::heatmap::render_heatmap(&buckets, 12);
        return Ok(());
    }

    // Early-exit: diff mode
    if let Some(ref periods) = args.diff {
        let (since_a, until_a) = dateparse::parse_diff_period(&periods[0])?;
        let (since_b, until_b) = dateparse::parse_diff_period(&periods[1])?;
        let records_a: Vec<_> = records
            .iter()
            .filter(|r| r.timestamp >= since_a && r.timestamp < until_a)
            .cloned()
            .collect();
        let records_b: Vec<_> = records
            .iter()
            .filter(|r| r.timestamp >= since_b && r.timestamp < until_b)
            .cloned()
            .collect();
        let buckets_a =
            aggregate::aggregate_with(&records_a, Period::Total, rc.co2_kg_per_kwh, rc.pue);
        let buckets_b =
            aggregate::aggregate_with(&records_b, Period::Total, rc.co2_kg_per_kwh, rc.pue);
        display::diff::render_diff(&periods[0], &buckets_a, &periods[1], &buckets_b);
        return Ok(());
    }

    // Aggregate
    let mut buckets = match args.group_by {
        Some(cli::GroupBy::Project) => {
            aggregate::aggregate_by_project(&records, rc.co2_kg_per_kwh, rc.pue)
        }
        Some(cli::GroupBy::Model) => {
            aggregate::aggregate_by_model(&records, rc.co2_kg_per_kwh, rc.pue)
        }
        None => aggregate::aggregate_with(&records, args.period, rc.co2_kg_per_kwh, rc.pue),
    };

    // Sort, filter, truncate
    if let Some(min_co2) = args.min_co2 {
        sort_filter::filter_min_co2(&mut buckets, min_co2);
    }
    if let Some(min_cost) = args.min_cost {
        sort_filter::filter_min_cost(&mut buckets, min_cost);
    }
    if let Some(sort_field) = args.sort {
        sort_filter::sort_buckets(&mut buckets, sort_field);
    }
    if let Some(top_n) = args.top {
        buckets.truncate(top_n);
    }

    // Build display options
    let display_opts = display::DisplayOptions {
        no_guilt: rc.no_guilt,
        no_color: rc.no_color,
        by_model: args.by_model && args.group_by != Some(cli::GroupBy::Model),
        show_trends: rc.trends,
        show_sparklines: rc.sparklines,
        show_cumulative: args.cumulative,
        show_efficiency: args.efficiency,
        budget_co2_grams: rc.budget_co2_grams,
        show_offset: args.offset,
    };

    // Hook output (single-line for git hooks)
    if args.hook_output {
        let total_cost: f64 = buckets.iter().map(|b| b.cost.total_cost_usd).sum();
        let total_co2: f64 = buckets.iter().map(|b| b.impact.co2_grams).sum();
        let total_trees: f64 = buckets.iter().map(|b| b.impact.trees_destroyed).sum();
        let total_impact = crate::models::ImpactSummary {
            co2_grams: total_co2,
            ..Default::default()
        };
        let guilt = calc::impact::determine_guilt(&total_impact);
        println!(
            "Cost: ${:.2} | CO2: {} | Trees: {:.4} | Guilt: {}",
            total_cost,
            display::format::format_co2(total_co2),
            total_trees,
            guilt.title,
        );
        return Ok(());
    }

    // Model recommendations
    if args.recommend {
        recommend::print_recommendations(&buckets, rc.co2_kg_per_kwh, rc.pue);
        return Ok(());
    }

    // Dispatch output
    if args.interactive {
        return interactive::run_interactive(records, buckets, display_opts, rc);
    }

    if let Some(ref interval) = args.watch {
        let watch_ctx = watch::WatchContext {
            claude_dir: data_dir.clone(),
            opencode_dir: opencode_data_dir,
            gemini_dir: gemini_data_dir,
            include_claude,
            include_opencode,
            include_gemini,
        };
        return watch::run_watch(interval, &args, &watch_ctx, &rc, &display_opts);
    }

    if args.json {
        let json = display::json::render_json(&buckets)?;
        println!("{json}");
    } else if args.csv {
        display::csv::render_csv(&buckets, &mut std::io::stdout())?;
    } else if args.markdown {
        display::markdown::render_markdown(&buckets, &mut std::io::stdout())?;
    } else if let Some(ref path) = args.html {
        let mut file = std::fs::File::create(path)?;
        display::html::render_html(&buckets, &mut file)?;
        if !rc.quiet {
            eprintln!(
                "  {} HTML report written to {}",
                ">>".green().bold(),
                path.display()
            );
        }
    } else {
        display::print_header();
        display::print_multi_source_metadata(
            claude_session_count,
            &data_dir,
            &opencode_data_dir,
            &gemini_data_dir,
            include_claude,
            include_opencode,
            include_gemini,
            args.project.as_deref(),
            args.fast,
        );
        display::table::render_table(&buckets, &display_opts);
        if args.chart {
            display::chart::render_chart(&buckets);
        }
        display::print_summary_footer(&buckets, &display_opts, &rc);
    }

    // Check achievements (after display, before file output)
    if !rc.no_guilt {
        achievements::check_and_announce(&buckets);
    }

    // Write to file if --output specified
    if let Some(ref path) = args.output {
        let mut file = std::fs::File::create(path)?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "json" => {
                let json = display::json::render_json(&buckets)?;
                use std::io::Write;
                write!(file, "{json}")?;
            }
            "csv" => display::csv::render_csv(&buckets, &mut file)?,
            "md" | "markdown" => display::markdown::render_markdown(&buckets, &mut file)?,
            "html" | "htm" => display::html::render_html(&buckets, &mut file)?,
            _ => {
                let json = display::json::render_json(&buckets)?;
                use std::io::Write;
                write!(file, "{json}")?;
            }
        }
        if !rc.quiet {
            eprintln!(
                "  {} Output written to {}",
                ">>".green().bold(),
                path.display()
            );
        }
    }

    Ok(())
}

pub fn fast_path(
    args: &Args,
    data_dir: &ClaudeDataDir,
    rc: &RuntimeConfig,
) -> Result<Vec<models::TokenRecord>> {
    let cache_path = data_dir.stats_cache_path();
    if !cache_path.exists() {
        eprintln!(
            "{} stats-cache.json not found. Run Claude Code first, or use deep scan (remove --fast).",
            "Error:".red().bold()
        );
        std::process::exit(1);
    }

    if args.period == Period::Session {
        eprintln!(
            "{} Session-level breakdown not available in fast mode.",
            "Warning:".yellow().bold()
        );
        eprintln!("Use deep scan (remove --fast) for per-session data.");
        std::process::exit(1);
    }

    if !rc.quiet {
        eprintln!("  {} Reading stats-cache.json...", ">>".yellow().bold());
    }
    let fast_data = data::cache::parse_stats_cache(&cache_path)?;

    Ok(match args.period {
        Period::Total => aggregate::fast_path_total(&fast_data.model_usage),
        _ => aggregate::fast_path_daily(&fast_data.daily_tokens, &fast_data.model_usage),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn unified_scan(
    args: &Args,
    data_dir: &ClaudeDataDir,
    opencode_dir: &OpenCodeDataDir,
    gemini_dir: &GeminiDataDir,
    include_claude: bool,
    include_opencode: bool,
    include_gemini: bool,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    rc: &RuntimeConfig,
) -> Result<Vec<models::TokenRecord>> {
    if !args.no_db {
        let db_path = data_dir.base.join("ccguilt.db");

        let claude_files = if include_claude {
            if let Some(ref pat) = args.project_regex {
                data_dir.jsonl_files_regex(pat)?
            } else {
                data_dir.jsonl_files(args.project.as_deref())
            }
        } else {
            Vec::new()
        };

        let opencode_db = opencode_dir.db_path();
        let opencode_path = if include_opencode {
            Some(opencode_db.as_path())
        } else {
            None
        };

        let gemini_files = if include_gemini {
            gemini_dir.session_files(args.project.as_deref())
        } else {
            Vec::new()
        };

        let source_filter = match args.source {
            Source::Claude => Some("claude"),
            Source::OpenCode => Some("opencode"),
            Source::Gemini => Some("gemini"),
            Source::All => None,
        };

        match data::db::load_all_records(
            &db_path,
            &claude_files,
            opencode_path,
            &gemini_files,
            since,
            until,
            args.project.as_deref(),
            args.rebuild_db,
            rc.quiet,
            source_filter,
        ) {
            Ok(records) => {
                if !rc.quiet {
                    eprintln!(
                        "  {} Loaded {} records (SQLite-backed)",
                        ">>".green().bold(),
                        records.len()
                    );
                }

                if records.is_empty() && !rc.quiet {
                    eprintln!();
                    eprintln!("No data found in the specified range. The planet thanks you.");
                }

                return Ok(records);
            }
            Err(e) => {
                if rc.verbose {
                    eprintln!(
                        "  {} SQLite cache unavailable ({}), falling back to direct scan",
                        ">>".yellow().bold(),
                        e
                    );
                }
            }
        }
    }

    let mut records = Vec::new();

    if include_claude {
        let files = if let Some(ref pat) = args.project_regex {
            data_dir.jsonl_files_regex(pat)?
        } else {
            data_dir.jsonl_files(args.project.as_deref())
        };

        if !files.is_empty() {
            if !rc.quiet {
                eprintln!(
                    "  {} Deep scan: parsing {} Claude session files...",
                    ">>".green().bold(),
                    files.len()
                );
            }
            let claude_records =
                data::jsonl::parse_jsonl_files(&files, since, until, args.project.as_deref())?;
            records.extend(claude_records);
        }
    }

    if include_opencode {
        if !rc.quiet {
            eprintln!("  {} Reading OpenCode database...", ">>".green().bold());
        }
        match data::opencode::parse_opencode_db(
            &opencode_dir.db_path(),
            since,
            until,
            args.project.as_deref(),
        ) {
            Ok(oc_records) => records.extend(oc_records),
            Err(e) => {
                if rc.verbose {
                    eprintln!(
                        "  {} OpenCode database read failed: {}",
                        ">>".yellow().bold(),
                        e
                    );
                }
            }
        }
    }

    if include_gemini {
        let files = gemini_dir.session_files(args.project.as_deref());
        if !files.is_empty() {
            if !rc.quiet {
                eprintln!(
                    "  {} Reading {} Gemini CLI session files...",
                    ">>".green().bold(),
                    files.len()
                );
            }
            match data::gemini::parse_gemini_files(&files, since, until, args.project.as_deref()) {
                Ok(gemini_records) => records.extend(gemini_records),
                Err(e) => {
                    if rc.verbose {
                        eprintln!(
                            "  {} Gemini session parse failed: {}",
                            ">>".yellow().bold(),
                            e
                        );
                    }
                }
            }
        }
    }

    records.sort_by_key(|r| r.timestamp);

    if !rc.quiet {
        eprintln!(
            "  {} Found {} total token records",
            ">>".green().bold(),
            records.len()
        );
    }

    if records.is_empty() && !rc.quiet {
        eprintln!();
        eprintln!("No data found in the specified range. The planet thanks you.");
    }

    Ok(records)
}
