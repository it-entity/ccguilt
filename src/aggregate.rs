use chrono::{Datelike, IsoWeek};
use indexmap::IndexMap;

use crate::calc::cost::accumulate_record_cost;
use crate::calc::impact::{calculate_impact_with, determine_guilt};
use crate::cli::Period;
use crate::models::{ModelTier, TokenRecord, UsageBucket};

#[allow(dead_code)]
pub fn aggregate(records: &[TokenRecord], period: Period) -> Vec<UsageBucket> {
    aggregate_with(
        records,
        period,
        crate::config::CO2_KG_PER_KWH,
        crate::config::PUE,
    )
}

pub fn aggregate_with(
    records: &[TokenRecord],
    period: Period,
    co2_kg_per_kwh: f64,
    pue: f64,
) -> Vec<UsageBucket> {
    match period {
        Period::Daily => aggregate_by(
            records,
            |r| r.timestamp.format("%Y-%m-%d").to_string(),
            co2_kg_per_kwh,
            pue,
        ),
        Period::Weekly => aggregate_by(
            records,
            |r| {
                let w: IsoWeek = r.timestamp.iso_week();
                format!("{}-W{:02}", w.year(), w.week())
            },
            co2_kg_per_kwh,
            pue,
        ),
        Period::Monthly => aggregate_by(
            records,
            |r| r.timestamp.format("%Y-%m").to_string(),
            co2_kg_per_kwh,
            pue,
        ),
        Period::Session => aggregate_by(records, |r| r.session_id.clone(), co2_kg_per_kwh, pue),
        Period::Total => {
            let mut bucket = UsageBucket {
                label: "All Time".to_string(),
                ..Default::default()
            };
            for record in records {
                add_record_to_bucket(&mut bucket, record);
            }
            finalize_bucket(&mut bucket, co2_kg_per_kwh, pue);
            vec![bucket]
        }
    }
}

pub fn aggregate_by_project(
    records: &[TokenRecord],
    co2_kg_per_kwh: f64,
    pue: f64,
) -> Vec<UsageBucket> {
    aggregate_by(
        records,
        |r| crate::data::discovery::decode_project_name(&r.project_name),
        co2_kg_per_kwh,
        pue,
    )
}

pub fn aggregate_by_model(
    records: &[TokenRecord],
    co2_kg_per_kwh: f64,
    pue: f64,
) -> Vec<UsageBucket> {
    aggregate_by(
        records,
        |r| r.model.display_name().to_string(),
        co2_kg_per_kwh,
        pue,
    )
}

fn aggregate_by<F: Fn(&TokenRecord) -> String>(
    records: &[TokenRecord],
    key_fn: F,
    co2_kg_per_kwh: f64,
    pue: f64,
) -> Vec<UsageBucket> {
    let mut buckets: IndexMap<String, UsageBucket> = IndexMap::new();

    for record in records {
        let key = key_fn(record);
        let bucket = buckets.entry(key.clone()).or_insert_with(|| UsageBucket {
            label: key,
            ..Default::default()
        });
        add_record_to_bucket(bucket, record);
    }

    for bucket in buckets.values_mut() {
        finalize_bucket(bucket, co2_kg_per_kwh, pue);
    }

    buckets.into_values().collect()
}

fn add_record_to_bucket(bucket: &mut UsageBucket, record: &TokenRecord) {
    bucket.tokens.input_tokens += record.input_tokens;
    bucket.tokens.output_tokens += record.output_tokens;
    bucket.tokens.cache_creation_tokens += record.cache_creation_input_tokens;
    bucket.tokens.cache_read_tokens += record.cache_read_input_tokens;

    let model_entry = bucket.tokens.by_model.entry(record.model).or_default();
    model_entry.input_tokens += record.input_tokens;
    model_entry.output_tokens += record.output_tokens;
    model_entry.cache_creation_tokens += record.cache_creation_input_tokens;
    model_entry.cache_read_tokens += record.cache_read_input_tokens;

    accumulate_record_cost(&mut bucket.cost, record);

    bucket
        .models_used
        .insert(record.model.display_name().to_string());
}

fn finalize_bucket(bucket: &mut UsageBucket, co2_kg_per_kwh: f64, pue: f64) {
    bucket.impact = calculate_impact_with(&bucket.tokens, co2_kg_per_kwh, pue);
    bucket.guilt = determine_guilt(&bucket.impact);
}

pub fn fast_path_total(
    model_usage: &indexmap::IndexMap<String, crate::models::CacheModelUsage>,
) -> Vec<TokenRecord> {
    use chrono::Utc;

    let mut records = Vec::new();
    let now = Utc::now();

    for (model_name, usage) in model_usage {
        let tier = match ModelTier::from_model_string(model_name) {
            Some(t) => t,
            None => continue,
        };

        records.push(TokenRecord {
            timestamp: now,
            session_id: "aggregate".to_string(),
            project_name: "all".to_string(),
            model: tier,
            model_raw: model_name.clone(),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_creation_input_tokens: usage.cache_creation_input_tokens,
            cache_read_input_tokens: usage.cache_read_input_tokens,
        });
    }

    records
}

pub fn fast_path_daily(
    daily_tokens: &[crate::models::CacheDailyTokens],
    model_usage: &indexmap::IndexMap<String, crate::models::CacheModelUsage>,
) -> Vec<TokenRecord> {
    let mut records = Vec::new();

    for day in daily_tokens {
        let timestamp = match chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
            Ok(d) => d.and_hms_opt(12, 0, 0).unwrap().and_utc(),
            Err(_) => continue,
        };

        for (model_name, &day_total) in &day.tokens_by_model {
            let tier = match ModelTier::from_model_string(model_name) {
                Some(t) => t,
                None => continue,
            };

            let (input_ratio, output_ratio, cache_create_ratio, cache_read_ratio) =
                if let Some(agg) = model_usage.get(model_name) {
                    let total = agg.input_tokens
                        + agg.output_tokens
                        + agg.cache_creation_input_tokens
                        + agg.cache_read_input_tokens;
                    if total == 0 {
                        (0.25, 0.25, 0.25, 0.25)
                    } else {
                        let t = total as f64;
                        (
                            agg.input_tokens as f64 / t,
                            agg.output_tokens as f64 / t,
                            agg.cache_creation_input_tokens as f64 / t,
                            agg.cache_read_input_tokens as f64 / t,
                        )
                    }
                } else {
                    (0.25, 0.25, 0.25, 0.25)
                };

            records.push(TokenRecord {
                timestamp,
                session_id: format!("fast-{}", day.date),
                project_name: "all".to_string(),
                model: tier,
                model_raw: model_name.clone(),
                input_tokens: (day_total as f64 * input_ratio) as u64,
                output_tokens: (day_total as f64 * output_ratio) as u64,
                cache_creation_input_tokens: (day_total as f64 * cache_create_ratio) as u64,
                cache_read_input_tokens: (day_total as f64 * cache_read_ratio) as u64,
            });
        }
    }

    records
}
