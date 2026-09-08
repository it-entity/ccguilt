use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::HashMap;

/// One assistant message's token usage from a JSONL line
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TokenRecord {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub project_name: String,
    pub model: ModelTier,
    /// Raw model name as recorded by the data source (e.g. "claude-opus-4-6").
    /// Used to look up exact pricing via LiteLLM; falls back to tier-based if absent.
    pub model_raw: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

/// Normalized model tier for pricing and energy calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ModelTier {
    Opus,
    Sonnet,
    Haiku,
    Fable,
    Mythos,
    Glm5,
    Glm47,
    DeepSeekReasoner,
    Gemini25Pro,
    Gemini31Pro,
    GeminiFlash,
    Unknown,
}

impl ModelTier {
    pub fn from_model_string(s: &str) -> Option<Self> {
        if s == "<synthetic>" {
            return None;
        }
        let s_lower = s.to_lowercase();
        if s_lower.contains("opus") {
            Some(ModelTier::Opus)
        } else if s_lower.contains("sonnet") {
            Some(ModelTier::Sonnet)
        } else if s_lower.contains("haiku") {
            Some(ModelTier::Haiku)
        } else if s_lower.contains("fable") {
            Some(ModelTier::Fable)
        } else if s_lower.contains("mythos") {
            Some(ModelTier::Mythos)
        } else if s_lower.contains("glm-5") || s_lower.contains("glm5") {
            Some(ModelTier::Glm5)
        } else if s_lower.contains("glm-4") || s_lower.contains("glm4") {
            Some(ModelTier::Glm47)
        } else if s_lower.contains("deepseek-reasoner") || s_lower.contains("deepseek-r1") {
            Some(ModelTier::DeepSeekReasoner)
        } else if s_lower.contains("gemini") && s_lower.contains("flash") {
            Some(ModelTier::GeminiFlash)
        } else if s_lower.contains("gemini-3") || s_lower.contains("gemini3") {
            Some(ModelTier::Gemini31Pro)
        } else if s_lower.contains("gemini") {
            Some(ModelTier::Gemini25Pro)
        } else {
            Some(ModelTier::Unknown)
        }
    }

    pub fn as_db_str(&self) -> &'static str {
        match self {
            ModelTier::Opus => "Opus",
            ModelTier::Sonnet => "Sonnet",
            ModelTier::Haiku => "Haiku",
            ModelTier::Fable => "Fable",
            ModelTier::Mythos => "Mythos",
            ModelTier::Glm5 => "Glm5",
            ModelTier::Glm47 => "Glm47",
            ModelTier::DeepSeekReasoner => "DeepSeekReasoner",
            ModelTier::Gemini25Pro => "Gemini25Pro",
            ModelTier::Gemini31Pro => "Gemini31Pro",
            ModelTier::GeminiFlash => "GeminiFlash",
            ModelTier::Unknown => "Unknown",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "Opus" => ModelTier::Opus,
            "Sonnet" => ModelTier::Sonnet,
            "Haiku" => ModelTier::Haiku,
            "Fable" => ModelTier::Fable,
            "Mythos" => ModelTier::Mythos,
            "Glm5" => ModelTier::Glm5,
            "Glm47" => ModelTier::Glm47,
            "DeepSeekReasoner" => ModelTier::DeepSeekReasoner,
            "Gemini25Pro" => ModelTier::Gemini25Pro,
            "Gemini31Pro" => ModelTier::Gemini31Pro,
            "GeminiFlash" => ModelTier::GeminiFlash,
            _ => ModelTier::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ModelTier::Opus => "Opus",
            ModelTier::Sonnet => "Sonnet",
            ModelTier::Haiku => "Haiku",
            ModelTier::Fable => "Fable",
            ModelTier::Mythos => "Mythos",
            ModelTier::Glm5 => "GLM-5",
            ModelTier::Glm47 => "GLM-4.7",
            ModelTier::DeepSeekReasoner => "DeepSeek R1",
            ModelTier::Gemini25Pro => "Gemini 2.5 Pro",
            ModelTier::Gemini31Pro => "Gemini 3.1 Pro",
            ModelTier::GeminiFlash => "Gemini Flash",
            ModelTier::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(s: &str) -> ModelTier {
        ModelTier::from_model_string(s).unwrap()
    }

    #[test]
    fn recognizes_new_frontier_lines() {
        assert_eq!(tier("claude-fable-5"), ModelTier::Fable);
        assert_eq!(tier("claude-fable-5-1"), ModelTier::Fable);
        assert_eq!(tier("anthropic/claude-fable-5"), ModelTier::Fable);
        assert_eq!(tier("claude-mythos-5"), ModelTier::Mythos);
        assert_eq!(tier("claude-mythos-5-1"), ModelTier::Mythos);
        assert_eq!(tier("claude-mythos-preview"), ModelTier::Mythos);
    }

    #[test]
    fn recognizes_latest_established_models() {
        assert_eq!(tier("claude-opus-5"), ModelTier::Opus);
        assert_eq!(tier("claude-opus-4-8"), ModelTier::Opus);
        assert_eq!(tier("claude-sonnet-5"), ModelTier::Sonnet);
        assert_eq!(tier("zai/glm-5.3"), ModelTier::Glm5);
        assert_eq!(tier("zai/glm-5.3-flash"), ModelTier::Glm5);
        assert_eq!(tier("gemini-3.1-flash"), ModelTier::GeminiFlash);
    }

    #[test]
    fn synthetic_and_unknown() {
        assert!(ModelTier::from_model_string("<synthetic>").is_none());
        assert_eq!(tier("gpt-9-turbo"), ModelTier::Unknown);
    }

    #[test]
    fn tier_db_roundtrip() {
        for t in [
            ModelTier::Opus,
            ModelTier::Sonnet,
            ModelTier::Haiku,
            ModelTier::Fable,
            ModelTier::Mythos,
            ModelTier::Glm5,
            ModelTier::Glm47,
            ModelTier::DeepSeekReasoner,
            ModelTier::Gemini25Pro,
            ModelTier::Gemini31Pro,
            ModelTier::GeminiFlash,
        ] {
            assert_eq!(ModelTier::from_db_str(t.as_db_str()), t);
        }
    }
}

/// Aggregated token usage for a time bucket
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageBucket {
    pub label: String,
    pub tokens: TokenSummary,
    pub cost: CostSummary,
    pub impact: ImpactSummary,
    pub guilt: GuiltRating,
    pub models_used: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub by_model: HashMap<ModelTier, ModelTokens>,
}

impl TokenSummary {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CostSummary {
    pub input_cost_usd: f64,
    pub output_cost_usd: f64,
    pub cache_read_cost_usd: f64,
    pub cache_creation_cost_usd: f64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImpactSummary {
    pub energy_wh: f64,
    pub co2_grams: f64,
    pub water_ml: f64,
    pub trees_destroyed: f64,
    pub trees_dehydrated: f64,
    pub netflix_hours_equiv: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GuiltRating {
    pub level: GuiltLevel,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize)]
pub enum GuiltLevel {
    #[default]
    DigitalSaint,
    CarbonCurious,
    TreeTrimmer,
    ForestFlattener,
    EcoTerrorist,
    PlanetIncinerator,
    HeatDeathAccelerator,
    Himanshu,
}

/// Fast-path data from stats-cache.json
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FastPathData {
    pub model_usage: IndexMap<String, CacheModelUsage>,
    pub daily_tokens: Vec<CacheDailyTokens>,
    pub total_sessions: u64,
    pub total_messages: u64,
    pub first_session_date: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheModelUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheDailyTokens {
    pub date: String,
    pub tokens_by_model: HashMap<String, u64>,
}
