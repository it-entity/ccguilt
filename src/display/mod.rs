pub mod chart;
pub mod compare;
pub mod csv;
pub mod diff;
pub mod format;
pub mod guilt;
pub mod heatmap;
pub mod html;
pub mod json;
pub mod markdown;
pub mod mascot;
pub mod offset;
pub mod session_detail;
pub mod table;
pub mod token_breakdown;

use colored::Colorize;

use crate::data::discovery::{ClaudeDataDir, GeminiDataDir, OpenCodeDataDir};
use crate::forecast;
use crate::models::UsageBucket;
use crate::runtime::RuntimeConfig;

pub struct DisplayOptions {
    pub no_guilt: bool,
    pub no_color: bool,
    pub by_model: bool,
    pub show_trends: bool,
    pub show_sparklines: bool,
    pub show_cumulative: bool,
    pub show_efficiency: bool,
    pub budget_co2_grams: Option<f64>,
    pub show_offset: bool,
}

pub fn print_header() {
    let border = "=".repeat(66);
    println!();
    println!("{}", border.bright_red().bold());
    println!("{}", "  CLAUDE CODE GUILT TRIP".bright_red().bold());
    println!(
        "{}",
        "  An environmental impact report nobody asked for".red()
    );
    println!("{}", border.bright_red().bold());
    println!();
}

#[allow(dead_code)]
pub fn print_metadata(
    data_dir: &ClaudeDataDir,
    file_count: usize,
    project_filter: Option<&str>,
    fast: bool,
) {
    let source = if fast {
        "Fast scan (stats-cache.json)".yellow().to_string()
    } else {
        format!(
            "Deep scan of {} session files across {} projects",
            file_count,
            data_dir.project_count()
        )
        .green()
        .to_string()
    };

    println!("  {}: {}", "Data".bold(), source);
    if let Some(filter) = project_filter {
        println!("  {}: {}", "Project filter".bold(), filter);
    }
    println!();
}

#[allow(clippy::too_many_arguments)]
pub fn print_multi_source_metadata(
    data_dir: &ClaudeDataDir,
    _opencode_dir: &OpenCodeDataDir,
    _gemini_dir: &GeminiDataDir,
    include_claude: bool,
    include_opencode: bool,
    include_gemini: bool,
    project_filter: Option<&str>,
    fast: bool,
) {
    let mut parts = Vec::new();

    if include_claude {
        if fast {
            parts.push("Fast scan (stats-cache.json)".yellow().to_string());
        } else {
            let file_count = data_dir.jsonl_files(project_filter).len();
            parts.push(
                format!(
                    "Claude Code ({} sessions, {} projects)",
                    file_count,
                    data_dir.project_count()
                )
                .green()
                .to_string(),
            );
        }
    }

    if include_opencode {
        parts.push("OpenCode".cyan().to_string());
    }

    if include_gemini {
        parts.push("Gemini CLI".magenta().to_string());
    }

    let source = parts.join(" + ");
    println!("  {}: {}", "Sources".bold(), source);
    if let Some(filter) = project_filter {
        println!("  {}: {}", "Project filter".bold(), filter);
    }
    println!();
}

pub fn print_summary_footer(buckets: &[UsageBucket], opts: &DisplayOptions, rc: &RuntimeConfig) {
    if opts.no_guilt || buckets.is_empty() {
        return;
    }

    // Compute totals
    let total_co2: f64 = buckets.iter().map(|b| b.impact.co2_grams).sum();
    let total_water: f64 = buckets.iter().map(|b| b.impact.water_ml).sum();
    let total_energy: f64 = buckets.iter().map(|b| b.impact.energy_wh).sum();
    let total_trees: f64 = buckets.iter().map(|b| b.impact.trees_destroyed).sum();

    let total_impact = crate::models::ImpactSummary {
        energy_wh: total_energy,
        co2_grams: total_co2,
        water_ml: total_water,
        trees_destroyed: total_trees,
        trees_dehydrated: buckets.iter().map(|b| b.impact.trees_dehydrated).sum(),
        netflix_hours_equiv: buckets.iter().map(|b| b.impact.netflix_hours_equiv).sum(),
    };

    // ASCII mascot
    let overall_guilt = crate::calc::impact::determine_guilt(&total_impact);
    mascot::print_mascot(overall_guilt.level);

    // Tree progress bar
    println!();
    println!("{}", guilt::tree_progress_bar(total_trees));

    // Token type breakdown
    token_breakdown::render_token_breakdown(buckets);

    // Sparklines
    if opts.show_sparklines && buckets.len() > 1 {
        let co2_values: Vec<f64> = buckets.iter().map(|b| b.impact.co2_grams).collect();
        let cost_values: Vec<f64> = buckets.iter().map(|b| b.cost.total_cost_usd).collect();
        println!();
        println!(
            "  {} {}",
            "CO2 trend:".dimmed(),
            format::sparkline(&co2_values)
        );
        println!(
            "  {} {}",
            "Cost trend:".dimmed(),
            format::sparkline(&cost_values)
        );
    }

    // Budget bar
    if let Some(budget) = opts.budget_co2_grams {
        println!();
        let pct = (total_co2 / budget * 100.0).min(200.0);
        let bar_width = 30;
        let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
        let filled = filled.min(bar_width);
        let empty = bar_width - filled;
        let bar_color = if pct > 90.0 {
            "red"
        } else if pct > 70.0 {
            "yellow"
        } else {
            "green"
        };
        let bar = format!("[{}{}]", "#".repeat(filled), ".".repeat(empty));
        let bar_colored = match bar_color {
            "red" => bar.red().bold(),
            "yellow" => bar.yellow().bold(),
            _ => bar.green().bold(),
        };
        let remaining = (budget - total_co2).max(0.0);
        println!(
            "  {} {} {:.1}% of {} used",
            "Carbon Budget:".bold(),
            bar_colored,
            pct,
            format::format_co2(budget),
        );
        if remaining > 0.0 {
            println!(
                "  {} {}",
                "Remaining:".dimmed(),
                format::format_co2(remaining),
            );
        } else {
            println!("  {} Budget exceeded!", "WARNING:".red().bold());
        }
    }

    // Percentile stats
    if buckets.len() >= 5 {
        let mut co2_vals: Vec<f64> = buckets.iter().map(|b| b.impact.co2_grams).collect();
        co2_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = percentile(&co2_vals, 50.0);
        let p95 = percentile(&co2_vals, 95.0);
        let p99 = percentile(&co2_vals, 99.0);
        println!();
        println!(
            "  {} {} {} {} {} {} {}",
            "Median CO2/period:".dimmed(),
            format::format_co2(median),
            "|".dimmed(),
            "P95:".dimmed(),
            format::format_co2(p95),
            "|".dimmed(),
            format!("P99: {}", format::format_co2(p99)).dimmed(),
        );
    }

    // Forecast
    if buckets.len() >= 3 {
        if let Some(fc) = forecast::project_annual(buckets) {
            println!();
            let trend_str = match fc.trend {
                forecast::TrendDirection::Accelerating => "Accelerating".red().to_string(),
                forecast::TrendDirection::Decelerating => "Decelerating".green().to_string(),
                forecast::TrendDirection::Stable => "Stable".yellow().to_string(),
            };
            println!(
                "  {} CO2: {} | Cost: {} | Trees: {:.1} | Trend: {}",
                "Annual projection:".bold(),
                format::format_co2(fc.annual_co2_grams),
                format::format_cost(fc.annual_cost_usd),
                fc.annual_trees,
                trend_str,
            );
        }
    }

    // Carbon offset
    if opts.show_offset {
        offset::render_offset(total_co2);
    }

    // Satirical comparisons
    println!();
    let comparisons = guilt::generate_comparisons(&total_impact);
    for (i, comp) in comparisons.iter().enumerate() {
        if i >= 3 {
            break;
        }
        println!("  {}", comp);
    }

    // Random guilt quote
    println!();
    let separator = "-".repeat(66);
    println!("{}", separator.dimmed());
    println!();
    let quote = guilt::random_quote();
    println!("  {}", quote.italic().dimmed());

    // Nihilistic / absurdist remark based on overall guilt level
    let overall_guilt = crate::calc::impact::determine_guilt(&total_impact);
    let remark = guilt::random_remark(overall_guilt.level);
    println!();
    println!("  {}", remark.italic().bright_black());

    // Region info
    if let Some(ref region) = rc.region {
        println!();
        println!(
            "  {} {} (CO2: {} kg/kWh, PUE: {})",
            "Region:".dimmed(),
            region,
            rc.co2_kg_per_kwh,
            rc.pue,
        );
    }

    // Sources
    println!();
    println!(
        "  {}",
        "Sources: Jegham et al. 2025, EPA eGRID 2024, Li et al. 2023,".dimmed()
    );
    println!(
        "  {}",
        "  Luccioni et al. 2023, USDA Forestry, IEA 2024".dimmed()
    );
    println!();
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
