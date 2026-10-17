# CLAUDE.md

The project guidance for all agents lives in AGENTS.md, so there is one copy to keep up to date. It is imported below.

@AGENTS.md

## Subagents (Claude Code only)

- At most **6 subagents** may run at once. The count includes every agent running, at every level, not only the ones you started.
- They may do any type of work: research, audits, writing, code, verification.
- They may be any combination of **Opus 5.5** (`claude-opus-5-5`), **Sonnet 5.5** (`claude-sonnet-5-5`) and **Haiku 5.5** (`claude-haiku-5-5`). In the Agent tool these are `model: "opus"`, `"sonnet"` and `"haiku"`.

Use them sparingly. A new agent can't see your context, so it re-reads everything it needs, and the large documents here cost hundreds of thousands of tokens per agent. Resuming an agent reloads its whole transcript.

- **Spawn only when it pays.** Do small or sequential work yourself. Spawn for work that is large and independent, or that must be checked by fresh eyes (an audit of work you or another agent wrote).
- **Subagents don't spawn subagents.** Only the main session starts agents. Write "don't start subagents" into every brief, unless the user asked for a fan-out.
- **One agent per job by default.** Split a job across agents only when the parts are independent and each is large, and say why in the brief.
- **Pick the model for the job:**
  - **Haiku:** mechanical checks, such as link checks, re-running a script, grep sweeps or counting.
  - **Sonnet:** well-specified edits and fixes from an exact list, routine research, writing to a template.
  - **Opus:** design, adversarial audits, and work that needs judgement across many documents.
- **Narrow briefs.** Name the files and sections to read, with line ranges or a grep where possible, never "read docs/backend". Pass the facts the agent needs in the brief instead of making it rediscover them. Ask for a short report.
- **Run expensive checks once.** Run the example checker and the link checks at the end of a job, not after every edit.

## Skills (Claude Code only)

- **Apple's Human Interface Guidelines:** `.claude/skills/` has one skill per page and per group of the [guidelines](https://developer.apple.com/design/human-interface-guidelines), named after its place in Apple's tree (`/hig-components-presentation-sheets`). Start with `/hig`, which has the full index, the Expo equivalents and the build and review steps. Use them when designing, building or reviewing anything in `app/` or `docs/frontend/screens.md`.
- Each page skill reads its page live with `.claude/skills/hig/scripts/hig_page.py`, so the repository holds Apple's links, not its text.
- The `hig-*` skills and `hig/index.md` are generated. Don't edit them; rerun `python3 .claude/skills/hig/scripts/build_skills.py` when Apple changes the guidelines.
- **The Rust backend:** start with `/rust` before writing or reviewing any Rust, `Cargo.toml` or backend design. It routes to the area skills, holds the pinned crate versions (`rust/versions.md`) and the example checker. The listing has too many skills to show every description, so use the area skill by name:

  | Skill | Use it for |
  |---|---|
  | `/rust-async` | tokio tasks, cancellation, channels, shutdown, async traits |
  | `/rust-axum-api` | routes, handlers, extractors, `ApiError`, Clerk and worker auth, SSE, layers, OpenAPI |
  | `/rust-contract-types` | `domain` wire types: serde, IDs, timestamps, tagged unions, utoipa and schemars |
  | `/rust-errors` | error enums, the contracts §2 mapping, logging error chains, worker job errors |
  | `/rust-tracing` | the JSON subscriber, spans, request IDs, what never to log |
  | `/rust-surrealdb` | `store` queries, transactions, conflict retries, batched deletes |
  | `/rust-workers` | the worker binary: leases, claims, drain, reqwest, ort |
  | `/rust-testing` | handler, property, mock and SurrealDB container tests |
  | `/rust-cargo-lints` | the workspace manifest, lints, `clippy.toml`, profiles |

- The skills follow the project's documents and link a source for every rule. Every Rust example in them compiles and passes Clippy: run `python3 .claude/skills/rust/scripts/check_examples.py` after changing one, and add `--live` (Docker and cargo-nextest at the pinned version) to also run the SurrealDB tests against the pinned container. A definition shared between skills has one canonical copy; the checker fails when a marked copy drifts from it.

## Plugins (Claude Code only)

- **Expo's official plugin** (`expo@claude-plugins-official`) is enabled for the project in `.claude/settings.json`. It adds [Expo Skills](https://docs.expo.dev/skills/) (`/expo:expo-router`, `/expo:expo-native-ui`, `/expo:expo-module`, `/expo:eas-app-stores` and others) and the [Expo MCP server](https://docs.expo.dev/mcp.md), which signs in with an Expo account through `/mcp`.
- **Clerk's official plugin** (`clerk@clerk-skills`) is enabled too, from Clerk's own marketplace (`clerk/skills`, declared in `extraKnownMarketplaces`). It adds [Clerk Skills](https://github.com/clerk/skills), among them `/clerk:clerk-expo` for `@clerk/expo`, and the [Clerk MCP server](https://clerk.com/docs/guides/ai/mcp). That server signs in to your Clerk workspace and acts as you: it can list, create and change users and applications and create production instances. Approve its write calls one by one, and never run them against the production instance without asking.
- Committing these settings doesn't download the plugins. Each teammate installs them once, then runs `/reload-plugins`:
  - `claude plugin install expo@claude-plugins-official --scope project`
  - `claude plugin install clerk --marketplace clerk/skills --scope project`

  A SessionStart hook (`.claude/hooks/check-plugins.mjs`) checks every plugin the project enables and says what to install when one is missing. To opt out of one, set it to `false` in `.claude/settings.local.json`.
- **The project's documents win over a plugin's defaults.** Expo's skills assume the latest SDK and their own library choices. Clerk's `clerk-expo` skill was written for `@clerk/expo` 3.x and Expo SDK 53–56, while this app uses `@clerk/expo` 4.13 on SDK 57. Follow `docs/frontend` (§3 for pins, §8 for Clerk) and the installed package's own types, and don't let `expo-upgrade` move the app past SDK 57 until the team takes frontend open decision 1 (`@clerk/expo` 4.13 already supports SDK 58).
