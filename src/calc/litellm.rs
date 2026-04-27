//! Pricing lookup sourced from a vendored snapshot of LiteLLM's
//! `model_prices_and_context_window.json` (the same data ccusage uses).
//!
//! The snapshot lives at `vendor/litellm_prices.json` and is baked into the
//! binary via `include_str!`. At runtime we parse it once into a HashMap keyed
//! by model name and serve direct lookups. If a model isn't found, callers
//! fall back to tier-based pricing in `config.rs`.
//!
//! To refresh the snapshot: `scripts/refresh-litellm.sh` (or fetch
//! https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json
//! and filter to providers ccguilt tracks).

use crate::config::PricingProfile;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const LITELLM_JSON: &str = include_str!("../../vendor/litellm_prices.json");

/// Raw entry shape from the LiteLLM JSON. Costs are per-token (not per-million).
#[derive(Deserialize)]
struct RawEntry {
    #[serde(default)]
    input_cost_per_token: Option<f64>,
    #[serde(default)]
    output_cost_per_token: Option<f64>,
    #[serde(default)]
    cache_creation_input_token_cost: Option<f64>,
    #[serde(default)]
    cache_read_input_token_cost: Option<f64>,
}

fn table() -> &'static HashMap<String, PricingProfile> {
    static TABLE: OnceLock<HashMap<String, PricingProfile>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let raw: HashMap<String, RawEntry> = serde_json::from_str(LITELLM_JSON).unwrap_or_default();
        raw.into_iter()
            .filter_map(|(name, e)| {
                let input = e.input_cost_per_token?;
                let output = e.output_cost_per_token?;
                Some((
                    name,
                    PricingProfile {
                        input_per_mtok: input * 1_000_000.0,
                        output_per_mtok: output * 1_000_000.0,
                        cache_read_per_mtok: e.cache_read_input_token_cost.unwrap_or(input * 0.1)
                            * 1_000_000.0,
                        cache_creation_per_mtok: e
                            .cache_creation_input_token_cost
                            .unwrap_or(input * 1.25)
                            * 1_000_000.0,
                    },
                ))
            })
            .collect()
    })
}

/// Look up pricing by exact model name. Returns None if the model isn't in the
/// vendored LiteLLM snapshot — caller should fall back to tier-based pricing.
///
/// Tries a few sensible name variants because Claude Code and OpenCode write
/// model strings slightly differently (`claude-opus-4-6` vs `anthropic/claude-opus-4-6`).
///
/// Returns a `&'static` reference into the lazily-initialized pricing table,
/// avoiding a per-record copy of the `PricingProfile` struct.
pub fn lookup(model_name: &str) -> Option<&'static PricingProfile> {
    let t = table();
    if let Some(p) = t.get(model_name) {
        return Some(p);
    }
    for prefix in ["anthropic/", "openrouter/anthropic/", "bedrock/"] {
        if let Some(stripped) = model_name.strip_prefix(prefix) {
            if let Some(p) = t.get(stripped) {
                return Some(p);
            }
        }
    }
    None
}
