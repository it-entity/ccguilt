//! Shared formatting functions used by table, csv, markdown, and html renderers

pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

pub fn format_cost(usd: f64) -> String {
    if usd >= 1000.0 {
        format!("${:.0}", usd)
    } else if usd >= 1.0 {
        format!("${:.2}", usd)
    } else if usd >= 0.01 {
        format!("{:.0}c", usd * 100.0)
    } else if usd > 0.0 {
        format!("{:.2}c", usd * 100.0)
    } else {
        "$0".to_string()
    }
}

pub fn format_energy(wh: f64) -> String {
    if wh >= 1_000_000.0 {
        format!("{:.1} MWh", wh / 1_000_000.0)
    } else if wh >= 1000.0 {
        format!("{:.1} kWh", wh / 1000.0)
    } else if wh >= 1.0 {
        format!("{:.1} Wh", wh)
    } else {
        format!("{:.2} mWh", wh * 1000.0)
    }
}

pub fn format_co2(grams: f64) -> String {
    if grams >= 1_000_000.0 {
        format!("{:.1} t", grams / 1_000_000.0)
    } else if grams >= 1000.0 {
        format!("{:.1} kg", grams / 1000.0)
    } else if grams >= 1.0 {
        format!("{:.1} g", grams)
    } else {
        format!("{:.2} mg", grams * 1000.0)
    }
}

pub fn format_water(ml: f64) -> String {
    if ml >= 1_000_000.0 {
        format!("{:.1} kL", ml / 1_000_000.0)
    } else if ml >= 1000.0 {
        format!("{:.1} L", ml / 1000.0)
    } else if ml >= 1.0 {
        format!("{:.0} mL", ml)
    } else {
        format!("{:.2} uL", ml * 1000.0)
    }
}

pub fn format_trees(trees: f64) -> String {
    if trees >= 100.0 {
        format!("{:.0}", trees)
    } else if trees >= 1.0 {
        format!("{:.2}", trees)
    } else if trees >= 0.01 {
        format!("{:.4}", trees)
    } else if trees > 0.0 {
        format!("{:.6}", trees)
    } else {
        "0".to_string()
    }
}

pub fn sparkline(values: &[f64]) -> String {
    let chars = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let range = max - min;
    if range == 0.0 {
        return "\u{2584}".repeat(values.len());
    }
    values
        .iter()
        .map(|v| {
            let idx = (((v - min) / range) * 7.0).round() as usize;
            chars[idx.min(7)]
        })
        .collect()
}
