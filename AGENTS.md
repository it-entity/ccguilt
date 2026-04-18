# AGENTS.md

## Commands

```bash
cargo fmt --check          # check formatting (CI runs this first)
cargo fmt                  # auto-fix formatting — always run before committing
cargo clippy -- -D warnings # lint — warnings are hard errors in CI
cargo test                  # run tests
cargo build --release       # release build (LTO + stripped)
```

CI order: `fmt → clippy → test → build`. Default branch is `master`.

## Toolchain

Pinned to **Rust 1.94.0** via `rust-toolchain.toml`. Release workflow uses the same version.

## Project

ccguilt — Rust CLI that reads session data from Claude Code, OpenCode, and Gemini CLI and calculates environmental impact (energy, CO2, water) from token usage with satirical commentary.

## Remotes

| Remote | URL |
|--------|-----|
| `origin` | `it-entity/ccguilt` (GitHub fork) |
| `github-upstream` | `aayushh-code/ccguilt` (upstream) |
| `gitea` | internal Gitea mirror |
| `upstream` | internal Gitea canonical |

PRs go from `origin` → `github-upstream` via `gh pr create --repo aayushh-code/ccguilt`.

## Architecture

Data sources (selected at runtime, auto-detected):
- **Claude Code**: JSONL session files under `~/.claude/projects/`
- **OpenCode**: JSONL session files under `~/.opencode/`
- **Gemini CLI**: JSON session files under `~/.gemini/tmp/`

Three data paths:
- **SQLite-backed incremental** (default in deep scan): parses JSONL into `ccguilt.db`, re-ingests only changed files. Falls back to direct JSONL parse on DB error.
- **Direct JSONL** (`--no-db` or fallback): parallel-parses all JSONL via rayon, dedup by message ID (last line wins).
- **Fast mode** (`--fast`): reads `~/.claude/stats-cache.json`. No session/project detail.

Data flow: `CLI → config file merge → discover data dir → parse tokens → aggregate by period/group → calculate cost + impact → render output`

## Module layout (src/)

| Module | Purpose |
|--------|---------|
| `main.rs` | Entry point, arg routing, early-exit branches |
| `cli.rs` | clap args, Period/GroupBy enums |
| `models.rs` | TokenRecord, ModelTier, UsageBucket, CostSummary, ImpactSummary, GuiltLevel |
| `config.rs` | Energy profiles, pricing, environmental constants |
| `config_file.rs` | Loads `ccguilt.toml` user config, merges with CLI |
| `runtime.rs` | RuntimeConfig — merged CLI + config file settings |
| `aggregate.rs` | Buckets records by period/project/model; fast-path converters |
| `dateparse.rs` | Natural date parsing (`--since 7d`, `--since monday`, `--diff last-week`) |
| `sort_filter.rs` | Sort buckets, apply min-co2/min-cost/top-N filters |
| `calc/` | `cost.rs` (USD), `impact.rs` (environmental metrics + guilt level), `litellm.rs` (pricing lookup) |
| `data/` | `discovery.rs` (find data dirs), `jsonl.rs` (parallel parse), `cache.rs` (stats-cache.json), `db.rs` (SQLite incremental), `opencode.rs` (OpenCode source) |
| `display/` | `table.rs` (comfy-table), `json.rs`, `csv.rs`, `html.rs`, `markdown.rs`, `chart.rs`, `heatmap.rs`, `diff.rs`, `compare.rs`, `guilt.rs`, `session_detail.rs`, `offset.rs`, `mascot.rs`, `token_breakdown.rs` |
| `interactive/` | TUI mode (`-i`): `state.rs`, `render.rs` |
| `achievements.rs` | Hall of Shame achievement system |
| `recommend.rs` | Model cost/CO2 optimization tips |
| `forecast.rs` | Usage forecasting |
| `watch.rs` | `--watch` interval re-run |
| `completions.rs` | Shell completion setup |
| `update.rs` | Self-update (`--increase-guilt`) |

## Gotchas

- **comfy_table styling**: Never pass ANSI escape codes (e.g. from `colored::bold()`) into `Cell::new()` — comfy_table counts them as visible width, breaking alignment. Use comfy_table's native `Attribute::Bold`, `.fg()`, etc. instead.
- **rusqlite bundled**: Uses `bundled` feature — no system SQLite needed. The C lib compiles from source.
- **LiteLLM pricing**: Vendored at `vendor/litellm_prices.json`, baked in via `include_str!`. Refresh with `scripts/refresh-litellm.sh`. Fallback is tier-based pricing in `config.rs`.
- **ModelTier mapping**: Extracted from model name substring — "opus"/"sonnet"/"haiku"/"glm-5"/"glm-4"/"deepseek-reasoner". Unknown models → tier-based fallback.
- **JSONL filtering**: Only `type="assistant"` messages; skips `<synthetic>` models and zero-token entries.
- **Cache tokens**: cache_read = 0.10x energy multiplier; cache_creation = 1.0x.
- **PUE 1.2** multiplier on all energy calcs.
- **IndexMap** preserves model insertion order in output.
- **NO_COLOR** env var and `--no-color` flag both respected.
