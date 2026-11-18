use crate::calc::litellm;
use crate::config::pricing_profile;
use crate::config::PricingProfile;
use crate::models::{CostSummary, ModelTier, ModelTokens, TokenRecord, TokenSummary};

pub fn pricing_for(model_raw: &str, tier: ModelTier) -> &'static PricingProfile {
    if !model_raw.is_empty() {
        if let Some(p) = litellm::lookup(model_raw) {
            return p;
        }
    }
    pricing_profile(tier)
}

fn cost_from_tokens(
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    pricing: &PricingProfile,
) -> CostSummary {
    let input = input_tokens as f64 * pricing.input_per_mtok / 1_000_000.0;
    let output = output_tokens as f64 * pricing.output_per_mtok / 1_000_000.0;
    let cache_read = cache_read_tokens as f64 * pricing.cache_read_per_mtok / 1_000_000.0;
    let cache_creation =
        cache_creation_tokens as f64 * pricing.cache_creation_per_mtok / 1_000_000.0;

    CostSummary {
        input_cost_usd: input,
        output_cost_usd: output,
        cache_read_cost_usd: cache_read,
        cache_creation_cost_usd: cache_creation,
        total_cost_usd: input + output + cache_read + cache_creation,
    }
}

pub fn calculate_record_cost(record: &TokenRecord) -> CostSummary {
    let pricing = pricing_for(&record.model_raw, record.model);
    cost_from_tokens(
        record.input_tokens,
        record.output_tokens,
        record.cache_creation_input_tokens,
        record.cache_read_input_tokens,
        pricing,
    )
}

pub fn calculate_model_cost(tokens: &ModelTokens, tier: ModelTier) -> CostSummary {
    let pricing = pricing_for("", tier);
    cost_from_tokens(
        tokens.input_tokens,
        tokens.output_tokens,
        tokens.cache_creation_tokens,
        tokens.cache_read_tokens,
        pricing,
    )
}

/// Legacy path used when we don't have raw names (e.g., fast-path reconstructions
/// that lose per-record identity). Uses tier pricing.
#[allow(dead_code)]
pub fn calculate_total_cost(summary: &TokenSummary) -> CostSummary {
    let mut total = CostSummary::default();

    for (tier, model_tokens) in &summary.by_model {
        let c = calculate_model_cost(model_tokens, *tier);
        total.input_cost_usd += c.input_cost_usd;
        total.output_cost_usd += c.output_cost_usd;
        total.cache_read_cost_usd += c.cache_read_cost_usd;
        total.cache_creation_cost_usd += c.cache_creation_cost_usd;
        total.total_cost_usd += c.total_cost_usd;
    }

    total
}

fn add(dst: &mut CostSummary, src: &CostSummary) {
    dst.input_cost_usd += src.input_cost_usd;
    dst.output_cost_usd += src.output_cost_usd;
    dst.cache_read_cost_usd += src.cache_read_cost_usd;
    dst.cache_creation_cost_usd += src.cache_creation_cost_usd;
    dst.total_cost_usd += src.total_cost_usd;
}

/// Accumulate a record's cost into an existing CostSummary in place.
pub fn accumulate_record_cost(dst: &mut CostSummary, record: &TokenRecord) {
    let c = calculate_record_cost(record);
    add(dst, &c);
}
