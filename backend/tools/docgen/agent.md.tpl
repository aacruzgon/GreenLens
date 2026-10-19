# PDM Backend: `agent` crate

> **Status:** Draft v0.5, 2026-10-19. v0.5: every successful tool result reaches the model inside a `<data kind="tool_result">` block (`prompt::tool_result`; a tool may pick another kind through `Tool::DATA_KIND`), refusals stay plain, and the stored copy keeps the tool's JSON; `DataKind` gains `ToolResult` and `FarmRecords`; the `<data` escape also catches a tag at the very end of a body, Unicode whitespace and zero-width characters after `<` or `/` (also in a JSON result's strings and keys, before it is written), the fullwidth and small `<`, and a multi-byte character after `data`; the chat prompt says where tool results arrive and moved to `2026-10-19.1`; 85 unit tests. v0.4: shared memories are recorded in `SurrealDB` Agent Memory through the new `beliefs` module (the `sync_pending` sweep, the only writer, and the reads `current_beliefs` and `search_beliefs`); memories and beliefs reach the model with their day; the context carries the current beliefs in their own `beliefs` block, `search_memory` returns `MemorySearch` with belief hits, the plan writer reads the plant's current beliefs, memories carry a `topic` (extraction and `remember`), and the chat, plan and extraction prompts moved to `2026-10-17.1`; 77 unit tests. v0.3: only the three tools that propose changes are `strict` (`ToolSpec::strict`), because all nineteen made a grammar the API refused as too large in the first live call; the Anthropic adapter maps `model_context_window_exceeded` to `MaxTokens` and ends a stream cut off before its stop reason with `Unavailable`; 62 unit tests and three ignored live tests (`tests/live_anthropic.rs`). v0.2: one `embedder` in `AgentDeps`, called with `EmbedMode::Query` for the user's message and searches and `EmbedMode::Document` for the memories it stores (`document_embedder` is gone). v0.1: the agent core (the chat turn and its stream, nineteen tools, the dose check, plan validation, the router and safety gate, the plan writer, the context builder, memory extraction and the Anthropic and classifier adapters), compiled on Rust 1.99.1 with 60 unit tests.

Companion to the [backend architecture](README.md) and the [`domain` crate](domain.md), whose ports this crate uses and implements. It implements [architecture 7](../architecture/README.md#7-chat-and-agent-core), [8.2](../architecture/README.md#82-chat-turn) and the safety rules of [architecture 11](../architecture/README.md#11-security-privacy-and-safety), over the shapes of [contracts 7](../architecture/contracts.md#7-pipeline-records) and [contracts 8](../architecture/contracts.md#8-chat-and-memory). Every listing below is copied from the compiled source of `crates/agent`: `cargo check --workspace`, `cargo clippy -p agent --all-targets` (no warnings in `agent`) and `cargo test -p agent` (83 tests) pass.

## Contents

1. [Purpose and boundaries](#1-purpose-and-boundaries)
2. [Dependencies](#2-dependencies)
3. [Module layout](#3-module-layout)
4. [The dose check](#4-the-dose-check)
5. [Sentences and products](#5-sentences-and-products)
6. [Plan validation](#6-plan-validation)
7. [Router and safety gate](#7-router-and-safety-gate)
8. [Prompts and the prompt-injection rule](#8-prompts-and-the-prompt-injection-rule)
9. [Model adapters](#9-model-adapters)
10. [The plan writer](#10-the-plan-writer)
11. [Context per anchor](#11-context-per-anchor)
12. [Tools](#12-tools)
13. [The chat turn](#13-the-chat-turn)
14. [Memory](#14-memory)
15. [Concurrency and lifetimes](#15-concurrency-and-lifetimes)
16. [Errors](#16-errors)
17. [Testing](#17-testing)
18. [Open questions](#18-open-questions)
19. [Sources](#19-sources)

---

## 1. Purpose and boundaries

**What it owns.**

| Part | Module | Called by |
|---|---|---|
| The chat turn: validation, budget, routing, context, streaming, tools, storing the reply | `turn` | `api` (`POST /conversations/{id}/messages`) |
| Nineteen tools with typed arguments, filtered by role | `tools` | the turn |
| The dose check, sentence splitting, the product list, plan validation, the router's decision | `safety` | the turn, the tools, the plan writer, and through it `pipeline` |
| The plan writer | `plan_writer` | `pipeline` (the `plan` step), never the chat |
| The context builder per anchor | `context` | the turn |
| Memory extraction and conversation summaries | `memory` | the turn |
| The belief mirror: shared memories into Agent Memory, the sweep, the bounded belief reads | `beliefs` | `api` (wraps its `MemoryStore`, runs the sweep), the context, `search_memory`, `pipeline` |
| `ChatModel` over the Anthropic Messages API, and `TextClassifier`'s LLM fallback | `model` | `api` wires them into `AgentDeps` |
| `apply_preview`, the pure half of confirming a `revise_plan` card | `tools::revise` | `api` (`POST /pending-actions/{id}/confirm`) |

**What it never does.**

- It stores no plan. The plan writer returns a [`WrittenPlan`](#10-the-plan-writer); `pipeline` assembles and stores the `ActionPlan` under the scan's plan lock ([contracts 7, re-planning](../architecture/contracts.md#actionplan)).
- It never confirms a card. Write tools store a `proposed` pending action; confirming is `api`'s ([contracts 8, pending actions](../architecture/contracts.md#pending-actions-confirmation-cards)).
- It never takes identity from the model. Every tool runs with the session's `ConversationOwner<Viewer>` proof ([domain 8](domain.md#8-authorization-proofs)); no argument names a garden.
- It speaks HTTP to no one itself. The Anthropic client is `integrations::anthropic`; Jev's adapter is `integrations::jev`.
- It writes no SSE bytes. The turn yields `domain::chat::ChatEvent` values; `api` writes each as `Event::default().event(e.event_name()).json_data(&e)` with no `id:` line.

**Where it departs from the planned layout.** [Backend README 6](README.md#6-workspace-layout) sketches `src/{safety/, plan_writer, context, tools/, turn, memory, classify, prompts/}`. The compiled crate has `model/` (the two adapters, including the classifier) instead of `classify`, one `prompt.rs` instead of `prompts/`, and `deps.rs`, `config.rs` and `error.rs` ([section 3](#3-module-layout)).

---

## 2. Dependencies

@@file ../Cargo.toml@@

| Crate | Why |
|---|---|
| `domain` | Every port, record and proof |
| `integrations` | `AnthropicClient`, its borrowed request types, `MessageAccumulator` and the schema sanitiser |
| `async-trait` | Implementing `domain`'s `#[async_trait]` ports (`ChatModel`, `TextClassifier`) |
| `async-stream` | `try_stream!` for `ChatModel::stream`, which borrows its request for `'a` |
| `futures` | `BoxStream`, `BoxFuture`, `StreamExt` |
| `regex` 1.13.1 | The dose check's compiled patterns and `RegexSet`, the product list, the poisoning check. New in `[workspace.dependencies]` |
| `image` | Cropping a photo for `view_image(region)` (WebP in, PNG out) |
| `schemars` | The JSON Schema of every tool's arguments, the plan writer's output and the extraction output |
| `serde`, `serde_json` | Tool arguments and results, prompt data |
| `thiserror` | The error enums ([section 16](#16-errors)) |
| `tokio`, `tokio-stream`, `tokio-util` | The turn task, `mpsc`/`oneshot`, `ReceiverStream`, `CancellationToken`, `TaskTracker`, `spawn_blocking`, `time::timeout` |
| `tracing` | Logs with ids and error chains only ([section 15](#15-concurrency-and-lifetimes)) |
| `bytes`, `chrono` | Image bytes, dates |

Tests add `domain`'s `mock` and `test-util` features (one `mockall` mock per port, `FixedClock`, `SequentialIds`) and tokio's `test-util`.

---

## 3. Module layout

```
crates/agent/src/
├─ lib.rs            re-exports: ChatAgent, TurnRequest, TurnStream, TurnOutcome, PlanWriter, PlanInput, …
├─ beliefs.rs        sync_pending (the belief sweep), SweepReport, current_beliefs, search_beliefs
├─ config.rs         AgentConfig: limits, budgets, belief timeouts, the poison-control text, the router's calibration
├─ deps.rs           AgentDeps and AgentStores: every port, as Arc<dyn …>
├─ error.rs          TurnError, PlanWriterError, ToolError, MemoryError, PrivateMemory
├─ prompt.rs         versioned system prompts, the <data> wrapper, fixed reply texts
├─ safety.rs         re-exports of the five safety modules
├─ safety/
│  ├─ dose.rs        the dose check: patterns, DoseMatch<'t>, dose_check
│  ├─ sentences.rs   sentence spans and the streaming SentenceSplitter
│  ├─ products.rs    ProductCatalogue: built-in product words + products.yaml
│  ├─ plan.rs        PlanDraft, ValidationContext<'k>, validate_plan
│  └─ routes.rs      Calibration, RouteDecision, decide, the poisoning check, fixed safety texts
├─ model.rs
├─ model/
│  ├─ anthropic.rs   AnthropicChat: ChatModel over integrations::anthropic
│  └─ classifier.rs  LlmClassifier: TextClassifier with structured output
├─ plan_writer.rs    PlanWriter, PlanInput<'k>, SharedMemories<'k>, WrittenPlan
├─ context.rs        context::load, AnchorData, TurnContext, borrowed views
├─ tools.rs          Tool, DynTool, Erased<T>, ToolCtx<'t>, ToolRegistry
├─ tools/
│  ├─ read.rs        the fourteen read tools
│  ├─ ui.rs          request_rescan, remember
│  ├─ write.rs       create_task, complete_task, propose()
│  └─ revise.rs      revise_plan, apply_preview, preview_neighbours
├─ turn.rs           ChatAgent::start_turn and the turn task
└─ memory.rs         after_turn (gate, extraction), summarise_if_due
```

Each `foo.rs` with tests too long to sit at its end has `foo/tests.rs` (`safety/plan`, `plan_writer`, `tools`, `turn`, `beliefs`, `context`). `fixtures.rs` and `testing.rs` are test-only.

---

## 4. The dose check

[Contracts 7, the dose check](../architecture/contracts.md#actionplan) defines it; this section is that definition as code. It runs on every plan text, every assistant sentence before it is streamed, and every string the model writes into a tool call.

### The patterns

Every pattern is a constant, compiled once into a `LazyLock`, and matched case-insensitively. Units are followed by `\b`, so "2 lowest leaves" and "3 large leaves" never read as litres.

@@items safety/dose.rs QUANTITY RATIO_QUANTITY MEASURE EVERYDAY OF_WITHIN_THREE@@

| Contract rule | `DoseRule` | Pattern | Condition |
|---|---|---|---|
| A quantity followed by a measure | `Measure` | `QUANTITY \s*-?\s* MEASURE \b` | none |
| `drop`, `cup`, `spoonful`, `capful`, `pinch` with `of` within three words | `EverydayMeasure` | `QUANTITY \s*-?\s* EVERYDAY \b OF_WITHIN_THREE` | none |
| "a few drops", "a splash", "a squirt" with `of` within three words | `VagueMeasure` | `\ba\s+(?:few\s+drops\|splash\|squirt)\b OF_WITHIN_THREE` | none |
| A rate | `Rate` | `\bper\s+(?:litre\|liter\|gallon)s?\b \| /\s*(?:l\|gal)\b` | none |
| Two numbers around a colon | `ColonRatio` | `\d+\s*:\s*\d+` | none |
| "<q> part(s) … to <q> part(s)" | `PartsRatio` | `RATIO_QUANTITY\s+parts?\b.*?\bto\s+RATIO_QUANTITY\s+parts?\b` | none |
| "<q> part(s) … to <q>", "<q> in <q>" | `MixtureRatio` | `…parts?\b.*?\bto\s+RATIO_QUANTITY`, `RATIO_QUANTITY\s+in\s+RATIO_QUANTITY` | a product name or a mixture word in the sentence |
| A percentage | `Percentage` | `\d+(?:[.,]\d+)?\s*(?:%\|percent\b\|per\s+cent\b)` | a product name or a mixture word in the sentence |

@@items safety/dose.rs DoseRule@@

@@items safety/dose.rs patterns RULES ANY MIXTURE@@

The mixture words of contracts 7 are matched with their inflections (`sprayed`, `mixing`, `diluted`, `dilution`), so a rewording doesn't slip past the condition. A product name is anything in the [`ProductCatalogue`](#5-sentences-and-products).

### The check

The text is split into sentences ([section 5](#5-sentences-and-products)); each is first tested against the `RegexSet` of every rule, so a sentence with nothing suspicious (almost all of them) costs one pass. Only the rules the set reports are run on their own, the mixture condition is evaluated at most once per sentence, and the earliest match wins.

@@items safety/dose.rs DoseMatch dose_check dose_check_all check_sentence@@

`DoseMatch<'t>` borrows the matched words and their sentence from the checked text, so a caller can name the words in the reason it gives the model ("\"2 tbsp\" is not allowed because it gives an amount…") without copying. `reason()` never repeats the whole sentence. `to_owned_match()` gives an `OwnedDoseMatch` for a value that must outlive the text.

### Test table

Every example of contracts 7, failing and passing, is a test case (`safety::dose::tests`), plus one case per measure and rate:

| Text | Result |
|---|---|
| "Mix 2 tbsp into the watering can.", "Use 0.5 l of water.", "Add 1/2 tsp.", "Add ¼ teaspoon.", "Add 1 1/2 tablespoons.", "Use half a litre.", "Use an ounce.", "Dissolve 10ml in water." | fails: `Measure` |
| "a drop of soap", "two cups of water", "a spoonful of sugar", "a capful of bleach", "a pinch of baking soda", "a cup or so of milk", "Add ½ cup of soap." | fails: `EverydayMeasure` |
| "Add a few drops of dish soap.", "Add a splash of vinegar.", "a squirt of soap" | fails: `VagueMeasure` |
| "Use it per litre of water.", "per liter", "per gallon", "Apply 2/l.", "dilute to /gal" | fails: `Rate` ("5 g/l" and "5 ml per litre" fail earlier, as `Measure`) |
| "Mix at 1:10." | fails: `ColonRatio` |
| "Use one part soap to ten parts water.", "Use 1 part milk to 9 parts water." | fails: `PartsRatio` |
| "Mix 1 in 10 with water.", "One part neem to ten of water." | fails: `MixtureRatio` |
| "Use a 2% soap spray.", "A 0.5 percent copper solution." | fails: `Percentage` |
| "Remove the 2 lowest leaves.", "Pick off 3 large leaves.", "Expect a drop in temperature tonight.", "The leaves curl into a cup.", "Cut a part of the stem back to a healthy bud.", "Plant one in a pot.", "Remove one in three canes.", "More than 50% of the leaves are spotted.", "Above 90% humidity, the fungus spreads.", "Check again in 2 weeks.", "Look for new spots every 3 days.", "Space them 10 cm apart.", "Mulch 30 cm deep around the stem.", "The spots are about 2 mm across." | passes |
| "Use a 1 gallon sprayer." / "Water at 10:30 in the morning." / "Spots cover 12% of the leaf, so spray the healthy leaves too." | fails: the three **known false positives** (`Measure`, `ColonRatio`, `Percentage`) |
| "Spots cover 12% of the leaf. Spray the healthy leaves too." | passes: the condition is per sentence, which is how the prompts tell the model to write a measured share |

@@test safety/dose.rs contract_examples_pass@@

---

## 5. Sentences and products

### Sentence splitting

A sentence ends after `.`, `!`, `?` or `…` (and any closing quote or bracket) followed by whitespace, or at a line break. A full stop does not end a sentence after a common abbreviation ("e.g.") or a list number at the start of a line ("1. Remove…"); a decimal point is never followed by whitespace, so "0.5" never splits. The same boundary function serves the complete texts of the dose check (`sentence_spans`, trimmed ranges into the text) and the stream.

@@items safety/sentences.rs sentence_spans SentenceSplitter@@

@@impl safety/sentences.rs impl SentenceSplitter@@

`SentenceSplitter` is **owned and lifetime-free**: `push` copies each `text_delta` into its buffer and returns owned `String`s. It lives in the turn task across `.await`s between model chunks, and its sentences move into events sent over a channel, so nothing in it may borrow from a chunk dropped at the next `.await`. A full stop at the very end of the buffer is undecided (the next delta may be "5" of "0.5"), so the splitter waits for one more character. Pieces keep the whitespace that followed them, so the streamed pieces concatenate back into exactly the model's text (a test streams a reply one character at a time and checks this). After 2,000 bytes without a boundary the buffer is released as one piece, still checked whole, so a run-on reply can't stall the stream.

### The product list

[Contracts 7](../architecture/contracts.md#actionplan) names products that make a step chemical ("pesticide, fungicide, insecticide, herbicide, copper, sulfur, neem, insecticidal soap, horticultural oil, Bt, spinosad, pyrethrin, and the list in the knowledge base"). `ProductCatalogue` compiles those words, the British spelling `sulphur`, and every name in `products.yaml` into one case-insensitive alternation (longest first, plurals allowed). It is built once at start-up from `KnowledgeStore::products()` and shared as `Arc<ProductCatalogue>`.

@@items safety/products.rs BUILTIN_PRODUCTS ProductMention ProductCatalogue@@

@@impl safety/products.rs impl ProductCatalogue@@

`mentions<'c, 't>` returns `impl Iterator + use<'c, 't>`: the iterator borrows both the catalogue (`'c`: the regex and the lookup table) and the text (`'t`: each `ProductMention` points into it). Edition 2024 would capture both anyway ([Edition Guide, RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html)); the bound says so in the signature.

---

## 6. Plan validation

Every rule of the [contracts 7 plan-validation table](../architecture/contracts.md#actionplan), over a `PlanDraft` and the knowledge the plan may use. Validation never fails as a whole: it returns the steps that pass, with their category fixed and the safety note added, and one `RuleFailure` per broken rule. The plan writer sends the failures back once and then keeps the passing steps; `revise_plan` returns them to the model as the tool result.

@@items safety/plan.rs PlanDraft ValidationContext PlanField PlanRule RuleFailure PlanValidation validate_plan@@

| Contract rule | How | `PlanRule` |
|---|---|---|
| No doses | `dose_check` on `summary`, each `watch_for` item, and every step's `title` and `detail`, whatever the category. A failing step is dropped, a failing `watch_for` item is dropped, a failing summary is replaced by `FALLBACK_SUMMARY` | `Dose` |
| The category comes from the knowledge base | A step with `treatment_id` takes `TreatmentSeed.kind` (via `From<TreatmentKind> for StepCategory`); an unknown treatment fails | `UnknownTreatment` |
| Products make a step chemical | Any `ProductCatalogue` mention in `title` or `detail` sets `chemical`, after the treatment's category | — |
| Chemical steps carry the safety note | Every passing `chemical` step gets `domain::records::plan::SAFETY_NOTE`; the draft type has no `safety_note` field, so the model can't write one | — |
| Beneficials are never targeted | A target organism whose profile role is `beneficial` fails; an organism with no profile (an `other` sighting, role `unknown`) can't be targeted either | `TargetsBeneficial`, `UnknownTarget` |
| Beneficials are not harmed by the way | With `beneficials_seen`, a `chemical` step naming a product marked `broad_spectrum` fails, asking for a targeted option | `BroadSpectrum` |
| Every step is cited | At least one citation, each in `known_sources`; the plan's `citations` are the passing steps' sources, deduplicated in first-cited order | `Uncited`, `UnknownSource` |
| Neighbour steps | `plant_id` must be one of `neighbours`, the step `monitoring` (after re-categorising), and a target one of the neighbour's `problems` or an organism one of them is `caused_by` | `UnknownNeighbour`, `NeighbourNotMonitoring`, `NeighbourTarget` |
| (structural) | Step ids unique; `recurring.every_days` at least 1 | `DuplicateStep`, `InvalidWhen` |

Passing steps are renumbered `order` 1, 2, … in their original order. `ValidationContext<'k>` borrows its slices for one validation (`'k` is the lifetime of the knowledge the caller loaded); the private `Index<'k>` builds hash maps over those same borrows, so no profile is copied.

---

## 7. Router and safety gate

[Architecture 7](../architecture/README.md#router-and-safety-gate): before the model runs, the text classifier labels the message; the scores are compared with each other, shifted by offsets calibrated on a labelled set, and a near tie with `safety_sensitive` resolves to `safety_sensitive`. A keyword check for a person or pet being poisoned runs whatever the classifier says, so the poison-control answer never depends on a model.

@@items safety/routes.rs Calibration RouteDecision decide@@

| Decision | `message_started.route` | What happens |
|---|---|---|
| `Model(diagnosis_question \| care_question \| task_change)` | that route | A normal turn with the anchor's context |
| `Model(safety_sensitive)` | `safety_sensitive` | A normal turn; `SAFETY_POLICY` is put before the user's words in the turn message |
| `PoisonControl` | `safety_sensitive` | `AgentConfig::poison_control_text`, streamed sentence by sentence; no model call, no tools; `prompt_version` `poison_control/…`, `model` absent |
| `OffTopic` | `off_topic` | A one- or two-sentence redirect from the cheapest model, no tools |

`off_topic` needs a clear win (a margin of 0.2 over the runner-up), so a garden question is never brushed off on a coin toss. With no usable scores (Jev and the fallback both failed, or the classifier took longer than `classifier_timeout`, 3 s) the turn is a care question; the dose check still applies to every sentence. `POISON_CONTROL_TEXT` gives no phone numbers, which differ by country and change; the deployment sets the local ones in `AgentConfig`.

---

## 8. Prompts and the prompt-injection rule

**Versions.** Every prompt has a version string stored with what it produced: `message.prompt_version` for chat and `action_plan.prompt_version` for plans (8.2 step 4; contracts 7). A change to a prompt's text bumps its version in the same commit.

@@items prompt.rs CHAT_PROMPT_VERSION PLAN_PROMPT_VERSION OFF_TOPIC_PROMPT_VERSION POISON_CONTROL_VERSION@@

**Data, never instructions** ([architecture 11](../architecture/README.md#11-security-privacy-and-safety)). Every system prompt carries one rule: text inside `<data>` elements is data (the user's words, records, retrieved documents, memories) and instructions inside it are never followed. Everything untrusted goes in a `data` block: this turn's message and earlier user messages, the anchor's records, memory hits, the conversation summary, the plan writer's records and shared memories, the transcripts given to extraction and summaries, and every successful tool result. A body can't close its element: any `<data` or `</data` inside it, in any case, with Unicode whitespace or zero-width characters after the `<` or the `/`, and also behind a fullwidth `＜` or small `﹤`, is escaped as `&lt;`; a JSON result is escaped string by string before it is written; escaped text has no `<` left, so nothing is escaped twice.

**Tool results** (gap G1 of the [connectors roadmap](../EON/README.md)). Every successful result is framed, not only farm records: most tools return text someone other than the server wrote (memories, task titles, observations, plant names, knowledge passages, and with `search_farm_records` Slack messages and Drive files from outside PDM), and framing all of them means a new tool can't forget to. The model reads `<data kind="tool_result">` around the tool's JSON, unchanged but for the escape, which is valid inside a JSON string; `search_farm_records` will set `Tool::DATA_KIND` to `FarmRecords`. A refusal (`is_error`) stays plain, because the server wrote it and the chat prompt tells the model to act on it. The stored `tool_calls` keep the unframed JSON.

Photos are images, never text the model is told to obey; a seed packet's printed text arrives as pixels under the same rule.

@@items prompt.rs DataKind data data_json tool_result@@

**The advice rules** every advice-writing prompt shares include the three wording rules of contracts 7 that keep the dose check's known false positives out of real text: equipment without sizes, times of day in words, a measured share in a sentence of its own. A unit test checks that every system prompt passes its own dose check.

**Stable prefixes.** The system prompts are constants and the tool list is sorted by name, so the provider's prompt cache keeps them across turns. Everything that changes per turn goes in the turn's last user message ([section 11](#11-context-per-anchor)).

---

## 9. Model adapters

### `AnthropicChat`

`ChatModel` over `integrations::anthropic::AnthropicClient`. The client's `ModelIds` picks the model per `ChatPurpose`: `claude-opus-5-5` for chat, the plan writer and the vision fallback, `claude-haiku-5-5` for extraction, summaries, off-topic redirects and classification, all configurable ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)).

@@items model/anthropic.rs AnthropicChat purpose_label effort_for@@

@@impl model/anthropic.rs impl AnthropicChat {@@

@@impl model/anthropic.rs impl ChatModel for AnthropicChat@@

How it follows the Messages API (research notes, section 4; the `claude-api` skill):

| Rule | Here |
|---|---|
| Thinking can't be disabled on Opus 5.5; `budget_tokens` is a 400 | `thinking` is never sent (adaptive by default); depth is `output_config.effort` per purpose: chat `medium`, plan writer `high`, cheap calls `low` |
| Forced `tool_choice` (`any`, `tool`) is a 400 on Opus 5.5 | Never sent; the plan writer uses `output_config.format` |
| Strict tools share one compiled grammar per request; nineteen were refused live ("The compiled grammar is too large", 2026-10-16) | Only `ToolKind::Write` tools set `ToolSpec::strict` (`create_task`, `complete_task`, `revise_plan`); the others' arguments are parsed into their typed structs and a bad call is refused with the reason |
| Structured-output schemas reject `minimum`, `maxLength`, `maxItems`, `oneOf` … and need `additionalProperties: false` | Every tool schema and response schema goes through `integrations::anthropic::sanitize_schema` |
| Thinking blocks of the current tool loop must be replayed unchanged | `ChatChunk::Reasoning(ReasoningBlock)` carries each complete block; the turn replays it in place ([section 13](#13-the-chat-turn)) |
| Tool results for parallel calls go in one user message; `is_error` marks a failed call | Domain `ChatRole::Tool` messages map to one user message; `ContentPart::ToolResult.is_error` maps to `is_error` |
| `stop_reason: "refusal"` arrives with HTTP 200 | `StopReason::Refusal`; the turn ends with a fixed sentence and runs no tool of that call |
| A clean close mid-answer ends the byte stream without an error | A stream without a stop reason ends with `ModelError::Unavailable`, so the turn ends with an `error` event instead of storing a cut-off answer as complete |
| Prompt caching: tools render before the system prompt | One `cache_control` breakpoint on the system block caches both; chat adds top-level automatic caching for the growing tool loop |

`stream<'a>` builds a `MessagesRequest<'a>` that borrows the domain request (text, tool schemas, image bytes as `Cow::Borrowed`), lets the client serialise it, and drops it; the client's `MessageStream` is `'static`, so the only borrows the returned `BoxStream<'a, …>` holds are the two the signature names. The client's `MessageAccumulator` assembles tool-call JSON and thinking blocks; each `Output` maps to one `ChatChunk`.

### `LlmClassifier`

`TextClassifier`'s fallback when Jev isn't configured ([architecture 10](../architecture/README.md#10-models-and-gpu-placement)): one cheap-model call with a strict schema of one number per label. A refusal counts as the safe label scoring 1 (`safety_sensitive` for routing, `skip` for the memory gate). `api` wires `integrations::jev::JevClassifier` when `JEV_*` is set and this otherwise.

@@items model/classifier.rs task_labels score_schema LlmClassifier parse_scores@@

---

## 10. The plan writer

[Architecture 8.1 step 8](../architecture/README.md#81-scan--diagnosis--plan): the strongest model writes `ActionPlan`'s model-written fields as strict JSON, plan validation checks them, the failures go back once, and the passing steps are returned. It stores nothing.

@@items plan_writer.rs SharedMemories PlanInput WrittenPlan PlanWriter@@

@@impl plan_writer.rs impl PlanWriter {@@

**Calls.** One call. If the answer doesn't parse (or stopped at `max_tokens`), one retry with the parse error; a second failure is `PlanWriterError::Output`. Then validation; if it isn't clean, one more call with `retry_message()`, appended to the same conversation (append-only), and the second answer's passing steps are kept. If the second answer doesn't parse, the first validation stands. A refusal is `PlanWriterError::Refused`.

**The context** ([contracts 7](../architecture/contracts.md#actionplan)), serialised once from borrowed views into one `context` data block:

| Input | Source (built by `pipeline`) |
|---|---|
| The diagnosis: status, primary, candidates, organisms seen with roles, risks | `PlanInput::diagnosis` |
| The problems the plan is about, with conditions, treatments, `caused_by`, sources | primary, or every candidate scored 0 or more when uncertain; none when healthy |
| Treatments with their kind and text | the problems' treatments |
| Organisms with their roles | those seen and those a step may target |
| Constraints carried forward | the current plan's `constraints` ("No copper products") |
| Neighbours, nearest first | the contracts 7 rule; `pipeline` computes them |
| The open question | a provisional plan names it |
| The forecast | the scan's weather |
| Shared memories, in their own `memories` block | `MemoryStore::shared_for_plan` through `SharedMemories::new` |
| The plant's and its garden's current beliefs, in their own `beliefs` block (`key`, `value`, `about_plant` only), as standing facts and constraints | `PlanInput::beliefs`, from `beliefs::current_beliefs` with the plan step's timeout; empty when Agent Memory is off or slow |

**Shared memories only, by type.** `SharedMemories<'k>` has one constructor, which fails with `PrivateMemory` if any memory is a chat memory or unshared. A private chat memory therefore can't reach a prompt whose output every member sees (architecture 7, memory). The beliefs keep the rule by construction: Agent Memory only ever receives shared memories ([section 14](#beliefs-in-agent-memory)). The plan prompt says a belief wins over an older memory that disagrees.

**Signatures agreed with `pipeline`.** `pipeline` calls only `PlanWriter::write`, through its own `#[async_trait] PlanAuthor` trait for tests; the future is `Send` (no guard lives across an `.await`). It resolves `WrittenPlan::citations` with `KnowledgeStore::citations` and assembles the `ActionPlan` (ids, version, `supersedes`, `provisional`, `open_question`, constraints, tasks, trigger rules).

---

## 11. Context per anchor

[Architecture 7, conversation anchors](../architecture/README.md#conversation-anchors): the context is **rebuilt from the store on every turn**: the conversation summary, the most recent messages and the top memory hits, never the full transcript, plus the anchor's current records.

@@items context.rs AnchorData PlantData GardenData TurnContext load@@

| Anchor | Records loaded (owned) | Port calls |
|---|---|---|
| `scan` | `ScanDetail<Stored>`: the scan, its images (ids and kinds), observations with regions, diagnosis, plan, open question, weather | `ScanRepo::scan_detail` |
| `plant` | The plant, its scans (timeline, newest first), its open tasks, its growth curve, the garden's weather | `PlantRepo::plant`, `timeline`, `growth_series`; `PlanRepo::tasks` (filtered to the plant); `garden_weather` |
| `garden` | The garden's name, every plant's summary, upcoming tasks, weather and forecast, the map's beds and neighbours | `GardenRepo::garden`, `PlantRepo::plants`, `PlanRepo::tasks`, `garden_weather`, `MapRepo::map` |
| all | The summary and up to `recent_messages` (12) messages, text only; memory hits; the current beliefs of the plant and its garden (of the garden for a garden anchor) | `ChatRepo::context`; `MemoryStore::search` with the message's embedding, within the plant when there is one; `beliefs::current_beliefs`, run beside the search with `tokio::join!` and bounded by `beliefs_read_timeout` (1.5 s) |

**Memory privacy.** `MemoryStore::search` takes the session's membership and applies `shared = true OR author = $user` within the garden ([database 5](../database/README.md#5-reads-indexes-and-vector-search)), so a member's context holds shared memories and their own chat memories, never another member's.

**Beliefs.** The current beliefs are the latest shared facts, one per key. `turn_message` renders them as their own `beliefs` block, before the memories, with only `key`, `value` and `about_plant`; the chat prompt says a belief wins over an older memory hit that disagrees. Agent Memory off, failing or slower than the timeout leaves the block out; the turn never waits longer.

**Where each part goes.** The summary and recent messages form the stable start of the conversation (earlier user messages wrapped as `user_message` data). The anchor's records, the current beliefs, the memory hits, the user's images and the user's words go in the turn's last user message, each untrusted part in its own block. Rendering is synchronous, through borrowed views (`ScanView<'a>`, `PlantView<'a>`, `GardenView<'a>`, `'a` being the `AnchorData` being rendered), each record serialised once. The garden's location never goes to the model: `GardenView` carries the name only.

@@impl context.rs impl TurnContext@@

`garden_weather` reads the weather cache for the hour and falls back to Open-Meteo, caching the result; a failure leaves the weather out rather than failing the turn.

**Why text-only history.** The provider binds each thinking block to the exact conversation before it, and editing earlier turns invalidates every later block (the `claude-api` skill, preserved thinking). The context is rebuilt every turn by design, so earlier turns' thinking blocks are never replayed (dropping the leading run of blocks is allowed); thinking blocks live only within one turn's tool loop, where the conversation is append-only. Earlier turns' tool calls and results are stored but not replayed either: the anchor's records are reloaded fresh, which is what architecture 7 asks for.

---

## 12. Tools

### Typed tools, erased once

Each tool is a type implementing `Tool`, with an associated `Args` type: `serde` parses the model's JSON into it, `schemars` derives the JSON Schema the model is shown from it, and `run` receives it typed. `Tool` has associated constants and returns `impl Future + Send`, so it is **not dyn compatible** ([Reference, dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility)); it is used with static dispatch only. The registry needs one list of different tools, so `Erased<T>` wraps each tool and implements the dyn-compatible `DynTool`, whose `call` takes `serde_json::Value` and returns a boxed future. The JSON is parsed into `T::Args` inside the box, where the type is known again ([The Book, 18.2: trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)).

@@items tools.rs ToolKind ToolCtx ToolEffect ToolOutput Tool DynTool Erased@@

@@impl tools.rs impl<T: Tool> DynTool for Erased<T>@@

**The lifetimes of the erasure.**

- `ToolCtx<'t>`: `'t` is the turn. The turn task owns `Arc<AgentDeps>` and lends `&'t AgentDeps` to every call. The struct is covariant in `'t`, so `&'a ToolCtx<'t>` coerces to `&'a ToolCtx<'a>` wherever a shorter borrow is asked for.
- `DynTool::call<'a>(&'a self, ctx: &'a ToolCtx<'a>, arguments: Value) -> BoxFuture<'a, …>`: one lifetime for the tool, the context and the context's own borrows; the boxed future borrows all three and must not outlive any. It is named because a trait-object return type needs an explicit bound (`Pin<Box<dyn Future + Send + 'a>>`), which elision can't supply.
- `Tool::model_strings(&Self::Args) -> Vec<&str>`: the returned strings borrow the arguments (elided `'_`); the borrow ends after the dose check, before `run` takes `args` by value.
- `Tool::run(&self, ctx: &ToolCtx<'_>, args) -> impl Future + Send`: under edition 2024 the future captures `self`, `ctx` and the context's lifetime; it is `Send` so the turn task stays `Send`. The implementations are written as `async fn`.
- `T::Args: DeserializeOwned + JsonSchema + Send + 'static`: parsed from an owned `Value` that is dropped at once, so nothing borrows from it.

**Dose check before running.** `Erased::call` checks every string of `model_strings` before `run`, so a failing call creates no card, task, memory or event, and the model gets the reason as an `is_error` result ([contracts 7](../architecture/contracts.md#actionplan): "a failing call is rejected and the model gets the reason as the tool result"). Unknown fields in arguments are rejected the same way (`deny_unknown_fields`).

### The registry and roles

@@items tools.rs ToolRegistry@@

@@impl tools.rs impl ToolRegistry {@@

Viewers and reviewers are offered the read tools, `request_rescan` and `remember`; editors and owners all nineteen ([contracts 1, roles and access](../architecture/contracts.md#roles-and-access)). A call to a tool the role isn't offered is answered like an unknown tool. The list is sorted by name and identical for every turn of a role, which keeps the provider's prompt cache.

### Every tool

| Tool | `Args` | Scope (as the session user) | Returns ([contracts 8](../architecture/contracts.md#tool-return-shapes-and-scope)) | Kind | Roles |
|---|---|---|---|---|---|
| `get_scan` | `GetScanArgs { scan_id }` | `ScanRepo::scan_detail(member, …)`: scans of the garden | `ScanDetail<Stored>`: ids, no URLs | read | all |
| `view_image` | `ViewImageArgs { image_id, scan_id?, region? }` | with `scan_id`: the garden's scan and an image of it; without: `ChatRepo::chat_image_object` of this conversation | the image (or a PNG crop) to the model; `{ ok: true }` | read | all |
| `get_forecast` | `GetForecastArgs { days }` (1–14) | the conversation's garden | `ForecastDay[]` | read | all |
| `search_memory` | `SearchMemoryArgs { query, plant_id? }` | `MemoryStore::search(member, …)`: shared + own; beside it `BeliefStore::search` within the garden's scope, bounded by `beliefs_search_timeout` (4 s) | `MemorySearch { memories, beliefs }`: `beliefs` empty when Agent Memory is off, failing or slow | read | all |
| `find_similar_cases` | `FindSimilarCasesArgs { image_id }` | `AnalysisRepo::similar_cases(member, …, 5)` | `SimilarCase[]`, at most 5 | read | all |
| `search_knowledge` | `SearchKnowledgeArgs { query, topic? }` | knowledge base | `{ text, citation }[]`; a `citation` event per source | read | all |
| `get_species_profile` | `GetSpeciesProfileArgs { species }` | knowledge base | `{ profile, susceptible_to }`; citations | read | all |
| `get_disease_profile` | `GetDiseaseProfileArgs { disease }` | knowledge base | `{ profile, treatments }`; citations | read | all |
| `compare_lookalikes` | `GetDiseaseProfileArgs { disease }` | knowledge base | `{ problem_id, name, how_to_tell }[]` | read | all |
| `get_organism_profile` | `GetOrganismProfileArgs { organism }` | knowledge base | `{ profile, preys_on }`; citations | read | all |
| `get_soil_profile` | `NoArgs` | the conversation's garden | `SoilProfileView { soil_profile, latest, texture? }` | read | all |
| `get_plant_measurements` | `GetPlantMeasurementsArgs { plant_id, quantity? }` | `PlantRepo::growth_series(member, …)`: plants of the garden | `MeasurementPoint[]`, the latest 50 | read | all |
| `get_garden_map` | `NoArgs` | the conversation's garden | `GardenMapView { map, neighbours }` | read | all |
| `list_tasks` | `ListTasksArgs { state? }` (default open) | the garden | `Task[]` | read | all |
| `request_rescan` | `RequestRescanArgs { plant_id, photo_kind, instructions }` | a live plant of the garden | `RescanRequest`; a `rescan_requested` event | ui | all |
| `remember` | `RememberArgs { fact, plant_id?, topic? }` (topic default `other`) | the caller's own memory (`MemoryStore::remember(member, …)`) | `Memory` (`source: chat`); a `memory_saved` event | memory | all |
| `create_task` | `CreateTaskArgs { title, detail?, plant_id?, due?, repeat_every_days? }` | `require::<Editor>()`; a live plant | `PendingAction` (preview `task`); a `pending_action` event | write | editor, owner |
| `complete_task` | `CompleteTaskArgs { task_id }` | an open task of the garden | `PendingAction` (preview `task` with `done_at`) | write | editor, owner |
| `revise_plan` | `RevisePlanArgs { scan_id, summary, changed_steps, removed_step_ids, constraints }` | the scan's current plan | `PendingAction` (preview `plan`), only after full plan validation | write | editor, owner |

Dose-checked strings: `request_rescan.instructions`, `remember.fact`, `create_task.title` and `detail`, and `revise_plan`'s summary, constraints and every changed step's title and detail. `complete_task` has no model-written string: its card is written from the stored task. The card labels ("Add a task?", "Add task", "Not now", "This changes the plan for everyone in the garden.") are the server's.

@@items tools/write.rs propose@@

### `revise_plan` and `apply_preview`

The model proposes changes against the scan's **current** plan. The tool merges them into the base plan's steps (as drafts, so categories and notes are recomputed), validates the **whole** plan the change would produce against the knowledge of the plan's problems, the diagnosis's organisms and the neighbours from the **current** map, and only then stores the card. A failing preview goes back to the model with every reason, never trimmed silently, so the card shows exactly what confirming stores ([contracts 7, re-planning](../architecture/contracts.md#actionplan)). The preview's `changed_steps` are every validated step that differs from the base plan's step with the same id (content, category, note or `order`), plus new ones.

@@items tools/revise.rs RevisePlanArgs plan_problems preview_neighbours RevisedPlan PreviewError apply_preview@@

`api` calls `apply_preview` on confirm, after `ChatRepo::pending_action`: it returns the plan to store (`summary`, steps in order, `watch_for`, constraints with the card's id, citation ids) or `PreviewError::BaseMismatch`, which `api` answers with `409 invalid_state` and `current_plan_id`. The store's `write_revision` keeps the compare-and-set and the "named neighbour still exists" check.

---

## 13. The chat turn

### The loop

```mermaid
sequenceDiagram
    autonumber
    participant App
    participant Api as api handler
    participant Agent as ChatAgent::start_turn
    participant Task as turn task
    participant Store as store and memory ports
    participant Model as ChatModel
    participant Tools as ToolRegistry
    App->>Api: POST /conversations/{id}/messages
    Api->>Api: check_conversation::<Viewer> (404 for anyone else)
    Api->>Agent: start_turn(TurnRequest)
    Agent->>Agent: validate (2,000 chars, 3 images)
    Agent->>Store: UsageRepo::usage (daily budget)
    Agent->>Store: conversation anchor, chat image objects
    Agent->>Agent: classify (3 s timeout), decide_route
    Agent->>Store: store the user message (with its route)
    Agent-->>Api: TurnStream, or TurnError as an HTTP error
    Agent->>Task: spawn on the TaskTracker
    Task-->>App: message_started
    Task->>Store: embed the message, context::load
    loop at most max_model_calls (6)
        Task->>Model: stream(&ChatRequest)
        Model-->>Task: TextDelta, Reasoning, ToolCall, Usage, Done
        Task-->>App: text_delta, one dose-checked sentence at a time
        opt stop_reason tool_use
            Task-->>App: tool_started
            Task->>Tools: DynTool::call (parse, dose check, run as the session user)
            Tools-->>Task: ToolOutput (content, images, effects)
            Task-->>App: citation, pending_action, memory_saved, rescan_requested
            Task-->>App: tool_finished
            Task->>Task: append the assistant turn unchanged, then the results
        end
    end
    Task->>Store: store the reply, add the usage
    Task->>Store: memory gate, extraction, private chat memory
    Task-->>App: memory_saved (if any), message_completed
    Task->>Store: summarise if due (after the stream closed)
```

@@items turn.rs ChatAgent TurnRequest TurnOutcome TurnStream@@

@@impl turn.rs impl ChatAgent {@@

### Before the stream

Errors before the stream starts are ordinary HTTP errors ([contracts 8, sending a message](../architecture/contracts.md#sending-a-message)): `TurnError::Domain` with `invalid_request` (empty text, more than 2,000 characters, more than 3 images, an image of another conversation) or `budget_exceeded` (`budget: chat_tokens`, `resets_at` the next midnight UTC, as `usage_daily` counts per UTC day), and `TurnError::Store`. The user's message is stored with its route before the stream exists, so an interrupted turn still has its question.

### The stream

- **Per-sentence dose check.** Text deltas go through a `SentenceSplitter`; each complete sentence is dose-checked and a failing one is replaced by the fixed safety note, keeping its surrounding whitespace, before its `text_delta` is sent. At the end of every model call the splitter is flushed, so text before a tool call ("Let me check the forecast") is checked and sent too. The stored message text is exactly what was streamed.
- **Tool use.** On `stop_reason: tool_use` the tools run in order, each between `tool_started` (with the tool's UI label) and `tool_finished { ok }`. The assistant turn goes back into the next request **exactly as the model wrote it** (its raw text, its reasoning blocks in place, its tool calls), then one `Tool` message with every result and any images after them. The raw text is replayed, not the checked one: editing the assistant turn would invalidate the thinking blocks that follow it. A unit test checks that the second request is the first plus those two messages, byte for byte.
- **The cap.** At most `max_model_calls` (6) calls; a turn that still wants tools after the last ends with `LOOP_CAP_TEXT`. A refusal ends with `REFUSAL_TEXT` and runs no tool of that call.
- **Effects** become events: `citation` (deduplicated by source, and stored on the message), `pending_action`, `memory_saved`, `rescan_requested`.
- **The end.** The reply is stored with its model, prompt version, token counts, citations and tool calls and results (never sent to the app), and the usage is added to the day's total. On a completed turn the memory gate runs ([section 14](#14-memory)), then `message_completed { message_id, usage }`.

### Token budget

The day's total is read before the stream (`budget_exceeded` if it is used up). During the turn, the tokens of every call are added up; before each further call, a turn that has spent what was left ends with `BUDGET_TEXT`. Usage within one call is cumulative (the last `Usage` chunk wins); across calls it is summed. A turn can overrun the budget by at most one call.

### Cancellation

The handler's stream is the receiving half of an `mpsc` channel (32 events). When the client disconnects, hyper drops the body, which drops the receiver. The task notices at its next `send` (it fails) or while it waits for the model (`select!` on `Sender::closed()`, `CancellationToken::cancelled()` for shutdown and the model stream's `next()`, all cancel-safe), drops the model stream, which stops the HTTP call, and stores the message with `interrupted: true`. A drop guard can't do that last part, since storing needs an `.await`; the task can. `TurnOutcome` reaches `api` through a `oneshot`: a completed message is stored under the `Idempotency-Key` and replayed as `200 { message }`; an interrupted or failed turn releases the key ([contracts 1, idempotency](../architecture/contracts.md#idempotency)).

### Errors after the stream started

A model failure ends the stream with an `error` event: `upstream_unavailable` for a provider that is down, overloaded or too slow (including a transport timeout), `internal` otherwise, each with its code's default `retryable` from contracts 2. The partial reply is stored with `interrupted: true`.

---

## 14. Memory

[Architecture 7, memory](../architecture/README.md#memory) and 8.2 step 4.

@@items memory.rs ExtractedMemory after_turn summary_due summarise_if_due@@

**Extraction.** After a completed model turn: the classifier's `MemoryWorthy` task scores the exchange (`remember` must beat `skip` by `memory_gate_margin`); only then the cheap model extracts at most one fact with a strict schema; a fact that gives a dose is dropped; the fact is embedded in document mode and stored with `MemoryStore::remember(member, …)` as a **private chat memory** of the turn's author, linked to the assistant message, about the conversation's plant or (when the model says it is garden-wide) the whole garden. `memory_saved` is sent before `message_completed`, and the app shows it with Undo and Share with garden.

**Privacy.**

| Who reads a chat memory | How it is enforced |
|---|---|
| Its author's later conversations | `MemoryStore::search(member, …)` returns `shared = true OR author = $user` within the garden |
| Other members | Never, until shared (`POST /memories/{id}/share` turns it into a `user` memory) |
| The plan writer | Never: `SharedMemories::new` refuses any chat or unshared memory ([section 10](#10-the-plan-writer)) |
| The model's own tools | `search_memory` and `remember` go through the same membership proof |

**Topics.** Every memory has a `MemoryTopic` (watering, feeding, treatment, pruning, planting, location, container, soil, light, health, harvest, other). The extraction schema carries it as `ExtractedTopic`, a copy with a `JsonSchema` derive (the domain type derives only `ToSchema`), and the prompt asks for `other` whenever a fact doesn't clearly belong to one topic or should stand beside earlier facts, because a newer fact on the same topic supersedes the older one in Agent Memory. `remember` takes an optional `topic`, `other` when absent.

**Summaries.** After the stream has closed, `summarise_if_due` asks `ChatRepo::message_count` and, when the count has just crossed a multiple of `summarise_every` (20), summarises the previous summary plus the last messages with the cheap model and stores it with `set_summary`. A summary that gives a dose is not stored.

### Beliefs in Agent Memory

[Architecture 7, memory](../architecture/README.md#memory): every **shared** memory (`pipeline` or `user`) is recorded in `SurrealDB` Agent Memory as one triple through the `BeliefStore` port ([domain](domain.md)); the `memory` table stays the system of record, and a private chat memory never leaves the database.

@@items beliefs.rs SweepReport sync_pending current_beliefs search_beliefs@@

**One writer.** No request waits on Agent Memory, and nothing records a memory while its subject is being erased, because only `sync_pending` calls it. `api`'s scheduler runs it every 10 s with a batch of 50. Requests only write rows ([store](store.md)):

| Request | What it writes |
|---|---|
| A garden note, a pipeline summary, a share | The row, `belief_synced_at` unset |
| `remember` | The row; a private chat memory is never recorded |
| An owner deletes a shared memory | The row goes, and its subject (the plant, or the garden for a garden-level fact) is queued in `belief_forget`, in one transaction |
| A plant's or garden's cascade starts | The subject is queued in `belief_forget` in the transaction that sets `deleting_at` |

**A run of the sweep.**

1. For each queued subject: `forget` it, then `finish_belief_forget`, which re-marks its remaining shared memories unsynced (a garden's take its plants') and removes it from the queue in one transaction. A failed erase stays queued.
2. Record each unsynced shared memory, oldest first, and `mark_synced` it. `unsynced_shared` leaves out a plant or garden being deleted. A record under a subject whose erase just failed waits for the next run (a garden covers its plants), and any other failure is logged and retried next run without holding up the rest.

Agent Memory supersedes by `observed_at`, the memory's `created_at`: a write older than its key's current value is dropped ([agent-memory 3](../database/agent-memory.md)). So a retry or a rebuild never brings an older value back, and nothing depends on the sweep's order. A memory recorded from a stale read just before its delete is erased by the next run, from the queue. With Agent Memory off, the sweep only empties the queue.

**The reads.** `current_beliefs` (a direct entity read, about 0.4 s measured) and `search_beliefs` (`/query`, 2 to 10 s measured) return empty when Agent Memory is off, fails or outlasts its `tokio::time::timeout`, with a `warn` log. Neither ever fails a turn, a tool call or a plan.

**Dates in the prompt.** `BeliefView` and `MemoryView` carry the UTC day (`date`) of the belief's `learned_at` and the memory's `created_at`. The prompts say the more recent of a belief and a memory wins, and that the model says which it is going by when a private note is newer than a shared fact.

---

## 15. Concurrency and lifetimes

**What runs where.** `start_turn` runs in the request handler's task. The turn runs in one task spawned on the process's `TaskTracker`, so shutdown waits for turns to store their messages. Within a turn everything is sequential: tool calls run one after another in the model's order, so events and stored results are deterministic. Image crops run on `spawn_blocking`. Text embeddings run behind the `TextEmbedder` port (on a blocking thread, [domain 9](domain.md#9-boundary-traits)).

**One embedder, two modes.** `AgentDeps::embedder` is the pinned text-embedding model. The user's message, `search_memory` and `search_knowledge` are embedded with `EmbedMode::Query`; the memory texts `remember` and extraction store with `EmbedMode::Document`, so they are embedded like the knowledge chunks they are searched beside. The mode is named at each call ([domain 9](domain.md#9-boundary-traits)), so no wiring can swap them, and `model().name` is stored as `embedding_model`.

**What is shared.** `ChatAgent` is a clone of `Arc<Inner>` (the deps, the tool registry, the tracker); `AgentDeps` holds every port as `Arc<dyn Port>` (`Send + Sync + 'static` by the default object lifetime). The store is split into the nine areas the agent uses (`AgentStores`), built from `api`'s one `Arc<dyn Store>` by trait upcasting, so a test fakes only what it touches.

**How the turn owns its `Arc`s.** axum needs a `'static` body, so the stream can't borrow from the handler. `start_turn` moves everything the turn needs into an owned `Turn` and spawns `run(inner: Arc<Inner>, turn, tx, outcome)`. Inside, `let deps = &inner.deps;` borrows from the `Arc` the task owns and lends it on: to `context::load`, to `ToolCtx<'t>`, and to `ChatModel::stream<'a>`, which borrows the `ChatRequest` the task owns. All of those borrows live in one `async` block, which the compiler turns into one future: a borrow held across an `.await` is a field of that future, so it is allowed, and the future is still `'static` because it owns what it borrows from ([Async Book](https://rust-lang.github.io/async-book/); [tokio, spawning: the `'static` bound](https://tokio.rs/tokio/tutorial/spawning#static-bound)). Between model calls the stream is dropped, which ends its borrow of the request, and only then is the request extended with the assistant turn and the tool results.

**Named lifetimes and generics.**

| Item | Lifetime or generic | Why |
|---|---|---|
| `DoseMatch<'t>` | `'t`: the checked text | The match points into the text, so the caller explains the failure without copying |
| `dose_check<'t>(text: &'t str, products: &ProductCatalogue) -> Option<DoseMatch<'t>>` | `'t` names the text | Two reference inputs: elision would not know which one the result borrows from |
| `dose_check_all<'t, I: IntoIterator<Item = &'t str>>` | `'t` and `I` | Strings from several owners (a tool's arguments, a plan's fields), all outliving `'t`; generic so arrays, vectors and iterator chains all work without collecting |
| `ProductCatalogue::mentions<'c, 't>` | `use<'c, 't>` | The iterator borrows the catalogue and the text |
| `ProductMention<'t>` | `'t`: the text | The product name as written |
| `ValidationContext<'k>`, `Index<'k>` | `'k`: the knowledge loaded for one plan | Validation is synchronous; borrowing avoids copying profiles |
| `PlanInput<'k>`, `SharedMemories<'k>` | `'k`: `pipeline`'s records | `write` is awaited in place, so its future may borrow them across `.await`s |
| `PromptView<'k>` and the context views (`'a`) | the input being rendered | Serialised once into the prompt, no copies |
| `AnthropicChat::build<'a>` | `'a`: shared by `self` and the request | The model id comes from `self`, everything else from the request |
| `ToolCtx<'t>` | `'t`: the turn | Lent to every call; covariant, so it shortens freely |
| `Tool::Args` | associated type | One argument type per tool, parsed and schema'd by its type |
| `Erased<T: Tool>` | generic over the tool | Static dispatch inside, `dyn DynTool` outside |
| `DynTool::call<'a> -> BoxFuture<'a, …>` | `'a` | A boxed future needs a named bound |
| `data_json<T: Serialize + ?Sized>` | `?Sized` | Accepts slices (`&[&str]`) as well as structs |
| `SentenceSplitter`, `Turn`, `TurnStream` | none (owned) | They cross `.await`s, tasks and channels |

**What never gets logged** (the `rust-tracing` skill): chat text, memories, plan text, tool arguments, image bytes and embeddings. Logs carry the message id, the tool name, the route and the error chain.

---

## 16. Errors

@@items error.rs TurnError PlanWriterError PrivateMemory ToolError MemoryError@@

| Error | Where it goes |
|---|---|
| `TurnError::Domain(DomainError)` | `api` maps it with its `DomainError` table: `invalid_request` 400, `budget_exceeded` 429, `not_found` 404 |
| `TurnError::Store(StoreError)` | `api`'s `StoreError` table |
| A model failure after the stream started | An `error` event ([section 13](#errors-after-the-stream-started)) |
| `ToolError::Rejected` | Back to the model as an `is_error` tool result with the reason; logged at `info` |
| `ToolError::Store`, `Blob`, `Model` | Back to the model as "unavailable" (`NotFound` as "not found in this garden"); the cause is logged |
| `PlanWriterError` | `pipeline` decides (retry the step, or fail it) |
| `MemoryError` | Logged; the turn has already completed, so nothing is remembered |
| `PrivateMemory` | `pipeline` passes it up: a store that returns a private memory from `shared_for_plan` is a bug |

---

## 17. Testing

83 unit tests, run with `cargo test -p agent` (or `cargo nextest run -p agent`):

| Module | Tests | What |
|---|---|---|
| `safety::dose` | 7 | Every contracts 7 example, failing and passing; the three known false positives; per-sentence conditions; the borrowed match and its span; earliest match; `dose_check_all` |
| `safety::sentences` | 6 | Terminators and line breaks; decimals, abbreviations and list numbers; closing quotes; the undecided full stop; streamed pieces concatenating to the text; the cap |
| `safety::products` | 2 | Built-in words, plurals, case, whole words; knowledge-base products and `broad_spectrum` |
| `safety::plan` | 9 | Every row of the validation table |
| `safety::routes` | 5 | Calibration, the safety tie-break, the off-topic margin, poisoning whatever the scores (and three non-poisoning sentences), no scores |
| `prompt` | 7 | A body can't close its `<data>` element: nested, mixed-case and spaced tags and a tag at the very end are escaped, other tags and short tails are not, escaped text is never escaped twice, an empty body is an empty element; a framed tool result is still its JSON; every system prompt passes its own dose check |
| `model::anthropic` | 3 | A stream cut off before its stop reason ends in an error (`wiremock`). The request body: sorted tools, `strict` only where the spec says, stripped keywords, no `tool_choice` or `thinking`, effort, the reasoning block replayed, `tool_result` with `is_error` and an image after it, the cache breakpoint |
| `model::classifier` | 3 | Parsing and clamping scores, the strict schema, a refusal routed as `safety_sensitive` |
| `plan_writer` | 7 | Private memories refused; one call when clean; exactly one validation round-trip, append-only; one parse retry; a refusal; the prompt's data blocks, the beliefs block with `key`, `value`, `about_plant` and the day only; no beliefs, no block |
| `tools` | 10 | Nineteen tools, sorted, filtered by role; only the three write tools strict; schemas from the argument types; `apply_preview`; crops; `remember`'s optional topic; `search_memory` merges belief hits, and a slow belief store gives empty `beliefs` after 4 s (paused time) |
| `beliefs` | 8 | Queued erases run before any record; a failed record doesn't hold up the rest; a failed erase stays queued and holds back its plant's records; a failed garden erase holds back its plants'; a private memory is never recorded; with Agent Memory off the queue is emptied and nothing is called; reads empty on error and on timeout (paused time) |
| `context` | 2 | The beliefs block, with each belief's day, between the context and the memories; a slow belief store leaves it out after 1.5 s (paused time) while the memory hits arrive |
| `turn` | 11 | A dosed sentence replaced before streaming; the tool loop with the reasoning block replayed unchanged; a dose in tool arguments refused before the tool runs; a write tool unknown to a reviewer; the Maple Row fixture's planted injection, returned by `search_memory`, reaches the model only inside `<data>` with its planted tags escaped, while the stored result keeps the JSON (T-E5.4); the budget; validation; a disconnect stored as interrupted; poison control with no model call; a model failure as an `error` event |
| `memory` | 3 | The summary trigger; the strict extraction schema with `topic` required; every `ExtractedTopic` maps to its wire name |

**Fakes.** `domain`'s `mock` feature gives one `mockall` mock per port (`MockMemoryStore` and `MockBeliefStore` for the mirror, whose call order is recorded through the mocks' closures); `testing::SlowBeliefs` answers every read after a minute, for the timeouts; `testing::garden_deps` builds a garden-anchored `AgentDeps` over mocks; the turn tests build a `ConversationOwner<Viewer>` the only way it can be built, through `authenticate` and `check_conversation` over mocked `AuthVerifier`, `IdentityRepo` and `AccessRepo`. `ChatModel` can't be mocked by `mockall` (its stream borrows the request), and `domain`'s `ScriptedChat` replays one script for every call, so `testing::ScriptedModel` pops one script per call, records every request, and turns an empty script into a provider failure.

---

## 18. Open questions

Real ambiguities found while writing the crate, with the choice made. They belong in [contracts 12](../architecture/contracts.md#12-open-questions).

1. **"`of` within three words".** Read as "`of` is one of the next three words", so "a cup or so of milk" fails and "a drop in temperature tonight" passes. "A drop in temperature of 5 °C" also fails (a fourth false positive); the chat prompt doesn't steer around it.
2. **Thinking blocks and the rebuilt context.** Opus 5.5's thinking is always on and its blocks must be replayed unchanged within a tool loop, but a block is bound to the conversation before it. The context is rebuilt every turn (architecture 7), so blocks are replayed only within a turn, and earlier turns are sent as text only. This needed `ContentPart::Reasoning`, `ChatChunk::Reasoning` and `ReasoningBlock` in `domain` ([section 9](#9-model-adapters)).
3. **`view_image` needs a scan id.** Contracts 8 lists `view_image(image_id, region?)`, but a scan image's object is found through its scan. The tool takes an optional `scan_id`; without it the id is one of this conversation's images.
4. **Garden arguments.** Architecture 7 lists `get_forecast(garden_id, days)`, `get_soil_profile(garden_id)` and `get_garden_map(garden_id)`. Identity comes from the session, never from the model (architecture 11), so these take no garden id: the garden is always the conversation's.
5. **`revise_plan` needs a scan id.** A plant or garden conversation has no single current plan, so the tool names the scan whose current plan changes.
6. **`get_soil_profile`'s `texture`** is the latest guided-check observation of the conversation's scan; no port finds the latest texture across a garden's scans, so plant and garden conversations get none.
7. **A failing summary or `watch_for` item after the retry.** Contracts 7 says failing steps are dropped. Here a failing `watch_for` item is dropped and a failing summary is replaced by a fixed sentence (`FALLBACK_SUMMARY`), so the plan is still stored.
8. **`memory_saved` after an automatic extraction** is sent before `message_completed`, which delays completion by the gate and the extraction (about one cheap call). The alternative, a second stream or a push, isn't in the contracts.
9. **`interrupted` for a failed turn.** A turn that ends with an `error` event is stored with `interrupted: true`, since it never reached `message_completed`.
10. **The poison-control text** has no phone numbers; the deployment provides the local ones.

### Contracts 12, questions 12–24

Where the agent's design settles one of the questions added in contracts v0.8.3:

| Question | Answer here | Where |
|---|---|---|
| 13, unknown fields | The app's `SendMessageRequest` already denies unknown fields (`domain`). The model's tool arguments do too: a call with a field the tool doesn't know is refused with the reason as an `is_error` result, so a misspelt optional argument isn't silently ignored | [section 12](#typed-tools-erased-once) |
| 14, a malformed ID | In tool arguments a malformed id fails parsing and the model is told the arguments are invalid; a `view_image` id that names nothing in the garden or conversation is "No such image", like `404`. App-side ids (`image_ids`) are parsed by `api` | [section 12](#every-tool) |
| 17, timeouts and `retryable` | A model call that times out mid-stream is `upstream_unavailable` (default `retryable: true`), not `internal`: the provider was slow, not our code. A classifier timeout (3 s) is not an error at all: the turn is routed as a care question | [section 13](#errors-after-the-stream-started), [section 7](#7-router-and-safety-gate) |

---

## 19. Sources

**Project documents**
- [Architecture 7](../architecture/README.md#7-chat-and-agent-core), [8.2](../architecture/README.md#82-chat-turn), [10](../architecture/README.md#10-models-and-gpu-placement), [11](../architecture/README.md#11-security-privacy-and-safety)
- [Contracts 1](../architecture/contracts.md#1-conventions), [2](../architecture/contracts.md#2-errors), [7](../architecture/contracts.md#7-pipeline-records), [8](../architecture/contracts.md#8-chat-and-memory), [10](../architecture/contracts.md#10-knowledge-seed-format), [12](../architecture/contracts.md#12-open-questions)
- [Database 5](../database/README.md#5-reads-indexes-and-vector-search); [frontend 10](../frontend/README.md#10-live-streams); [domain](domain.md), [integrations](integrations.md)
- The `claude-api` skill (bundled with Claude Code, cached 2026-10-06) and the research notes on the Anthropic Messages API (streaming, tool use, structured outputs and their keyword limits, `claude-opus-5-5` and `claude-haiku-5-5`, forced `tool_choice`, preserved thinking)

**Rust**
- [The Book, 10.3: lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html); [18.2: trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html); [16.4: `Send` and `Sync`](https://doc.rust-lang.org/book/ch16-04-extensible-concurrency-sync-and-send.html)
- [Reference: dyn compatibility](https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility); [lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html); [subtyping and variance](https://doc.rust-lang.org/reference/subtyping.html#variance); [trait upcasting](https://doc.rust-lang.org/reference/type-coercions.html#r-coerce.unsize.trait-upcast)
- [Edition Guide: RPIT lifetime capture](https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html); [`std::sync::LazyLock`](https://doc.rust-lang.org/std/sync/struct.LazyLock.html)
- [Async Book](https://rust-lang.github.io/async-book/)

**Crates (pinned versions)**
- [regex 1.13.1: `RegexSet`](https://docs.rs/regex/1.13.1/regex/struct.RegexSet.html), [`RegexBuilder`](https://docs.rs/regex/1.13.1/regex/struct.RegexBuilder.html)
- [tokio 1.53.3: `mpsc::Sender::closed`](https://docs.rs/tokio/1.53.3/tokio/sync/mpsc/struct.Sender.html#method.closed), [`select!` cancellation safety](https://docs.rs/tokio/1.53.3/tokio/macro.select.html#cancellation-safety), [spawning](https://tokio.rs/tokio/tutorial/spawning)
- [tokio-util 0.7.20: `CancellationToken`](https://docs.rs/tokio-util/0.7.20/tokio_util/sync/struct.CancellationToken.html), [`TaskTracker`](https://docs.rs/tokio-util/0.7.20/tokio_util/task/task_tracker/struct.TaskTracker.html); [tokio-stream 0.1.19: `ReceiverStream`](https://docs.rs/tokio-stream/0.1.19/tokio_stream/wrappers/struct.ReceiverStream.html)
- [async-stream 0.3.6](https://docs.rs/async-stream/0.3.6/async_stream/); [async-trait 0.1.92](https://docs.rs/async-trait/0.1.92/async_trait/); [futures 0.3: `BoxFuture`](https://docs.rs/futures/0.3.34/futures/future/type.BoxFuture.html)
- [schemars 1.2.3](https://docs.rs/schemars/1.2.3/schemars/); [image 0.25.10: `DynamicImage::crop_imm`](https://docs.rs/image/0.25.10/image/enum.DynamicImage.html#method.crop_imm)

Every crate API above compiled against its pinned version in the scratch workspace ✓. The links point at those versions; they were not all opened on 2026-10-16.
