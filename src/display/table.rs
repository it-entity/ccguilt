use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::*;

use crate::display::format::*;
use crate::display::DisplayOptions;
use crate::models::{GuiltLevel, UsageBucket};

pub fn render_table(buckets: &[UsageBucket], opts: &DisplayOptions) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Build headers
    let mut headers = vec![
        Cell::new("Period").set_alignment(CellAlignment::Left),
        Cell::new("Tokens").set_alignment(CellAlignment::Right),
        Cell::new("Cost").set_alignment(CellAlignment::Right),
        Cell::new("Energy").set_alignment(CellAlignment::Right),
        Cell::new("CO2").set_alignment(CellAlignment::Right),
        Cell::new("Water").set_alignment(CellAlignment::Right),
        Cell::new("Trees").set_alignment(CellAlignment::Right),
    ];
    if opts.show_efficiency {
        headers.push(Cell::new("$/Mtok").set_alignment(CellAlignment::Right));
        headers.push(Cell::new("gCO2/Mtok").set_alignment(CellAlignment::Right));
    }
    if opts.show_cumulative {
        headers.push(Cell::new("Cum. CO2").set_alignment(CellAlignment::Right));
    }
    if !opts.no_guilt {
        headers.push(Cell::new("Guilt").set_alignment(CellAlignment::Center));
    }
    table.set_header(headers);

    // Compute trend arrows if enabled
    let trends: Vec<&str> = if opts.show_trends && buckets.len() > 1 {
        let mut t = vec![""];
        for w in buckets.windows(2) {
            let prev = w[0].impact.co2_grams;
            let curr = w[1].impact.co2_grams;
            if prev == 0.0 {
                t.push("");
            } else {
                let change = (curr - prev) / prev;
                if change > 0.10 {
                    t.push(" \u{25B2}"); // ▲
                } else if change < -0.10 {
                    t.push(" \u{25BC}"); // ▼
                } else {
                    t.push(" =");
                }
            }
        }
        t
    } else {
        vec![""; buckets.len()]
    };

    let mut cumulative_co2 = 0.0;

    for (i, bucket) in buckets.iter().enumerate() {
        cumulative_co2 += bucket.impact.co2_grams;

        let label = format!("{}{}", bucket.label, trends.get(i).unwrap_or(&""));

        let mut row = vec![
            Cell::new(&label),
            Cell::new(format_tokens(bucket.tokens.total_tokens())),
            Cell::new(format_cost(bucket.cost.total_cost_usd)),
            Cell::new(format_energy(bucket.impact.energy_wh)),
            Cell::new(format_co2(bucket.impact.co2_grams)),
            Cell::new(format_water(bucket.impact.water_ml)),
            Cell::new(format_trees(bucket.impact.trees_destroyed)),
        ];
        if opts.show_efficiency {
            let mtok = bucket.tokens.total_tokens() as f64 / 1_000_000.0;
            if mtok > 0.0 {
                row.push(Cell::new(format!(
                    "{:.2}",
                    bucket.cost.total_cost_usd / mtok
                )));
                row.push(Cell::new(format!("{:.1}", bucket.impact.co2_grams / mtok)));
            } else {
                row.push(Cell::new("-"));
                row.push(Cell::new("-"));
            }
        }
        if opts.show_cumulative {
            row.push(Cell::new(format_co2(cumulative_co2)));
        }
        if !opts.no_guilt && !opts.no_color {
            row.push(Cell::new(&bucket.guilt.title).fg(guilt_table_color(bucket.guilt.level)));
        } else if !opts.no_guilt {
            row.push(Cell::new(&bucket.guilt.title));
        }
        table.add_row(row);

        // Per-model sub-rows
        if opts.by_model {
            for (tier, model_tokens) in &bucket.tokens.by_model {
                let model_total = model_tokens.input_tokens
                    + model_tokens.output_tokens
                    + model_tokens.cache_creation_tokens
                    + model_tokens.cache_read_tokens;
                if model_total == 0 {
                    continue;
                }

                let model_cost = crate::calc::cost::calculate_model_cost(model_tokens, *tier);
                let model_summary = crate::models::TokenSummary {
                    input_tokens: model_tokens.input_tokens,
                    output_tokens: model_tokens.output_tokens,
                    cache_creation_tokens: model_tokens.cache_creation_tokens,
                    cache_read_tokens: model_tokens.cache_read_tokens,
                    by_model: {
                        let mut m = std::collections::HashMap::new();
                        m.insert(*tier, model_tokens.clone());
                        m
                    },
                };
                let model_impact = crate::calc::impact::calculate_impact(&model_summary);

                let mut sub_row = vec![
                    Cell::new(format!("  {}", tier.display_name()))
                        .fg(comfy_table::Color::DarkGrey),
                    Cell::new(format_tokens(model_total)).fg(comfy_table::Color::DarkGrey),
                    Cell::new(format_cost(model_cost.total_cost_usd))
                        .fg(comfy_table::Color::DarkGrey),
                    Cell::new(format_energy(model_impact.energy_wh))
                        .fg(comfy_table::Color::DarkGrey),
                    Cell::new(format_co2(model_impact.co2_grams)).fg(comfy_table::Color::DarkGrey),
                    Cell::new(format_water(model_impact.water_ml)).fg(comfy_table::Color::DarkGrey),
                    Cell::new(format_trees(model_impact.trees_destroyed))
                        .fg(comfy_table::Color::DarkGrey),
                ];
                if opts.show_efficiency {
                    let mtok = model_total as f64 / 1_000_000.0;
                    if mtok > 0.0 {
                        sub_row.push(
                            Cell::new(format!("{:.2}", model_cost.total_cost_usd / mtok))
                                .fg(comfy_table::Color::DarkGrey),
                        );
                        sub_row.push(
                            Cell::new(format!("{:.1}", model_impact.co2_grams / mtok))
                                .fg(comfy_table::Color::DarkGrey),
                        );
                    } else {
                        sub_row.push(Cell::new("-").fg(comfy_table::Color::DarkGrey));
                        sub_row.push(Cell::new("-").fg(comfy_table::Color::DarkGrey));
                    }
                }
                if opts.show_cumulative {
                    sub_row.push(Cell::new("").fg(comfy_table::Color::DarkGrey));
                }
                if !opts.no_guilt {
                    sub_row.push(Cell::new("").fg(comfy_table::Color::DarkGrey));
                }
                table.add_row(sub_row);
            }
        }
    }

    // Totals row if multiple buckets
    if buckets.len() > 1 {
        let total_tokens: u64 = buckets.iter().map(|b| b.tokens.total_tokens()).sum();
        let total_cost: f64 = buckets.iter().map(|b| b.cost.total_cost_usd).sum();
        let total_energy: f64 = buckets.iter().map(|b| b.impact.energy_wh).sum();
        let total_co2: f64 = buckets.iter().map(|b| b.impact.co2_grams).sum();
        let total_water: f64 = buckets.iter().map(|b| b.impact.water_ml).sum();
        let total_trees: f64 = buckets.iter().map(|b| b.impact.trees_destroyed).sum();

        let worst_guilt = buckets
            .iter()
            .map(|b| b.guilt.level)
            .max()
            .unwrap_or(GuiltLevel::DigitalSaint);
        let worst_title = buckets
            .iter()
            .find(|b| b.guilt.level == worst_guilt)
            .map(|b| b.guilt.title.clone())
            .unwrap_or_default();

        let mut total_row = vec![
            Cell::new("TOTAL")
                .add_attribute(Attribute::Bold)
                .fg(comfy_table::Color::White),
            Cell::new(format_tokens(total_tokens)).fg(comfy_table::Color::White),
            Cell::new(format_cost(total_cost)).fg(comfy_table::Color::White),
            Cell::new(format_energy(total_energy)).fg(comfy_table::Color::White),
            Cell::new(format_co2(total_co2)).fg(comfy_table::Color::White),
            Cell::new(format_water(total_water)).fg(comfy_table::Color::White),
            Cell::new(format_trees(total_trees)).fg(comfy_table::Color::White),
        ];
        if opts.show_efficiency {
            let mtok = total_tokens as f64 / 1_000_000.0;
            if mtok > 0.0 {
                total_row.push(
                    Cell::new(format!("{:.2}", total_cost / mtok)).fg(comfy_table::Color::White),
                );
                total_row.push(
                    Cell::new(format!("{:.1}", total_co2 / mtok)).fg(comfy_table::Color::White),
                );
            } else {
                total_row.push(Cell::new("-").fg(comfy_table::Color::White));
                total_row.push(Cell::new("-").fg(comfy_table::Color::White));
            }
        }
        if opts.show_cumulative {
            total_row.push(Cell::new(format_co2(total_co2)).fg(comfy_table::Color::White));
        }
        if !opts.no_guilt {
            if !opts.no_color {
                total_row.push(Cell::new(worst_title).fg(guilt_table_color(worst_guilt)));
            } else {
                total_row.push(Cell::new(worst_title));
            }
        }
        table.add_row(total_row);
    }

    println!("{table}");
}

fn guilt_table_color(level: GuiltLevel) -> comfy_table::Color {
    match level {
        GuiltLevel::DigitalSaint => comfy_table::Color::Green,
        GuiltLevel::CarbonCurious => comfy_table::Color::Cyan,
        GuiltLevel::TreeTrimmer => comfy_table::Color::Yellow,
        GuiltLevel::ForestFlattener => comfy_table::Color::DarkYellow,
        GuiltLevel::EcoTerrorist => comfy_table::Color::Red,
        GuiltLevel::PlanetIncinerator => comfy_table::Color::DarkRed,
        GuiltLevel::HeatDeathAccelerator => comfy_table::Color::Magenta,
        GuiltLevel::Himanshu => comfy_table::Color::White,
    }
}
