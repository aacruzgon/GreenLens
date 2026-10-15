# CLAUDE.md

The project guidance for all agents lives in AGENTS.md, so there is one copy to keep up to date. It is imported below.

@AGENTS.md

## Subagents (Claude Code only)

- At most **6 subagents** may run at once.
- They may do any type of work: research, audits, writing, code, verification.
- They may be any combination of **Opus 5.5** (`claude-opus-5-5`), **Sonnet 5.5** (`claude-sonnet-5-5`) and **Haiku 5.5** (`claude-haiku-5-5`). In the Agent tool these are `model: "opus"`, `"sonnet"` and `"haiku"`.

## Skills (Claude Code only)

- **Apple's Human Interface Guidelines:** `.claude/skills/` has one skill per page and per group of the [guidelines](https://developer.apple.com/design/human-interface-guidelines), named after its place in Apple's tree (`/hig-components-presentation-sheets`). Start with `/hig`, which has the full index, the Expo equivalents and the build and review steps. Use them when designing, building or reviewing anything in `app/` or `docs/frontend/screens.md`.
- Each page skill reads its page live with `.claude/skills/hig/scripts/hig_page.py`, so the repository holds Apple's links, not its text.
- The `hig-*` skills and `hig/index.md` are generated. Don't edit them; rerun `python3 .claude/skills/hig/scripts/build_skills.py` when Apple changes the guidelines.
