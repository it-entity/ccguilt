# ccguilt

**Claude Code Guilt Trip** — a satirical environmental impact tracker for your AI habit.

Reads your local Claude Code session data and computes token usage, API cost, energy consumption, CO2 emissions, water usage, and tree destruction metrics. All wrapped in darkly humorous commentary backed by peer-reviewed research.

## Install

```bash
curl -sSL https://raw.githubusercontent.com/aayushh-code/ccguilt/master/install.sh | bash
```

Or from Gitea (LAN only):

```bash
curl -sSL http://git-aeoru.local:3000/entity/ccguilt/raw/branch/master/install.sh | bash
```

Or build from source:

```bash
git clone https://github.com/aayushh-code/ccguilt.git
cd ccguilt
cargo install --path .
```

## Shell Completions

Enable tab completion for all flags and options:

```bash
ccguilt --setup-completions          # auto-detect your shell
ccguilt --setup-completions bash     # explicit shell
ccguilt --setup-completions zsh
ccguilt --setup-completions fish
```

Restart your terminal afterward. You can also generate raw completion scripts for manual setup:

```bash
ccguilt --completions bash > ~/.local/share/bash-completion/completions/ccguilt
```

## Usage

### Report Periods

```bash
ccguilt daily                        # per-day breakdown (default)
ccguilt weekly                       # per-week
ccguilt monthly                      # per-month
ccguilt session                      # per-session
ccguilt total                        # all-time summary
```

### Date Filtering

```bash
ccguilt daily --since 2026-03-01     # from a specific date
ccguilt daily --until 2026-03-15     # up to a specific date
ccguilt daily --since 7d             # last 7 days
ccguilt daily --since 2w             # last 2 weeks
ccguilt daily --since last-week      # last week
ccguilt daily --since yesterday      # since yesterday
ccguilt daily --since monday         # since Monday
```

### Project Filtering

```bash
ccguilt daily --project myproject           # filter by project (substring match)
ccguilt daily --project-regex "my.*proj"    # filter by project (regex)
```

### Data Mode

```bash
ccguilt total --fast                 # quick mode via stats-cache.json
ccguilt daily --claude-home /path    # custom Claude data directory
```

### Analysis Options

```bash
ccguilt daily --by-model             # show per-model token breakdown
ccguilt daily --sort co2             # sort by metric (co2, cost, tokens, energy, water)
ccguilt daily --top 5                # show only top 5 periods
ccguilt daily --group-by project     # group by project instead of time
ccguilt daily --group-by model       # group by model instead of time
ccguilt daily --efficiency           # show $/Mtok, gCO2/Mtok metrics
ccguilt daily --cumulative           # show cumulative running totals
ccguilt daily --min-co2 10           # hide periods below 10g CO2
ccguilt daily --min-cost 1.0         # hide periods below $1.00
ccguilt daily --budget 50kg          # show progress toward a carbon budget
ccguilt --compare proj1,proj2        # compare projects side-by-side
```

### Output Formats

```bash
ccguilt daily --json                 # JSON output
ccguilt daily --csv                  # CSV output
ccguilt daily --markdown             # Markdown table
ccguilt daily --html report.html     # standalone HTML report
ccguilt daily --output report.csv    # auto-detect format from extension (.csv, .json, .html, .md)
```

### Visualization

```bash
ccguilt daily --chart                # bar chart of CO2/water per period
ccguilt daily --sparkline            # sparklines in table/footer
ccguilt --heatmap                    # calendar heatmap of daily CO2
```

### Display Options

```bash
ccguilt daily --no-guilt             # hide satirical commentary (coward mode)
ccguilt daily --no-color             # disable colored output (also respects NO_COLOR env)
ccguilt daily -q                     # quiet mode: suppress progress messages
ccguilt daily -v                     # verbose mode: show per-file parsing details
```

### Interactive & Watch Modes

```bash
ccguilt -i                           # launch interactive TUI
ccguilt --watch                      # re-run every 30 seconds
ccguilt --watch 10                   # re-run every 10 seconds
```

### Comparison & Insights

```bash
ccguilt --diff last-week this-week   # compare two time periods
ccguilt --recommend                  # model cost/CO2 optimization tips
ccguilt --achievements               # Hall of Shame (unlocked achievements)
ccguilt --offset                     # carbon offset options
ccguilt --projects                   # all projects ranked by impact
ccguilt --session abc123             # detailed timeline for a session (substring match on ID)
```

### Utility

```bash
ccguilt --hook-output                # single compact line (for git hooks)
ccguilt --increase-guilt             # check for updates and self-update
ccguilt --mcp                        # run as an MCP server (see below)
ccguilt --setup-mcp                  # one-shot register MCP server with Claude Code
ccguilt --version                    # print version
```

## MCP Server

ccguilt can run as an MCP (Model Context Protocol) server so Claude Code can call it during your conversations to check usage in real time.

If you installed via `install.sh` and have Claude Code on your PATH, it's already registered — skip ahead to the tools list. Otherwise:

```bash
ccguilt --setup-mcp        # one-shot, idempotent, registers at user scope
```

(If you'd rather do it manually: `claude mcp add --scope user ccguilt -- $(which ccguilt) --mcp`.)

Then in any Claude Code session, ask things like *"how much CO2 have I burned today?"* and Claude will call the ccguilt tools directly.

### Tools exposed

| Tool | Purpose |
|------|---------|
| `ccguilt_today` | Today's tokens, cost, energy, CO2, water, trees, guilt level |
| `ccguilt_total` | All-time cumulative impact |
| `ccguilt_range` | Custom date range — `since`/`until` accept YYYY-MM-DD, `7d`, `last-week`, `yesterday`, `monday`, etc. |

### Tree-fallen warnings

Each tool response may include a `tree_fallen_warning` field with a satirical message — but only when a *new* tree's worth of CO2 (22 kg) has accumulated since the last warning. State is persisted in `~/.local/share/ccguilt/mcp_state.json`, so you'll see one warning per tree, naturally rate-limited. On first install the current floor is recorded silently — historical accumulation doesn't trigger a flood of warnings.

## What it shows

| Column | Description | Source |
|--------|-------------|--------|
| Tokens | Input, output, cache read/write tokens | Claude Code JSONL session files |
| Cost | Estimated API cost in USD | Anthropic pricing (per model tier) |
| Energy | Watt-hours consumed | Jegham et al. 2025, Luccioni et al. 2023 |
| CO2 | Carbon dioxide emitted | EPA eGRID 2024 (0.39 kg/kWh US avg) |
| Water | Data center cooling water | Li et al. 2023 "Making AI Less Thirsty" |
| Trees | Equivalent trees destroyed per year | EPA (22 kg CO2/tree/year absorption) |
| Guilt | 7-tier satirical rating | Your conscience |

## Guilt Ratings

| Rating | CO2 Threshold | Flavor |
|--------|--------------|--------|
| Digital Saint | < 10g | "Your carbon footprint is basically a carbon toe-print" |
| Carbon Curious | < 100g | "Dipping your toes into environmental destruction" |
| Tree Trimmer | < 500g | "A few branches fell. The forest will recover. Probably." |
| Forest Flattener | < 2kg | "The squirrels are filing a class action" |
| Eco-Terrorist | < 10kg | "Greenpeace has entered the chat. And they brought lawyers." |
| Planet Incinerator | < 50kg | "Making Venus look hospitable" |
| Heat Death Accelerator | < 500kg | "The universe was going to end eventually. You're just helping it along." |
| Himanshu | 500kg+ | "Himanshu." |

## Example Output

```
==================================================================
  CLAUDE CODE GUILT TRIP
  An environmental impact report nobody asked for
==================================================================

╭──────────┬────────┬───────┬─────────┬───────┬────────┬───────┬────────────────────────╮
│ Period   ┆ Tokens ┆  Cost ┆  Energy ┆   CO2 ┆  Water ┆ Trees ┆          Guilt         │
╞══════════╪════════╪═══════╪═════════╪═══════╪════════╪═══════╪════════════════════════╡
│ All Time ┆ 1.8B   ┆ $4797 ┆ 2.9 MWh ┆ 1.1 t ┆ 5.2 m3 ┆ 51.30 ┆ Heat Death Accelerator │
╰──────────┴────────┴───────┴─────────┴───────┴────────┴───────┴────────────────────────╯

  Trees completely destroyed: 51  |  Next victim: [#########.....................] 29.5%

  🚽 That's 868 toilet flushes. At least those serve a purpose.
  🔋 Enough energy to charge your phone 192905 times.

  "This report was generated by Claude Code, consuming additional energy.
   We are the problem reporting on the problem."
```

## Environmental Data Sources

All environmental calculations are backed by peer-reviewed research:

- **Energy per token**: Jegham et al. 2025 "How Hungry is AI?", Luccioni et al. 2023 "Power Hungry Processing"
- **Carbon intensity**: EPA eGRID 2024 (US average grid: 0.39 kgCO2/kWh)
- **Water usage**: Li et al. 2023 "Making AI Less Thirsty" (WUE: 1.8 L/kWh)
- **Tree absorption**: EPA (22 kg CO2/tree/year), USDA Forestry (3,900 L water/tree/year)
- **Data center PUE**: Industry average 1.2x overhead
- **Model pricing**: Sourced from a snapshot of [LiteLLM's `model_prices_and_context_window.json`](https://github.com/BerriAI/litellm/blob/main/model_prices_and_context_window.json) — same source `ccusage` uses — vendored at `vendor/litellm_prices.json`. Looked up by exact model name at runtime, with a tier-based fallback if a model isn't in the snapshot. Refresh with `scripts/refresh-litellm.sh`.

## How it reads your data

ccguilt reads Claude Code's local session files at `~/.claude/projects/`. It parses JSONL conversation logs to extract per-message token usage with model identification, then aggregates by your chosen time period.

- **Deep scan** (default): Parses all JSONL session files with parallel processing (rayon). Accurate per-model energy calculation.
- **Fast mode** (`--fast`): Reads `~/.claude/stats-cache.json` for instant results with estimated token distribution.

## License

MIT
