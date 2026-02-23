---
name: devboy-agent-usage
description: Analyze AI agent (Claude Code) usage — costs, tasks, time tracking,
  focus analysis. Use when user asks about Claude Code costs, token usage, task
  breakdown, productivity metrics, or session analysis.
allowed-tools: Bash(devboy-agent-usage:*)
---

# devboy-agent-usage

CLI tool for analyzing Claude Code usage: costs, tasks, sessions, focus.

## Commands

### Cost analysis

```bash
# Cost by day for current month
devboy-agent-usage cost --from 2026-02-01 --group-by day

# Cost by week for a project
devboy-agent-usage cost --from 2026-02-01 --group-by week --project myproject

# Cost by session
devboy-agent-usage cost --from 2026-02-20 --group-by session
```

### Task breakdown

```bash
# Tasks grouped by git branch
devboy-agent-usage tasks --from 2026-02-20

# With LLM titles and summaries (requires LLM API configured)
devboy-agent-usage tasks --from 2026-02-20 --with-llm

# With ActivityWatch human time tracking
devboy-agent-usage tasks --from 2026-02-20 --with-aw

# Sort by time/sessions/recent instead of cost
devboy-agent-usage tasks --from 2026-02-20 --sort recent
```

### Summary

```bash
# Overall stats for a date range
devboy-agent-usage summary --from 2026-02-20 --to 2026-02-23

# Filter by project
devboy-agent-usage summary --from 2026-02-01 --project myproject
```

### Sessions

```bash
# Recent sessions
devboy-agent-usage sessions --from 2026-02-20 --limit 10

# Session details (pass UUID or prefix)
devboy-agent-usage session abc12345

# With LLM chunk summaries
devboy-agent-usage session abc12345 --with-llm
```

### Timeline

```bash
# By task ID (from git branch, e.g. DEV-570)
devboy-agent-usage timeline DEV-570

# By session UUID
devboy-agent-usage timeline abc12345
```

### Projects

```bash
devboy-agent-usage projects
```

### Focus & browser (requires ActivityWatch)

```bash
devboy-agent-usage focus --from 2026-02-20
devboy-agent-usage browse <SESSION_ID>
```

### Task management

```bash
# Set manual title for a task
devboy-agent-usage retitle DEV-531 "Multi-project JIRA support"

# Clear LLM cache to re-summarize
devboy-agent-usage reclassify --from 2026-02-20
```

## Output formats

All commands support `--format table|json|csv` (default: table).

```bash
devboy-agent-usage tasks --from 2026-02-20 --format json
devboy-agent-usage cost --from 2026-02-01 --format csv
```

## Tips

- Dates use `YYYY-MM-DD` format: `--from 2026-02-20 --to 2026-02-23`
- `--with-llm` requires LLM API configured (env vars `TRACK_CLAUDE_LLM_*`)
- `--with-aw` requires ActivityWatch running
- Use `--format json` when you need to process the output programmatically
- `--sort recent` is useful to see what was worked on last
