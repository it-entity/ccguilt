use crossterm::{cursor, execute, terminal};

use crate::cli::Args;
use crate::data::discovery::ClaudeDataDir;
use crate::display::DisplayOptions;
use crate::runtime::RuntimeConfig;

pub fn run_watch(
    interval_secs: u64,
    args: &Args,
    data_dir: &ClaudeDataDir,
    rc: &RuntimeConfig,
    display_opts: &DisplayOptions,
) -> anyhow::Result<()> {
    loop {
        execute!(
            std::io::stdout(),
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0),
        )?;

        // Parse records
        let records = if args.fast {
            let cache_path = data_dir.stats_cache_path();
            let fast_data = crate::data::cache::parse_stats_cache(&cache_path)?;
            match args.period {
                crate::cli::Period::Total => {
                    crate::aggregate::fast_path_total(&fast_data.model_usage)
                }
                _ => crate::aggregate::fast_path_daily(
                    &fast_data.daily_tokens,
                    &fast_data.model_usage,
                ),
            }
        } else {
            let files = data_dir.jsonl_files(args.project.as_deref());
            let since = args
                .since
                .as_deref()
                .map(crate::dateparse::parse_natural_date)
                .transpose()?;
            let until = args
                .until
                .as_deref()
                .map(crate::dateparse::parse_natural_date_end)
                .transpose()?;
            crate::data::jsonl::parse_jsonl_files(&files, since, until, args.project.as_deref())?
        };

        if records.is_empty() {
            println!("No data found. Waiting...");
        } else {
            let mut buckets =
                crate::aggregate::aggregate_with(records, args.period, rc.co2_kg_per_kwh, rc.pue);

            if let Some(field) = args.sort {
                crate::sort_filter::sort_buckets(&mut buckets, field);
            }
            if let Some(top_n) = args.top {
                buckets.truncate(top_n);
            }

            let file_count = if args.fast {
                1
            } else {
                data_dir.jsonl_files(args.project.as_deref()).len()
            };

            crate::display::print_header();
            crate::display::print_metadata(
                data_dir,
                file_count,
                args.project.as_deref(),
                args.fast,
            );
            crate::display::table::render_table(&buckets, display_opts);
        }

        println!(
            "\n  Refreshing every {}s... (Ctrl+C to stop)",
            interval_secs
        );

        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
    }
}
