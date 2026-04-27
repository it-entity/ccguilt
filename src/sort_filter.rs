use crate::cli::SortField;
use crate::models::UsageBucket;

pub fn sort_buckets(buckets: &mut Vec<UsageBucket>, field: SortField) {
    let get_val = |bucket: &UsageBucket| -> f64 {
        match field {
            SortField::Co2 => bucket.impact.co2_grams,
            SortField::Cost => bucket.cost.total_cost_usd,
            SortField::Tokens => bucket.tokens.total_tokens() as f64,
            SortField::Energy => bucket.impact.energy_wh,
            SortField::Water => bucket.impact.water_ml,
        }
    };

    let mut decorated: Vec<(u64, UsageBucket)> = buckets
        .drain(..)
        .map(|b| {
            let v = get_val(&b);
            (v.to_bits(), b)
        })
        .collect();
    decorated.sort_by(|a, b| b.0.cmp(&a.0));
    buckets.extend(decorated.into_iter().map(|(_, b)| b));
}

pub fn filter_min_co2(buckets: &mut Vec<UsageBucket>, min_co2: f64) {
    buckets.retain(|b| b.impact.co2_grams >= min_co2);
}

pub fn filter_min_cost(buckets: &mut Vec<UsageBucket>, min_cost: f64) {
    buckets.retain(|b| b.cost.total_cost_usd >= min_cost);
}
