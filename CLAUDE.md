# CLAUDE.md

The project guidance for all agents lives in AGENTS.md, so there is one copy to keep up to date. It is imported below.

@AGENTS.md

## Subagents (Claude Code only)

- At most **6 subagents** may run at once.
- They may do any type of work: research, audits, writing, code, verification.
- They may be any combination of **Opus 5.5** (`claude-opus-5-5`), **Sonnet 5.5** (`claude-sonnet-5-5`) and **Haiku 5.5** (`claude-haiku-5-5`). In the Agent tool these are `model: "opus"`, `"sonnet"` and `"haiku"`.
