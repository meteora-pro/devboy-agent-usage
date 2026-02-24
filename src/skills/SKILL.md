---
name: devboy-tools-agent-usage
description: Analyze AI agent (Claude Code) usage — costs, tasks, time tracking,
  focus analysis. Use when user asks about Claude Code costs, token usage, task
  breakdown, productivity metrics, or session analysis.
allowed-tools: Bash(devboy-tools-agent-usage:*)
---

# devboy-tools-agent-usage

CLI tool for analyzing Claude Code usage: costs, tasks, sessions, focus.

## Commands

### Cost analysis

```bash
# Cost by day for current month
devboy-tools-agent-usage cost --from 2026-02-01 --group-by day

# Cost by week for a project
devboy-tools-agent-usage cost --from 2026-02-01 --group-by week --project myproject

# Cost by session
devboy-tools-agent-usage cost --from 2026-02-20 --group-by session
```

### Task breakdown

```bash
# Tasks grouped by git branch
devboy-tools-agent-usage tasks --from 2026-02-20

# With LLM titles and summaries (requires LLM API configured)
devboy-tools-agent-usage tasks --from 2026-02-20 --with-llm

# With ActivityWatch human time tracking
devboy-tools-agent-usage tasks --from 2026-02-20 --with-aw

# Sort by time/sessions/recent instead of cost
devboy-tools-agent-usage tasks --from 2026-02-20 --sort recent
```

### Summary

```bash
# Overall stats for a date range
devboy-tools-agent-usage summary --from 2026-02-20 --to 2026-02-23

# Filter by project
devboy-tools-agent-usage summary --from 2026-02-01 --project myproject
```

### Sessions

```bash
# Recent sessions
devboy-tools-agent-usage sessions --from 2026-02-20 --limit 10

# Session details (pass UUID or prefix)
devboy-tools-agent-usage session abc12345

# With LLM chunk summaries
devboy-tools-agent-usage session abc12345 --with-llm
```

### Timeline

```bash
# By task ID (from git branch, e.g. DEV-570)
devboy-tools-agent-usage timeline DEV-570

# By session UUID
devboy-tools-agent-usage timeline abc12345
```

### Projects

```bash
devboy-tools-agent-usage projects
```

### Focus & browser (requires ActivityWatch)

```bash
devboy-tools-agent-usage focus --from 2026-02-20
devboy-tools-agent-usage browse <SESSION_ID>
```

### Task management

```bash
# Set manual title for a task
devboy-tools-agent-usage retitle DEV-531 "Multi-project JIRA support"

# Clear LLM cache to re-summarize
devboy-tools-agent-usage reclassify --from 2026-02-20
```

## Output formats

All commands support `--format table|json|csv` (default: table).

```bash
devboy-tools-agent-usage tasks --from 2026-02-20 --format json
devboy-tools-agent-usage cost --from 2026-02-01 --format csv
```

## LLM configuration (--with-llm)

Requires one of:

Anthropic (only API key needed, URL auto-configured):
```bash
export TRACK_CLAUDE_LLM_API_KEY=sk-ant-...
```

Ollama / local LLM (free):
```bash
export TRACK_CLAUDE_LLM_PROVIDER=openai
export TRACK_CLAUDE_LLM_URL=http://localhost:11434/v1/chat/completions
export TRACK_CLAUDE_LLM_MODEL=qwen2.5:7b
```

All env vars:
- `TRACK_CLAUDE_LLM_API_KEY` — API key (required for Anthropic)
- `TRACK_CLAUDE_LLM_PROVIDER` — `anthropic` (default) or `openai`
- `TRACK_CLAUDE_LLM_MODEL` — model name (default: `claude-3-5-haiku-20241022`)
- `TRACK_CLAUDE_LLM_URL` — full endpoint URL (usually not needed, auto-constructed)

Results are cached in SQLite — repeated runs don't spend tokens on already processed tasks.

## ActivityWatch (--with-aw)

ActivityWatch is auto-detected if installed:
- macOS: `~/Library/Application Support/activitywatch/aw-server/peewee-sqlite.v2.db`
- Linux: `~/.local/share/activitywatch/aw-server/peewee-sqlite.v2.db`

No configuration needed — just install and run ActivityWatch.

## Tips

- Dates use `YYYY-MM-DD` format: `--from 2026-02-20 --to 2026-02-23`
- `--with-llm` requires LLM API configured (see above)
- `--with-aw` requires ActivityWatch running
- Use `--format json` when you need to process the output programmatically
- `--sort recent` is useful to see what was worked on last
