# PDM and SurrealDB Agent Memory

> **Status:** v0.1, 2026-10-17. v0.1: first version. The design is implemented in `domain`, `integrations`, `agent`, `store` and `api`; the behaviour of the hosted service was measured on 2026-10-17 with a script of about 140 requests, and the adapter's ignored live test (`agent_memory::tests::live_round_trip_supersedes_and_forgets`) passed against the `pdm-dev` context.
> **Verified:** a claim marked ✓ was run against the live hosted service on 2026-10-17. Anything taken from the vendor's documents is marked *unverified*. Companion to the [database notes](README.md); the wire shapes are in [contracts §8](../architecture/contracts.md#memory) and the design in [architecture §7](../architecture/README.md#memory).

## Contents

1. [What it is](#1-what-it-is)
2. [Setup](#2-setup)
3. [Measured behaviour](#3-measured-behaviour)
4. [The design](#4-the-design)
5. [Privacy](#5-privacy)
6. [Caveats and open items](#6-caveats-and-open-items)
7. [Sources](#7-sources)

---

## 1. What it is

SurrealDB Agent Memory is a hosted memory service from SurrealDB Cloud. An agent writes facts about an entity, and the service keeps them with a validity period: a newer value for the same attribute **supersedes** the older one, and the history stays readable. It also answers semantic queries, with each hit traced to its source. It has a REST API and no Rust SDK, so the `integrations` crate calls the REST API (*unverified*, vendor docs).

The Build North sponsor pack gives each participant a $50 credit (code `BUILDNORTH50`), and offers a prize, "Best Use of SurrealDB Agent Memory", of $1,000 in credit. The criteria:

- facts that supersede rather than overwrite;
- retrieval traceable to its source;
- memory shared coherently across more than one agent (here: the chat agent, the plan writer and the pipeline's scan summaries).

The `memory` table in our own SurrealDB stays the system of record. Agent Memory holds a mirror of the **shared** memories only.

## 2. Setup

**One context per environment:** `pdm-dev` and `pdm-prod`. A context is the unit of isolation; entity names are matched across a whole context (section 3, point 2).

**The API key** is created in Studio's "Create API key" form and narrowed there. An empty field inherits full access, which is the trap: fill every field.

| Grant | Value | Why |
|---|---|---|
| `memory:read`, `memory:write`, `memory:forget` | `gardens/*` | The mirror records, reads and erases garden memories |
| `scope:read`, `scope:create` | `gardens/*` | Garden and plant scopes are created on first write |
| `scope:delete` | `none/*` | The key can't tombstone a scope |
| `grant:manage` | `none/*` | The key can't widen its own access |

Write `gardens/*`, not `gardens`: a grant on a node alone doesn't cover its children. The check in section 3, point 9, shows what the narrowed key can and can't do. `allow_self_service_keys: false` in the context config stops a key from minting another one.

**Environment variables of `api`** ([backend §9](../backend/README.md#9-configuration), [infra §6](../infra/README.md#6-secrets-and-configuration)), all three or none:

| Variable | Value |
|---|---|
| `AGENT_MEMORY_URL` | The context's REST base URL |
| `AGENT_MEMORY_CONTEXT_ID` | `pdm-dev` or `pdm-prod` |
| `AGENT_MEMORY_API_KEY` | The narrowed key; a secret |

The vendor's own documents call these `SPECTRON_*` and the headers `X-Spectron-*`, the product's former name (*unverified*). Ours are `AGENT_MEMORY_*`.

## 3. Measured behaviour

Run on 2026-10-17 against the hosted service, from a script with about 140 requests. Every point below is ✓.

1. **Scope lenses.** ✓ A read with a scope `lens` returns only facts tagged at exactly that path: a plant path doesn't return garden-level facts, and the garden path doesn't return the plants' facts. An OR lens `[[plant],[garden]]` returns both. Facts under a separate tree (`chats/…`) never appeared in a garden read.
2. **Entities are matched by name across the whole context.** ✓ "tomato plant" in two gardens is one entity. Attributes keep their own scope and didn't supersede each other across gardens, but deleting the entity would affect both. So entity names are our ids (section 4).
3. **Free-text writes (`infer: "full"`) are unreliable.** ✓
   - One plant became two entities ("p1" and "tomato plant p1").
   - "tomato plant" to "tomato" was recorded as a spurious correction.
   - `/query` still returned the superseded sentence.
   - `asOf` didn't filter the conversation chunks.
   - Writes took 4–6 s, because Agent Memory runs its own language model on them.
4. **Triples (`infer: "triples"`) supersede reliably.** ✓ The old value gets `validUntil` and `supersededBy`, the history endpoint shows the chain, and `asOf` one day back returned the old value. A triple write takes about 0.8 s. Labels stick when sent at the top level of the body. Action triples (`verb`) are stored but not returned by the entity read, so events are recorded as `last_<topic>` attributes.
5. **Deleting.** ✓
   - Deleting a session removes the conversation chunk but keeps the facts extracted from it.
   - Semantic `POST /forget` isn't scoped to a garden: a dry run with one garden's exact sentence matched 2 records across two gardens, and with another sentence 0.
   - `DELETE /entities/{type}/{name}` is soft: the deleted value stays in the history.
   - `POST /scopes/forget` hard-deletes every fact and entity in the subtree (3 to 34 in the runs) and left no entities behind.
   - Re-recording after an entity delete or a scope forget works and rebuilds the history in order.
6. **No `uncertainty` record** ✓ was raised for a user claim ("waters every day") against a scan observation ("soil dry and cracked"). The only uncertainty in the runs came from extraction ("who sprayed the neem oil?"). Conflict detection would be our own logic.
7. **Latency.** ✓

   | Call | Time |
   |---|---|
   | `/query`, `hybrid` tier | median 2.1 s |
   | `/query` when it escalates (about two thirds of queries) | median 5.3 s, max 10.5 s |
   | `/context` | median 3.2 s |
   | `GET /entities/…` and `/attributes` | 0.37–0.40 s |
   | `GET …/history` | about 0.4 s |
   | triple write | about 0.8 s |

   `mode` (hybrid, vector, bm25, graph) made no difference to `/query`.
8. **Scopes.** ✓ Registering a scope path twice returns `201` both times. Writing to an unregistered path returns `400 unknown scope node — register it first`. Parent paths are created automatically. ✓ A path segment with an uppercase letter is refused with `400 invalid scope path: illegal character in segment`, so the ids in paths and entity names are lowercased; a ULID is case-insensitive, and the `memory=<id>` label keeps the uppercase id.
9. **The narrowed key.** ✓ `GET /me` reported exactly the narrowed effective grants. Creating garden scopes, writing, reading, searching, deleting an entity and erasing a scope were allowed. Creating a scope outside `gardens/`, writing under `chats/`, tombstoning a scope and granting itself access got `403`. The key could mint a new key (`POST /keys`, `201`), but the minted key had the same narrow grants, so there is no escalation.
10. **Cost.** The API reports no token counts. Check the Usage page in Studio.

From the vendor's documents, not run: embeddings are fixed to `gemini-embedding-2` (*unverified*).

## 4. The design

The `memory` table is the system of record. Shared memories (`source` `pipeline` or `user`) are mirrored as **triples**, because triples supersede reliably and free text doesn't (section 3, points 3 and 4).

**One triple per shared memory**

| Part | Value |
|---|---|
| Entity | `plant_<plant id>` or `garden_<garden id>`, the id lowercased, type `other` |
| Attribute key | from the mapping below |
| Value | the memory's text |
| Labels | `memory=<memory id>`, `source=<pipeline\|user>` |
| `observed_at` | the memory's `created_at` |
| Scope path | `gardens/<garden id>` or `gardens/<garden id>/plants/<plant id>`, ids lowercased |

**Key mapping** (`domain::ports::belief_key`)

| Memory | Key | Supersedes |
|---|---|---|
| `summary` | `scan_summary` | the previous scan summary of that subject |
| `fact`, topic `other` | `note_<memory id>` | nothing |
| `fact`, any other topic | the topic, for example `watering` | the previous fact with that topic |
| `event` | `last_<topic>`, for example `last_treatment` | the previous event with that topic |

A newer memory with the same key on the same subject supersedes the older one, and Agent Memory keeps the history.

**Port.** `BeliefStore` (`backend/crates/domain/src/ports/beliefs.rs`) has `record`, `current`, `history`, `search` and `forget`. `integrations::agent_memory::AgentMemory` implements it over REST. `NoBeliefs` stands in when Agent Memory isn't configured.

**Writes and the sweep.** `agent::beliefs::MirroredMemory` wraps the SurrealDB `MemoryStore`. After a note, a pipeline summary or a share commits, it records the memory and sets `memory.belief_synced_at`. A failure is logged and left for the sweep. The scheduler task `beliefs` runs every minute and records unsynced shared memories oldest first, stopping at the first failure, so supersession order holds.

**Reads**
- The chat context gets the current beliefs of the plant (or garden) as their own data block, by a direct entity read, under a 1.5 s timeout (about 0.4 s measured).
- The plan writer gets the plant's current beliefs. They are shared only, so the rule that the plan writer reads only shared memories holds.
- The `search_memory` tool returns `MemorySearch { memories, beliefs }`: the local hits plus Agent Memory `/query` hits under a 4 s timeout. `beliefs` is empty when it is slow or down; `/query` takes 2–10 s (section 3, point 7).
- `GET /plants/{plant_id}/beliefs` returns `PlantBeliefs { available, current, history }` for any member ([contracts §8](../architecture/contracts.md#beliefs)). `available` is `false` when Agent Memory is off or doesn't answer. It is never a `5xx`.

**Deletion** ([contracts §9](../architecture/contracts.md#deletion-cascades))
- Deleting a plant or garden erases its scope subtree with `POST /scopes/forget`, which is permanent, after the objects and before the records. A failure resumes the cascade on restart.
- When an owner deletes one shared memory, the mirror erases that subject's scope, marks the subject's remaining shared memories unsynced and deletes the row. The sweep records them again in order. Semantic `/forget` isn't used, because it isn't scoped to a garden (section 3, point 5).
- Account deletion needs nothing: shared memories stay with the garden, and private ones were never sent.

**Disabled mode.** Without the three variables, `NoBeliefs` is used. Nothing is recorded, the context block and the plan writer get no beliefs, `search_memory` returns only `memories`, and the endpoint answers `available: false`. Chat and plans work on the `memory` table alone.

## 5. Privacy

- **What leaves our control plane:** the text of shared memories, their topic, their date, and the garden, plant and memory ids, sent over HTTPS to SurrealDB Cloud with the narrowed key.
- **What never leaves:** private chat memories (sharing one is the consent to send it), photos, depth maps, locations, names and emails.
- **Who processes it:** SurrealDB Cloud stores it. Embeddings are fixed to `gemini-embedding-2`, so the text is embedded by Google through SurrealDB Cloud (*unverified*, vendor docs).
- **Erasure:** deleting a plant or garden erases its scope for good; deleting one shared memory erases and rebuilds its subject's scope (section 4).
- **Scope of the key:** it can act only under `gardens/` and can't change grants (section 2).

## 6. Caveats and open items

- **The API is pre-GA,** and its documents already announce a breaking change (*unverified*). `AgentMemory` is the only code that knows the wire format.
- **Cost is unknown.** No token counts come back (section 3, point 10). Read Studio's Usage page after a day of use and compare it with the $50 credit.
- **Conflicts are ours to build.** Agent Memory raised no `uncertainty` for a claim against a scan observation (section 3, point 6). Detecting that "waters every day" disagrees with "soil dry and cracked" would be our own logic, and isn't built.
- **A wrong topic supersedes the wrong fact.** A feeding note filed as `watering` replaces the watering belief. `other` is the safe default, because its `note_<id>` key supersedes nothing. The extraction prompt and the `remember` tool should pick a specific topic only when it is clear.
- **Entity names are context-wide,** so a plant or garden id must never be reused or shared between environments' contexts.
- **Latency:** `/query` is slow, so it only backs `search_memory`; the chat context uses the direct entity read.
- **Sync lag.** A memory is a belief only after the commit and the next successful record; the sweep runs every minute.
- **Open:** a conflict check against scan observations; a read of the history in the app beyond the endpoint; whether to raise `/query` timeouts once the real latency is known.

## 7. Sources

- Agent Memory: https://surrealdb.com/docs/agent-memory
- REST API reference: https://surrealdb.com/docs/agent-memory/reference/rest-api
- Contexts and scope: https://surrealdb.com/docs/agent-memory/mental-model/contexts-and-scope
- Memory lifecycle: https://surrealdb.com/docs/agent-memory/mental-model/memory-lifecycle
- Forget: https://surrealdb.com/docs/agent-memory/operations/forget
- Bulk import (triples): https://surrealdb.com/docs/agent-memory/ingest/authoritative/bulk-import
- Sponsor pack: https://surrealdb.notion.site/Build-North-SurrealDB-participant-resources-pack-3edbfa44ae3e81a0a6ebd0b4fbb802d3
