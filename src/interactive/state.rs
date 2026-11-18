use chrono::Datelike;

use crate::cli::SortField;
use crate::display::DisplayOptions;
use crate::models::{TokenRecord, UsageBucket};
use crate::runtime::RuntimeConfig;

#[allow(dead_code)]
pub struct AppState {
    pub buckets: Vec<UsageBucket>,
    pub records: Vec<TokenRecord>,
    pub view: View,
    pub selected: usize,
    pub scroll_offset: usize,
    pub sort_field: Option<SortField>,
    pub by_model: bool,
    pub display_opts: DisplayOptions,
    pub rc: RuntimeConfig,
    pub drill_stack: Vec<DrillLevel>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct DrillLevel {
    pub label: String,
    pub buckets: Vec<UsageBucket>,
    pub selected: usize,
}

pub enum View {
    Table,
    Chart,
}

impl AppState {
    pub fn new(
        records: Vec<TokenRecord>,
        buckets: Vec<UsageBucket>,
        display_opts: DisplayOptions,
        rc: RuntimeConfig,
    ) -> Self {
        Self {
            records,
            buckets,
            view: View::Table,
            selected: 0,
            scroll_offset: 0,
            sort_field: None,
            by_model: false,
            display_opts,
            rc,
            drill_stack: Vec::new(),
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.buckets.len() {
            self.selected += 1;
        }
    }

    pub fn cycle_view(&mut self) {
        self.view = match self.view {
            View::Table => View::Chart,
            View::Chart => View::Table,
        };
    }

    pub fn cycle_sort(&mut self) {
        self.sort_field = match self.sort_field {
            None => Some(SortField::Co2),
            Some(SortField::Co2) => Some(SortField::Cost),
            Some(SortField::Cost) => Some(SortField::Tokens),
            Some(SortField::Tokens) => Some(SortField::Energy),
            Some(SortField::Energy) => Some(SortField::Water),
            Some(SortField::Water) => None,
        };
        if let Some(field) = self.sort_field {
            crate::sort_filter::sort_buckets(&mut self.buckets, field);
        } else {
            // Restore chronological order
            self.buckets.sort_by(|a, b| a.label.cmp(&b.label));
        }
        self.selected = 0;
    }

    pub fn toggle_by_model(&mut self) {
        self.by_model = !self.by_model;
    }

    pub fn drill_down(&mut self) {
        if self.buckets.is_empty() {
            return;
        }
        let bucket = &self.buckets[self.selected];
        let label = bucket.label.clone();

        // Save current level
        self.drill_stack.push(DrillLevel {
            label: "Previous".to_string(),
            buckets: self.buckets.clone(),
            selected: self.selected,
        });

        // Filter records matching this bucket's label and re-aggregate at finer granularity
        let filtered: Vec<TokenRecord> = self
            .records
            .iter()
            .filter(|r| {
                let record_label = r.timestamp.format("%Y-%m-%d").to_string();
                let record_week = {
                    let w = r.timestamp.iso_week();
                    format!("{}-W{:02}", chrono::Datelike::year(&r.timestamp), w.week())
                };
                let record_month = r.timestamp.format("%Y-%m").to_string();
                label == record_label
                    || label == record_week
                    || label == record_month
                    || label == r.session_id
                    || label == "All Time"
            })
            .cloned()
            .collect();

        if filtered.is_empty() {
            self.drill_stack.pop();
            return;
        }

        let new_buckets = if label.contains("-W") {
            crate::aggregate::aggregate_with(
                &filtered,
                crate::cli::Period::Daily,
                self.rc.co2_kg_per_kwh,
                self.rc.pue,
            )
        } else if label.len() == 10 {
            crate::aggregate::aggregate_with(
                &filtered,
                crate::cli::Period::Session,
                self.rc.co2_kg_per_kwh,
                self.rc.pue,
            )
        } else {
            // Can't drill deeper
            self.drill_stack.pop();
            return;
        };

        self.buckets = new_buckets;
        self.selected = 0;
    }

    pub fn drill_up(&mut self) {
        if let Some(level) = self.drill_stack.pop() {
            self.buckets = level.buckets;
            self.selected = level.selected;
        }
    }

    pub fn sort_label(&self) -> &str {
        match self.sort_field {
            None => "Date",
            Some(SortField::Co2) => "CO2",
            Some(SortField::Cost) => "Cost",
            Some(SortField::Tokens) => "Tokens",
            Some(SortField::Energy) => "Energy",
            Some(SortField::Water) => "Water",
        }
    }
}
