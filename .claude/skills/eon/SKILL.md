---
name: eon
description: "Era by Eon for PDM's farm connectors: the era CLI and our hosted company (Holling Holdings), Era's local Slack and Google Drive simulator images (compose profile, /_admin inject, tenant credentials, request journal, faults, day0/day1/day2, strict-mode OAuth), the Maple Row Farm fixture, and how the era_ tests run. Use before touching anything Era: running the simulators, writing or debugging an era_ test, calling the hosted company, rotating its token, or the era CI jobs."
---

# Era by Eon (EON)

Era gives PDM's farm connectors something real to talk to: Slack's and Google's own REST APIs, served by simulators with our synthetic farm injected, and a hosted synthetic company. This skill is the quick reference. The full record, with every result and the date it was checked, is [docs/EON](../../../docs/EON/README.md); the design is [docs/connectors](../../../docs/connectors/README.md).

**The documents win.** When this skill and [EON section 3](../../../docs/EON/README.md#3-era-what-we-checked-on-oct-19) disagree, section 3 was checked by running it; fix this skill.

## Which Era

| Need | Use | Costs calls? |
|---|---|---|
| Develop or test the clients and the sync, inject our farm, set faults, move time | **The simulator images**, locally or in CI | No |
| A nightly check that our clients work on Era's hosted systems | **The hosted company** (`era` CLI) | Yes: each call uses one of 2,000 a month |
| The demo garden on the live link | The images on the control plane (E9) | No |

Default to the images. The hosted company has no inject, faults or time controls, and every REST or MCP call to it is metered.

## The local simulators

Start them (the compose file pins tag and digest; [infra 4](../../../docs/infra/README.md#infradevcomposeyml)):

```bash
docker compose -f infra/dev.compose.yml --profile era up -d --wait era-slack era-drive
curl -s http://127.0.0.1:8091/_meta | jq .build.fidelity   # "high"; Drive is on :8092
```

- **Settings:** `SIMCORE_SEED=42`, `SIMCORE_TODAY=2026-10-15`, `SIMCORE_FIDELITY=high`, `AUTH_MODE=open`. With `docker run`, pass each as its own `-e`: joined into one string, the image silently falls back to `legacy` fidelity.
- **The day never moves.** Era tests pin `api`'s clock (the `Clock` port) to 2026-10-15T12:00:00Z, and to 91 days later after `day2`.
- **Tokens in open mode:** Slack accepts any `xoxb-…`; Drive any bearer. Tests use a minted tenant credential instead (below).
- **Health:** check `GET /_admin/tenants`. A fault also fails `/_meta`, and the image has `python3` but no `curl`.

### The admin API

All under `/_admin` on each simulator. It is never faulted, and needs no auth unless `SIMCORE_ADMIN_TOKEN` is set.

| Call | What it does |
|---|---|
| `POST /tenants/<t>/inject` | Adds records to tenant `<t>`, created on first use. Slack keys: `team`, `users`, `channels`, `messages`, `bots`, `oauth_apps`, `files`, `pins`. Drive keys: `files`, `sheets`, `comments`. Any other key is an error |
| `POST /tenants/<t>/credentials` with `{}` | A JWT in `access_token`, valid 1 hour. As the bearer token it selects the tenant (no `X-Sim-Tenant`) and turns on the journal; we found no other way to turn it on |
| `GET /tenants/<t>/requests` | The journal: `{requests: [{method, route, status, latency_ms}]}`, up to 500 entries kept 24 hours, only for calls made with a minted credential |
| `POST /tenants/<t>/reset` / `DELETE /tenants/<t>` | Back to the generated company / drop the tenant |
| `POST /tenants/<t>/state/{day0,day1,day2}`, `GET …/state` | Move the tenant in time; `day2` is +91 days |
| `POST /faults`, `POST /faults/reset` | `fail_next`, `retry_after_seconds`, `throttle`, `error_rate` (0–1), `latency_ms`, `quota_limit`. **Global to the simulator, all tenants** |

Inject formats ([EON 3, injecting the farm](../../../docs/EON/README.md#injecting-the-farm)):

- **Slack threads:** a thread's replies are a `reply` array on the parent message, not a separate message with `thread_ts`. A message with no `ts` gets the current time, so give every `ts`, inside the 180 days before 2026-10-15.
- **Bot membership:** the bot is a channel member only when `user-bot` is in the channel's `members`.
- **Drive file types:** a file with no `mimeType` is a Google Doc. A folder is `application/vnd.google-apps.folder`. A Sheet goes in `sheets` with `tabs: [{title, values}]`, and its CSV export holds only the first tab. A PDF injected by name only can't be downloaded (`403 fileNotDownloadable`).
- **Inject adds:** the generated company (Northwind Robotics) stays alongside the farm. Injecting `team` doesn't rename the workspace.

### Behaviour that trips people up

- **Slack refuses with HTTP 200:** `{"ok":false,"error":"not_authed"}` with no token, `invalid_auth` with a wrong one. Check `ok`, never only the status.
- **Read-only refusals differ by vendor:**
  - Slack: `403 writes_disabled`.
  - Drive: `403 PERMISSION_DENIED`, reason `insufficientPermissions`.
- **Faults ([EON 3, tenants, faults and time](../../../docs/EON/README.md#tenants-faults-and-time)):**
  - `fail_next` and `throttle` give `429` with `retry-after` set to `retry_after_seconds` (default 1). `throttle` keeps doing it on every call.
  - `error_rate: 1` gives `500`.
  - `quota_limit: 1` lets one call through, then gives `429` with `retry-after: 60` (ran on Slack on Oct 19, and on Drive by E2's tests).
  - Always `POST /faults/reset` afterwards, even when a test fails.
- **`day0` isn't usable:**
  - Drive lists nothing, injected files included.
  - Slack's `auth.test` answers `user_not_found`.
- **`changes.list` after a move** returns thousands of changes over several pages, not in time order. Read every page.
- **Links lead nowhere:**
  - Slack's `auth.test` `url` is always `http://127.0.0.1:8080/`.
  - Drive's `webViewLink` points at Google URLs that don't exist.
- **`AUTH_MODE=strict`** answers both vendors' OAuth paths:
  - Slack: `/oauth/v2/authorize`, then `/api/oauth.v2.access`.
  - Google: `/o/oauth2/v2/auth` with PKCE, then `/token`.
  - Drive checks scopes in this mode.
- **`SIMCORE_ADMIN_TOKEN`:**
  - `/_admin` needs it (`401` without it, `403` with a wrong one).
  - `X-Sim-Tenant` is ignored unless the admin token comes with it.

## The farm fixture

`backend/tests/fixtures/era/maple-row/` holds Maple Row Farm, in each simulator's inject format. Its [README](../../../backend/tests/fixtures/era/maple-row/README.md) lists the files, the planted cases and the inject commands.

| File | What it holds |
|---|---|
| `slack.json` | `#field-reports` (`C_FIELD`) and `#crew` (`C_CREW`): 94 messages and 29 replies in 20 threads |
| `google-drive.json` | Folder `F_FARM`: 3 Docs, the Sheet `S_PLAN`, one PDF |
| `garden.json` | The matching PDM garden |
| `answers.toml` | The evaluation's 19 questions |

Change the fixture and the Era tests' expected counts change with it.

## The tests

The harness is `integrations::era_sim`, behind the `era-sim` feature, so it is never in a release build. With it, a test:

- creates a tenant with a unique id;
- injects the fixture;
- mints a credential and passes it to the client as its token, so the client needs no test-only constructor;
- moves the tenant in time, reads the journal and sets faults.

```bash
cd backend
ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
  cargo nextest run -p integrations -p api --run-ignored only \
  -E 'test(/(^|::)era_/) and not test(/(^|::)era_strict_/)'
```

- **Marking:** Era tests are `#[ignore = "needs Era's simulators"]`, and their names start with `era_`. Fault tests start with `era_fault_`.
- **Missing variables fail the test,** they don't skip it, so a misconfigured CI job can't pass by doing nothing.
- **They run one at a time.** `backend/.config/nextest.toml` puts every `era_` test in the `era` group (`max-threads = 1`), because a fault reaches every test on that simulator. In CI, the fault tests have their own simulators (E6.2), and the strict-mode tests a third pair (E6.4): the `era` job in `ci.yml` has three nextest steps, one per pair, and is in `ci`'s `needs`, so a failing Era test blocks a merge ([infra 2](../../../docs/infra/README.md#the-era-job)). Strict tests read `ERA_STRICT_SLACK_BASE` and `ERA_STRICT_GOOGLE_BASE`; a locally started pair needs `-e AUTH_MODE=strict`, and the strict tests run alone with `ERA_STRICT_SLACK_BASE` and `ERA_STRICT_GOOGLE_BASE` set to that pair and `-E 'test(/(^|::)era_strict_/)'`.
- **Setting a fault:** create the tenant and inject first, so only the call under test meets the fault.
- **Wrap fault tests in a guard** that resets the faults on panic. Reset before setting, too.
- **`api`'s Era tests** (E4 onward) use the embedded store, a counting fake embedder and the pinned clock.

## The hosted company and the `era` CLI

Our company is **Holling Holdings**: e-commerce, B2B, small, with `slack` and `google-drive`. Its values are in `infra/env/.env.era`, which git ignores. Never commit it, print `ERA_TENANT_TOKEN`, or pass `--force`, which writes an env file git doesn't ignore.

| Command | Use |
|---|---|
| `era init` | Sign in through the browser; the key stays on this machine. On Oct 19 a run in a `!` prompt gave up after 300 s (exit 7, timed out), so run `era init --timeout 900` in a separate terminal |
| `era doctor --env infra/env/.env.era` | Check the settings, the credential and the console |
| `era usage [tenant]` | Metered calls this month, for the account or one company |
| `era ls [tenant]` | The companies, or one in full |
| `era systems`, `era options` | The systems; the industries, sizes, models and scenarios |
| `era plan --size small --model b2b --systems slack,google-drive` | What a company would hold, before building it |
| `era new --industry ecommerce --model b2b --size small --systems slack,google-drive --env infra/env/.env.era --strict` | Build a company and write its env block. `--strict` exits 6 if a system failed |
| `era token <tenant> --env infra/env/.env.era` | A new token, which retires the old one at once, for CI and every teammate too, so ask the user first. **Tokens last 7 days**: `ERA_TENANT_TOKEN_EXPIRES_AT` in the env file says when |
| `era env <tenant> --env infra/env/.env.era` | Rewrite the env block for an existing company |
| `era client <tenant>` | OAuth client credentials for a pipeline (E7.3 checks them against our OAuth exchanges) |
| `era mcp <tenant> --client claude` | The `claude mcp add` lines, one per system |
| `era rm <tenant>` | Delete a company. Ask the user first; it can't be undone |
| `era skill` | Prints Era's own generic skill: how to work across a company's systems |

**The env file** holds:
- `ERA_TENANT` and `ERA_TENANT_TOKEN`;
- `ERA_TENANT_TOKEN_EXPIRES_AT`;
- `SLACK_BASE_URL` (`https://slack-mcp.era.eon.io`) and `GOOGLE_DRIVE_BASE_URL` (`https://google-drive-mcp.era.eon.io`);
- each system's `*_MCP_URL` and `*_MCP_HEADER`.

The REST routes take the same `Authorization` header as MCP. Load the file with `set -a; . infra/env/.env.era; set +a`.

**Spend calls deliberately.** Each REST or MCP call to the hosted systems costs one of the 2,000 a month. Check `era usage` before and after, and record the cost in the PR.
- The first `curl` check cost 5 calls.
- A B2B small company keeps listings to a page or two.
- `era` commands that only talk to the console, such as `ls`, `usage` and `doctor`, aren't metered: `era usage` stayed at 5 after several of them on Oct 19.

**What the hosted company differs on:**
- Era's docs document no inject, fault or point-in-time commands for it. The console's **Advance to Next quarter** moves it, and the docs say tokens survive the move (docs only; not tried).
- Its data is Era's own, not our farm.
- Slack's `auth.test` `url` looks real (`https://hollingholdings.slack.com/`).

**In CI** (E6.5, not built yet): the nightly `era-hosted` job is to read `SLACK_BASE_URL`, `GOOGLE_DRIVE_BASE_URL` and `ERA_TENANT_TOKEN` from the GitHub environment `era-hosted`, which only `main` can use. The CLI's own CI credential is `ERA_TOKEN`. The job is to stay out of `ci`'s `needs`, so a hosted outage or an expired token can't block a merge.

## Sources

- [docs/EON](../../../docs/EON/README.md): the roadmap, what ran on Oct 19 (section 3), the decisions (section 10).
- [docs/connectors 9–10](../../../docs/connectors/README.md#9-era-as-the-test-bench): the test bench in the design; the measured behaviour.
- `era --help` and `era <command> --help` (era 0.1.5).
- Era's docs: index `https://console.era.eon.io/docs.md`; each article is also served as Markdown. Facts and limits: `https://console.era.eon.io/llms.txt`. Announcement: [Era, build and test AI agents](https://www.eon.io/blog/era-build-and-test-ai-agents). CI example: [eon-io/era/examples/ci](https://github.com/eon-io/era/tree/main/examples/ci).
