# PDM Frontend Flows

> **Status:** Draft v0.4.3, 2026-10-20. v0.4.3: a capture slot's delete can fail: after two retries the shot goes back to `stored` (section 14). v0.4.2: the second factor is a code by text, not TOTP (section 1).
> v0.4 applies the third audit and contracts v0.8.2: a failed scan supersedes its question first, and a guided check that finishes after a failure is kept for a Retry.
> v0.3 applies the second audit and contracts v0.8.1:
>   - **Species and capture:** the species card shows each photo's credit, and discarding a capture also deletes its jar-test task.
>   - **Diagnosis:** with `low_evidence`, the closer photo answers the open question.
>   - **Push:** notifications are handled on arrival as well as on tap, and push coverage is stated as it is.
>   - **Smaller fixes:** the stream stays open while a check runs on a failed scan, account-deletion names come from the cache, and the retry rule names `409 request_in_progress`.
>
> v0.2 applied the first audit and used [contracts](../architecture/contracts.md) v0.8:
>   - **Scan stream:** scans open with a full event replay, and a closed stream refetches the scan before reconnecting.
>   - **Onboarding:** consent is recorded before the location is sent.
>   - **Uploads:** a re-encoded upload gets a new key.
>   - **New contract fields:** jar-test tasks carry their scan, the species card shows reference photos, and discarding a capture deletes its pending scan.
>   - **Chat:** retries take the JSON replay, and only editors and owners get the rescan button.
>   - **Push:** routed from the `PushPayload`.
>   - **Browsers:** phone and in-app browsers get their own sign-in and capture paths.
>   - **Smaller fixes:** wording, triggers and state-machine edges.
>
> v0.1: Sequence diagrams for every user flow and the state machines behind them.

Companion to the [frontend architecture](README.md) and the [screens](screens.md). Every request and event here is defined in the [data contracts](../architecture/contracts.md); the server-side behaviour is in the [system architecture](../architecture/README.md), section 8.

**Notation.** `──▶` is a call or an event, `◀──` its response. `[…]` is a note. Lifelines are the user, the app's layers (screen, model, api) and the backend. Idempotency keys are written `key=A`; the same letter means the same key.

## Contents

**Sequences**

1. [App start and sign-in](#1-app-start-and-sign-in)
2. [Onboarding and the first garden](#2-onboarding-and-the-first-garden)
3. [Invites and membership](#3-invites-and-membership)
4. [Capture and submit a scan](#4-capture-and-submit-a-scan)
5. [Live scan progress](#5-live-scan-progress)
6. [Answering a question](#6-answering-a-question)
7. [Species confirmation and reruns](#7-species-confirmation-and-reruns)
8. [Late work and plan updates](#8-late-work-and-plan-updates)
9. [Chat turn](#9-chat-turn)
10. [Confirmation cards and memories](#10-confirmation-cards-and-memories)
11. [Garden mapping session](#11-garden-mapping-session)
12. [Push notifications and deep links](#12-push-notifications-and-deep-links)
13. [Sign-out and account deletion](#13-sign-out-and-account-deletion)

**State machines**

14. [Capture slot](#14-capture-slot)
15. [Capture session](#15-capture-session)
16. [Scan status as the app shows it](#16-scan-status-as-the-app-shows-it)
17. [Scan event stream](#17-scan-event-stream)
18. [Chat message](#18-chat-message)
19. [Pending action card](#19-pending-action-card)
20. [Garden map upload](#20-garden-map-upload)

21. [Sources](#21-sources)

---

## 1. App start and sign-in

```
 User          App (root layout)         Clerk SDK                API
  │ opens app        │                       │                      │
  │─────────────────▶│ splash stays up       │                      │
  │                  │ load session ────────▶│ [secure store; browser session on the web]
  │                  │◀──── isLoaded ────────│                      │
  │                  │                       │                      │
  │      ┌───────────┴─── isSignedIn? ───────┴──────┐               │
  │      │ no                                  yes  │               │
  │      ▼                                          ▼               │
  │  (auth)/sign-in                          (app) SessionGate      │
  │  email + password ──▶ signIn ──────────▶│                      │
  │                  │◀── needs_second_factor│ [phone reserved]     │
  │  code by text ──▶│ verify ──────────────▶│                      │
  │                  │◀──── session active ──│                      │
  │                  │ guard flips to (app)  │                      │
  │                  │                       │                      │
  │                  │ SessionGate: GET /me ─┼──── Bearer token ───▶│ [creates user on first request]
  │                  │◀──────────────────────┼──── 200 Me ──────────│
  │                  │ GET /gardens ─────────┼─────────────────────▶│
  │                  │◀──────────────────────┼──── Page<Garden> ────│
  │                  │                       │                      │
  │                  ├─ no gardens ─▶ onboarding (flow 2)           │
  │                  ├─ display_name empty ─▶ PATCH /me with Clerk's full name, or ask in onboarding
  │                  └─ else ─▶ (tabs)/index for the active garden (persisted, or the first by name)
  │                  │ push: register token (flow 12), native builds only
```

- **Google and Apple** use `useSSO` in a browser session on native builds and a redirect in browsers, and return to the app; the rest is the same.
- **In an embedded web view** (`platform/browser` reports `in_app`: Instagram, Facebook, LinkedIn), the sign-in screen offers email and password only, with "For Google or Apple sign-in, open this page in Safari or Chrome" and **Copy link**. Google refuses OAuth in embedded web views. Links opened from Slack or Gmail usually land in the system's in-app view, where every method works ([README 13](README.md#phone-browsers-and-in-app-browsers)).
- The active garden is checked against `GET /gardens`. A persisted ID that is no longer in the list (the user was removed, or the garden was deleted) falls back to the first garden, with a notice when a `garden_deleted` push was received.

---

## 2. Onboarding and the first garden

```
 User              App (onboarding)            platform/location        API
  │ name the garden      │                           │                    │
  │─────────────────────▶│ ask for location consent  │                    │
  │ allow ───────────────│──────────────────────────▶│ OS or browser prompt
  │                      │◀── lat, lon ──────────────│                    │
  │ precision: exact or "about 1 km"                 │                    │
  │─────────────────────▶│ PATCH /me { location_consent: true } ─────────▶│ [consent recorded first]
  │                      │◀──────────────────────────┼── 200 Me ──────────│
  │                      │ POST /gardens  key=A ─────┼───────────────────▶│
  │                      │   { name, location: { lat, lon, precision } }  │
  │                      │◀──────────────────────────┼── 201 Garden ──────│ [SoilGrids fetched in the background]
  │                      │ set active garden; invalidate qk.gardens()     │
  │                      │ go to (tabs)/index        │                    │
```

- **Without consent** the screen asks for a postal code or a city ("Guelph, ON") and sends `{ postal_code }` or `{ city }` (contracts 9). The garden's `location_label` and `location_precision` come back from the API; raw coordinates are never returned.
- `soil_profile_status: "pending"` shows a quiet "soil data loading" note on the garden home until a later read says `ready` or `unavailable`.

---

## 3. Invites and membership

```
 Owner (app)                     API                         Invitee (app)
  │ members ▶ "Invite"            │                               │
  │ role: editor                  │                               │
  │ POST /gardens/{id}/invites ──▶│                               │
  │   key=B { role }              │                               │
  │◀── 201 Invite { code } ───────│                               │
  │ share sheet: pdm://invite/<code>  (web: https://<web origin>/invite/<code>)
  │──────────────────────────────────────────────────────────────▶│ opens link
  │                               │                               │ invite/[code] (outside the guards)
  │                               │                               │ signed out? ─▶ sign-in, then back here
  │                               │◀── POST /invites/{code}/accept│
  │                               │─── 200 Garden ───────────────▶│ set active garden; invalidate qk.gardens()
  │                               │                               │ ─▶ (tabs)/index
```

| Response | Invitee sees |
|---|---|
| `200` (already a member) | The garden opens; the role is unchanged |
| `404` | "This invite isn't valid any more. Ask for a new one." Unknown, used, revoked and expired codes look the same (contracts 9) |
| `429` | "Too many attempts. Try again in a minute." |

- Only **owners** see open invites (`GET /gardens/{id}/invites`), with their role and expiry, and can revoke one (`DELETE /gardens/{id}/invites/{invite_id}`).
- **Leaving** a garden (`DELETE /gardens/{id}/members/{my_user_id}`) or **removing** a member asks first, and says that the person's private chats in that garden are deleted (contracts 9, removing a member). The last owner can't leave or be demoted (`409 invalid_state`); the screen hides those actions.

---

## 4. Capture and submit a scan

```
 User            Capture screen          capture model           api                    API
  │ "Scan" on a plant  │                       │                    │                       │
  │───────────────────▶│ useStartCapture       │                    │                       │
  │                    │──────────────────────────────────────────▶│ POST /scans key=C ───▶│
  │                    │                       │                    │   { garden_id, plant_id }
  │                    │◀───────────────────────────────────────────│◀── 201 Scan pending ──│
  │                    │ open capture/[scanId] │ draft created      │                       │
  │                    │                       │ (persisted)        │                       │
  │ frame whole plant  │ [LiDAR: distance hint "move back"]         │                       │
  │ shutter ──────────▶│ capture() ──────────▶ │ slot whole_plant:  │                       │
  │                    │                       │ captured, key=D    │                       │
  │                    │                       │──── upload ───────▶│ POST /scans/{id}/images key=D
  │                    │                       │                    │  kind, file [, depth, depth_confidence, depth_meta]
  │                    │                       │◀───────────────────│◀── 422 image_quality ─│ [not stored]
  │◀── "Too blurry: hold still and tap the leaf to focus"  (hint tap_to_focus)              │
  │ retake ───────────▶│ capture() ──────────▶ │ captured, key=E    │                       │
  │                    │                       │──── upload ───────▶│ POST … key=E ────────▶│
  │                    │                       │◀───────────────────│◀── 201 { image, quality, depth? }
  │◀── tick, haptic    │                       │ stored             │                       │
  │ leaf close-up …    │  (same for each slot; each shot uploads as soon as it is taken)    │
  │ "Analyse" ────────▶│ submit allowed? ─────▶│ whole_plant stored, nothing uploading: yes │
  │                    │──────────────────────────────────────────▶│ POST /scans/{id}/submit ─▶│
  │                    │◀───────────────────────────────────────────│◀── 202 Scan queued ───│
  │                    │ draft cleared; replace route with scans/[scanId] (flow 5)          │
```

- **Same key, same bytes.** A retryable failure (network, `503`, `409 request_in_progress`) is retried with the **same** key and the same file. A **retake** is a new action, so it gets a new key.
- **Re-encoding.** A picked photo refused with `413` or `415` is re-encoded once and sent as a new action with a new key (`key=D2`), because the first answer is stored under the old key (contracts 1). A LiDAR shot is never re-encoded: it must be the module's file, unchanged (contracts 3).
- **Replacing** a stored shot: `DELETE /scans/{id}/images/{image_id}`, then the new shot uploads with a new key.
- **Depth problems never fail the upload.** `depth: { stored: false, problem }` keeps the photo and shows a small "no depth for this shot" note (contracts 3).
- **In a phone browser** the shutter is **Take photo**, which opens the phone's own camera through `launchCameraAsync`, called directly inside the press handler. The slot's instructions are shown before the camera opens. **Choose from library** sits beside it. On a desktop it is a file picker. HEIC files are converted to JPEG before the first attempt (contracts open question 8).
- **The app closes mid-capture:** the draft (scan ID, slots, the exact files, keys) is persisted, and the plant page offers **Continue scan** for a `pending` scan with a draft.
- **Discard** asks first, saying that a jar-test task started on this scan goes too, then `DELETE /scans/{id}` (`204`) and drops the draft. The server deletes that task with the scan; the app invalidates `qk.tasks`, `qk.plants` and `qk.timeline`. A `409` means the scan was submitted meanwhile (from another device), and the app opens it instead.
- **`409 invalid_state` on submit with `hint: "new_scan"`** (a failed scan that can't be resubmitted) offers a new scan instead.

---

## 5. Live scan progress

```
 Scan screen           scan model               api                         API
  │ mount                  │                      │                            │
  │ useScan ──────────────────────────────────────▶ GET /scans/{id} ───────────▶│
  │◀────────────────────────────────────────────── 200 ScanDetail (status analysing, last_event_seq = 11)
  │ seed(state, detail) ──▶│ records from the snapshot                          │
  │ not at rest ─▶ useScanEvents ─────────────────▶ GET /scans/{id}/events ────▶│
  │                        │                      │   (no Last-Event-ID: full history)
  │                        │◀── seq 1 … 11 replayed ◀────────────────────────────│ [rebuilds stage ticks, candidates,
  │                        │    folded without animation                         │  needs_confirmation]
  │                        │◀── seq 12 finding (disease, region with mask)  [live]│
  │◀── re-render: stage ticks, finding drawn on the photo                        │
  │                        │◀── : keepalive  [every 15 s; resets the 45 s watchdog]
  │ app to background      │                      │                            │
  │ abort stream ─────────────────────────────────▶ (connection closed)        │
  │ app to foreground      │                      │                            │
  │ reopen ───────────────────────────────────────▶ GET …/events  Last-Event-ID: 21
  │                        │◀── seq 22 … (missed events replayed, then live)    │
  │                        │◀── seq 28 diagnosis                                │
  │                        │◀── seq 32 status_changed diagnosed → planned       │
  │                        │◀── seq 33 plan_ready { provisional: false }        │
  │                        │ effectsOf: invalidate qk.scan, qk.plant, qk.plants, qk.timeline,
  │                        │            qk.measurements, qk.tasks, qk.memories  │
  │ refetch ──────────────────────────────────────▶ GET /scans/{id} ───────────▶│
  │◀────────────────────────────────────────────── 200 ScanDetail with plan     │
  │                        │◀── (server closes: at rest)                        │
  │ refetched record at rest ─▶ stream stays closed │                            │
```

- **A scan already at rest** when opened (no guided check `analysing`, and `planned` with no question open, or `failed`) gets no stream at all; the records are the whole screen.
- **Duplicates are harmless.** The reducer ignores any `seq` it has applied, so a replay that overlaps live events changes nothing (contracts 4, resume).
- **`run_started`** resets the listed stages and drops their findings; findings of other stages stay.
- **`failed`** shows the failure. `retryable: true` offers **Retry** (`POST /scans/{id}/submit`, a new run) to editors and owners; otherwise **New scan**. Failing a scan first resolves its open question as `superseded` (contracts 3), so the question card goes before the failure shows. A guided check that finishes after the failure only adds its result, which a Retry run reads.
- The stream's states and transitions are in [section 17](#17-scan-event-stream).

---

## 6. Answering a question

### Indicator question

```
 User              Scan screen                 api                     API
  │                    │◀── event: question { kind: indicator, options } ──────│ [status awaiting_answer]
  │◀── question card: "Do the spots have rings, like a target?"  yes · no · not sure
  │ "yes" ────────────▶│ POST /scans/{id}/answers  key=F ──────────────────────▶│
  │                    │   { question_id, option_id: "yes" }                   │
  │                    │◀────────────────────────────────── 202 Scan ──────────│
  │                    │◀── question_resolved { by: user }                     │
  │                    │◀── stage_* scoring … diagnosis … plan_ready           │
```

### Photo request

```
 User              Scan screen      Capture (question mode)     api                  API
  │                    │◀── question { kind: photo_request, photo_kind: leaf_underside, photo_hint }
  │ "Take photo" ─────▶│ open capture/[scanId]/question/[questionId]                 │
  │ shutter ──────────────────────────▶│ one slot, kind leaf_underside               │
  │                    │               │ POST /scans/{id}/images key=G ────────────▶│
  │                    │               │   kind, file, question_id [, depth parts]   │
  │                    │               │◀────────────────────────── 201 ────────────│ [question → answering]
  │                    │◀── back to the scan screen; card shows "Checking your photo"│
  │                    │◀── [some depth photos] stage_* segmentation, measurement    │
  │                    │◀── [unless measurements settled everything] stage_* verify  │
  │                    │◀── stage_* scoring                                          │
  │                    │◀── question_resolved { by: photo }  or  { by: dropped }     │
```

- **Segmentation and measurement** run first only for a photo with depth whose question has a measurable indicator that the photo's kind can show and that no user or measurement has answered (contracts 7, photos sent for a question). When the measurements settle every indicator, no `verify` job runs and the scan is rescored at once.
- While a question is `answering`, the card accepts nothing else. A second answer or photo gets `409 invalid_state`, and the card refetches the scan.
- `by: dropped` means the photo couldn't be analysed: the card says so, and the diagnosis and plan stay (contracts 3).
- **No answer within 10 minutes:** the scheduler continues. The app sees `status_changed awaiting_answer → diagnosed` and a `diagnosis` with status `uncertain`; the question stays open and the plan arrives provisional. Answering later is late work (flow 8).
- Viewers and reviewers see the question without the answer buttons.
- **With `low_evidence`**, the screen's **Take a closer photo** opens this question's capture when the open question is a `photo_request`, so the photo answers it (a late answer that re-plans the scan) instead of starting a new scan.

---

## 7. Species confirmation and reruns

```
 User              Scan screen / plant page         api                      API
  │                    │◀── species_suggested { candidates, needs_confirmation }  │
  │◀── needs_confirmation false: "Is this a tomato?"  [photo]  [Yes] [Change]     │
  │◀── needs_confirmation true:  top 3 with reference photos and credits, plus "Something else"
  │ picks "pepper" ───▶│ POST /plants/{id}/species ──────────────────────────────▶│
  │                    │   { species_id: "pepper" }                               │
  │                    │◀──────────────────────────────────────── 200 Plant ──────│
  │                    │ invalidate qk.plant, qk.plants                           │
  │                    │◀── species_confirmed { rerun: true }                     │
  │                    │◀── run_started { reason: species_changed, rerun_stages } │
  │                    │ reducer: run 2; reset species, disease, vlm_*, scoring …; drop their findings
  │                    │◀── status_changed → analysing  … stage events of run 2 … │
  │                    │◀── plan_superseded { reason: species_changed } [if run 1 had a plan]
  │◀── notice: "Plan updated for pepper"                                          │
```

- **Photos** come from `SpeciesCandidate.reference_image`, whose credit is shown under the photo on the card itself (contracts 3). A candidate without a photo shows its name only. If a photo's URL has expired on a screen left open, the card refetches the plant and uses `plant.species_candidates`.
- **Change** and **Something else** open the species picker (`GET /species?q=`), a search over the knowledge base's species. A species not in it is typed as a name and sent as `{ custom_name }` (at most 80 characters); the plant is treated as unsupported and the rerun skips species identification.
- The card can also be answered from the plant page while no scan is open. Only the plant's most recent scan, if it was created in the last 24 hours and isn't `failed`, follows the species (contracts 3); the plant page says so.
- If that scan is `pending` or `queued`, no rerun happens: `species_confirmed { rerun: false }`, and the run uses the new species.
- A scan opened at rest gets no `species_suggested` event, so its card shows the top 3 whenever the plant's species is still `suggested`.

---

## 8. Late work and plan updates

Late work is a late answer, a late photo for a question, or any guided soil check (contracts 3). It never changes the scan's status. If the stream was closed, because the app was in the background or the scan was at rest, the app **reopens it** when the user starts late work.

```
 User              Scan screen                    api                       API
  │ [scan planned; plan provisional; question still open at the top of the plan;
  │  stream closed while the app was in the background]
  │ "no" ─────────────▶│ POST /scans/{id}/answers key=H ──────────────────────────▶│
  │                    │◀───────────────────────────────────────── 202 Scan ───────│
  │                    │ reopen the stream, Last-Event-ID: 33                      │
  │                    │◀── question_resolved { by: user }                          │
  │                    │◀── diagnosis { status: confirmed, supersedes }             │
  │                    │◀── plan_superseded { reason: late_answer, old_plan_id, new_plan_id }
  │                    │ effectsOf: invalidate qk.scan, qk.plant, qk.timeline, qk.tasks, …
  │◀── "Plan updated: the question is settled". The old plan's open tasks are gone (dismissed by the server)
```

```
 User              Capture (guided)               api                       API
  │ "Jar test" ───────▶│ instructions; "Start"                                     │
  │                    │ POST /tasks key=I { garden_id, plant_id, scan_id, title: "Photograph the jar", due }
  │                    │◀───────────────────────────────────────── 201 Task ───────│
  │ hours later, from the task, on any device: open capture/[task.scan_id]/guided/jar_test
  │ shutter ──────────▶│ POST /scans/{id}/images key=J kind=jar_test ─────────────▶│
  │                    │◀───────────────────────────────────────── 201 ────────────│ [own guided job]
  │                    │ open the stream ──▶ guided_check_done { check }            │
  │                    │◀── diagnosis, plan_superseded { reason: guided_check } [only if the texture changed]
  │                    │ check done: PATCH /tasks/{id} { done: true }               │
```

- A guided check can be added in any status except `failed`, before or after submit, and as many times as needed after submit (contracts 3).
- A jar that hadn't settled gets no texture result. The check shows "Not settled yet: photograph it again later", and the task stays open.
- While a check is `analysing`, the scan isn't at rest, so the server keeps the stream open (contracts 4).
- When the app is closed, a late result that produces a new plan is pushed (`plan_superseded`, flow 12). A check that doesn't change the plan, or a dropped photo answer, isn't pushed; it is on the scan and the timeline when the user opens them (contracts 9, push notifications).

---

## 9. Chat turn

```
 User              Conversation screen      message model          api                  API
  │ "Ask about this result" from a scan          │                      │                    │
  │──────────────────▶│ GET /conversations?garden_id&anchor_type=scan&anchor_id ─────────▶│
  │                   │◀── Page<Conversation> (empty) ─────────────────────────────────────│
  │                   │ POST /conversations key=K { garden_id, anchor: { type: scan, id } }▶│
  │                   │◀── 201 Conversation ───────────────────────────────────────────────│
  │ types, attaches a photo                       │                      │                    │
  │                   │ POST /conversations/{id}/images key=L (file) ─────────────────────▶│
  │                   │◀── 201 { image_id, url } ──────────────────────────────────────────│
  │ send ────────────▶│ optimistic user bubble   │                      │                    │
  │                   │ POST /conversations/{id}/messages key=M { text, image_ids } ──────▶│
  │                   │ Content-Type: text/event-stream ─▶ messageReducer                  │
  │                   │                          │◀── message_started { route } ◀────────────│
  │                   │                          │◀── tool_started { label: "Checking the forecast" }
  │                   │                          │◀── tool_finished { ok: true }             │
  │                   │                          │◀── text_delta × n                         │
  │                   │                          │◀── citation                               │
  │                   │                          │◀── pending_action (flow 10)               │
  │                   │                          │◀── memory_saved (flow 10)                 │
  │                   │                          │◀── message_completed { usage }            │
  │◀── finished bubble with sources, card, "Remembered: …"                                    │
  │                   │ invalidate qk.conversation(id), qk.conversations(…)                  │
```

- **A chat photo** goes through the same checks as a scan upload: `image_quality`, `413` and `415` show under the attachment with a retry, and the message can be sent without it.
- **A send that fails before any response** (network error) is retried with the **same** key. If the first attempt had finished, the server answers `200 application/json { message }`, the idempotent replay (contracts 1), and the app shows that message as complete.
- **Errors before the stream** are HTTP errors (`budget_exceeded` says when the budget resets). **Errors during it** arrive as an `error` event that ends the message.
- **The stream drops:** the message is shown as interrupted with **Resend**, which sends a new request with a **new** key (`key=N`).
- **`rescan_requested`** shows **Take a new photo**, which calls `useStartCapture({ plant_id, photo_kind })` (flow 4), for editors and owners. Viewers and reviewers see the instructions as text.
- Viewers and reviewers chat too; they are never offered write tools, so they never get a confirmation card.
- Each finished assistant message has thumbs up and down; a tap sends `POST /messages/{id}/feedback { rating, notes? }`.

---

## 10. Confirmation cards and memories

```
 User              Chat card                       api                        API
  │◀── card "Add a task?"  [Add task] [Not now]  (from pending_action, status proposed)
  │ "Add task" ──────▶│ POST /pending-actions/{id}/confirm ────────────────────────▶│
  │                   │◀── 200 { pending_action: confirmed, result: { task } } ─────│
  │                   │ invalidate qk.tasks(gardenId); card shows "Added"           │
  │                   │                                                             │
  │◀── plan card "Revise the plan?" with changed steps and "This changes the plan for everyone in the garden"
  │ confirm ─────────▶│ POST /pending-actions/{id}/confirm ────────────────────────▶│
  │                   │◀── 409 invalid_state { current_plan_id } ───────────────────│ [the plan changed meanwhile]
  │◀── card: "The plan changed since this was suggested. Ask again?"  ─▶ prefills the composer
  │                   │                                                             │
  │◀── "Remembered: it's in a container"  [Undo] [Share with garden]                │
  │ Share ───────────▶│ POST /memories/{id}/share ─────────────────────────────────▶│
  │                   │◀── 200 Memory { source: user, shared: true } ───────────────│
  │ (or Undo) ───────▶│ DELETE /memories/{id} ─────────────────────────────────────▶│ 204
```

| Confirm response | Card shows |
|---|---|
| `200` | Confirmed, with the task or "Plan updated" (invalidates `qk.scan` and `qk.tasks`) |
| `200` on a retry of a confirmed card | The same result |
| `409` with `current_plan_id` | "The plan changed. Ask again?" |
| `409` with `hint: neighbour_removed` | "A plant in this plan was removed. Ask again?" |
| `409` with `hint: plant_removed` | "That plant was removed." No retry |
| `409` on a card already `rejected` | Rejected |
| `409` on a card past `expires_at` | Expired |

**Share with garden** appears only for editors and owners (contracts 8, memory).

---

## 11. Garden mapping session

```
 User              Map screens                 lidar module               api                 API
  │ "Map garden" ─────▶│ GET /gardens/{id}/map ─────────────────────────────────────────────▶│
  │                    │◀── { map: { version: 4, … } } ───────────────────────────────────────│
  │                    │ base_version = 4; load every page of GET /plants for the picker      │
  │                    │ savedWorldMapVersion() ──▶│                                          │
  │                    │◀── 4 ─────────────────────│ [matches: an edit is offered]            │
  │ "Edit map" ───────▶│ relocalise() ────────────▶│                                          │
  │                    │◀── "ok" ──────────────────│ [failed → map from the start]            │
  │ walk, tap corners ─────────────────────────────▶ onGroundTap(x_m, y_m) ─▶ draft: bed polygon
  │ tap a plant ───────────────────────────────────▶ onPinPlaced(x_m, y_m, canopy) ─▶ "Which plant?"
  │ picks "Tomato 2" or names a new plant ─▶ draft: pin { plant_id | new_plant }               │
  │                    │                           │ onHeading(north, accuracy ≤ 20°) ─▶ north_deg
  │ "Save" ───────────▶│ PUT /gardens/{id}/map key=P { base_version: 4, beds, pins, north_deg? }─▶│
  │                    │◀── 200 GardenMap { version: 5 } ──────────────────────────────────────│
  │                    │ saveWorldMap(5) ─────────▶│ [stays on the phone]                     │
  │                    │ invalidate qk.map, qk.plants                                         │
```

| Response | App behaviour |
|---|---|
| `200` | Saved; the module's world map is tagged with the new version |
| `409 invalid_state` with `current_map_version` | "The map changed while you were mapping." **Reload** (read the map, map again) or **Replace** (after confirming, resend with `base_version` = the current version and a new key) |
| `400 invalid_request`, `problem: unknown_plant`, `plant_ids` | Those plants were deleted: remove their pins, resend with the **same** `base_version` and a **new** key, because the `400` is stored under the old key (contracts 9) |
| Tracking lost during mapping | The session restarts from the beginning (architecture 6.8) |

---

## 12. Push notifications and deep links

```
 App (native build)       platform/push              API                Expo Push / OS
  │ after sign-in              │                       │                      │
  │ register() ───────────────▶│ permission, token     │                      │
  │◀── { token, platform } ────│                       │                      │
  │ POST /me/push-tokens { token, platform } ─────────▶│ 204                  │
  │                            │                       │ plan_ready, question, forecast alert, …
  │                            │                       │─────────────────────▶│
  │                            │◀── tapped; data = PushPayload ◀──────────────│
  │◀── onOpen(payload) ────────│  (tapped)             │                      │
  │ v or kind unknown? ─▶ ignore                                              │
  │ signed in?  yes ─▶ router.push(routeFor(payload))                         │
  │             no  ─▶ sign-in, then router.push(routeFor(payload))           │
```

**Arriving while the app is open.** `platform/push.onReceive(payload)` fires without a tap:
- a `scan` payload reopens that scan's stream if its screen is mounted and the stream is closed (flow 17), and invalidates `qk.scan` and `qk.plant`;
- `setForegroundFilter` hides the banner for the scan already on screen, and shows the rest;
- `alert` invalidates `qk.tasks`, and `garden_deleted` invalidates `qk.gardens()`.

**Tapped.** `routeFor` lives in `features/account`, mapping contracts 9's `PushPayload`:

| `kind` | Route | Also |
|---|---|---|
| `scan` (`plan_ready`, `plan_superseded`, `question`, `failed`) | `/scans/<scan_id>`, after switching the active garden to `garden_id` | |
| `alert` | `/tasks?source=forecast_alert`, in `garden_id` | |
| `garden_deleted` | The garden home of the next garden, or onboarding | A notice: "<garden_name> was deleted"; invalidates `qk.gardens()` |

- Expo Go and the web build have no push. The alert inbox (the tasks screen filtered to `source=forecast_alert`) shows the same alerts in every build (architecture 8.3).
- Invite links (`pdm://invite/<code>`, or the web URL) open `invite/[code]` (flow 3).

---

## 13. Sign-out and account deletion

```
 User              Settings                    api                    Clerk SDK          API
  │ "Sign out" ──────▶│ DELETE /me/push-tokens/{token, URL-encoded} ─────────────────────▶│ 204
  │                   │ queryClient.clear(); reset zustand stores; drop capture drafts    │
  │                   │ signOut() ─────────────────────────────────────▶│                │
  │                   │ guard flips to (auth)/sign-in                                    │
  │                   │                                                                  │
  │ "Delete account" ▶│ account/delete: what is deleted, what stays with shared gardens (architecture 5)
  │ types "delete" ──▶│ note garden names and member names from the cache (qk.gardens, qk.members)
  │                   │ DELETE /me ─────────────────────────────────────────────────────▶│
  │                   │◀── 202 { deleted_gardens, transferred_gardens } ──────────────────│ [cascade runs; API deletes the Clerk account]
  │◀── summary: "2 gardens deleted; 'Allotment' now owned by Sam" (IDs resolved from the noted names)
  │                   │ local sign-out as above (no push-token call: the cascade removes it)
```

---

## 14. Capture slot

One per slot of the shot plan ([README 11](README.md#capture)); `leaf_closeup` has two slots.

```
                 capture()
     ┌───────┐ ───────────▶ ┌──────────┐  upload (new key)  ┌───────────┐   201    ┌────────┐
     │ empty │              │ captured │ ─────────────────▶ │ uploading │ ───────▶ │ stored │
     └───────┘ ◀──────┐     └──────────┘                    └───────────┘          └────────┘
         ▲            │          ▲                           │   │  │  │              │
         │            │          │ retake                    │   │  │  │              │ replace
         │            │     ┌──────────┐  422 image_quality  │   │  │  │              ▼
         │            │     │ rejected │ ◀───────────────────┘   │  │  │        ┌──────────┐
         │            │     │ (hint)   │                         │  │  │        │ deleting │
         │            │     └──────────┘                         │  │  │        └──────────┘
         │            │     ┌──────────┐  network, 5xx,          │  │  │   204       │
         │            │     │          │  409 request_in_progress│  │  │             │
         │            │     │  failed  │ ◀───────────────────────┘  │  │ ◀───────────┘ to empty
         │            │     └──────────┘ ── retry (same key, same bytes) ──▶ uploading
         │            │                                             │  │
         │            │     413, 415 on a picked photo ─▶ re-encode ┘  │  ─▶ uploading (new key)
         │            │                                                │
         │            └── other 4xx, or 413/415 on a LiDAR shot ───────┘  message shown
         └──────────── discard
```

- **A failed delete** (network, 5xx) is retried twice with backoff. If it still fails, the slot goes from `deleting` back to `stored` with the message and **Try again**, and a retake that was waiting on the delete is dropped with its photo. A `404` means the image is already gone, which counts as the `204`.
- **A saved draft is checked** against `GET /scans/{id}` once per visit. A stored shot whose image the server no longer has goes back to `empty`; a server image the draft lacks fills a free slot of its kind, unless a slot of that kind still holds an upload, since that upload may be the image.

---

## 15. Capture session

```
 ┌──────────┐ POST /scans 201 ┌───────────┐ submit (whole_plant stored, ┌────────────┐  202  ┌───────────┐
 │ creating │ ──────────────▶ │ capturing │ ──── nothing uploading) ──▶ │ submitting │ ────▶ │ submitted │ ─▶ scans/[scanId]
 └──────────┘                 └───────────┘                             └────────────┘       └───────────┘
       │                       │   │    ▲                                   │
       │ error                 │   │    │ app reopened: draft restored      │ 422 missing_required_shot,
       ▼                       │   │    │                                   │ 409 hint new_scan, network
   error shown                 │   ▼    │                                   ▼
                               │ ┌───────────────┐                    back to capturing, with the reason
                               │ │ suspended     │
                               │ │ (draft saved) │
                               │ └───────────────┘
                               │ discard (confirmed): DELETE /scans/{id}
                               ▼
                         ┌───────────┐ 204 ─▶ draft dropped
                         │ discarding│ 409 ─▶ submitted elsewhere: open scans/[scanId]
                         └───────────┘
```

---

## 16. Scan status as the app shows it

The scan's own state machine is in contracts 3. The app maps each status, plus `open_question`, guided checks and the plan, to what the screen leads with:

| `status` | Extra condition | The screen leads with |
|---|---|---|
| `pending` | A local draft exists | **Continue scan**, and **Discard** |
| `pending` | No draft (another device, or the draft was lost) | "Not submitted": add photos here, then submit, or **Discard** |
| `queued` | | "Waiting for analysis" with the stage list |
| `analysing` | | Stage progress and findings as they arrive |
| `awaiting_answer` | | The question card |
| `diagnosed` | | The diagnosis, and "Writing the plan" |
| `diagnosed`, `planned` | `open_question` present | The diagnosis marked uncertain, the question at the top of the provisional plan |
| `planned` | A guided check `analysing` | The plan, and "Checking your soil photo" |
| `planned` | | The diagnosis and the plan |
| `failed` | A guided check `analysing` | The failure, and "Checking your soil photo"; the stream stays open |
| `failed` | `failure.retryable` | The failure with **Retry** (editors and owners) |
| `failed` | not retryable | The failure with **New scan** (editors and owners) |

Unknown statuses are shown as "Processing" (contracts 1, versioning). **Discard**, **Retry** and **New scan** are hidden from viewers and reviewers.

---

## 17. Scan event stream

```
                         open (screen mounted, foreground, scan not at rest)
   ┌──────┐ ───────────────────────────────────────────────▶ ┌────────────┐
   │ idle │                                                  │ connecting │ ◀───────────────┐
   └──────┘                                                  └────────────┘                 │
      ▲                                                200 │      │ 401 → refresh token,    │
      │ snapshot at rest                                   ▼      │ retry once; 404 → gone  │
      │                                              ┌───────────┐                          │
      │                  event, keepalive ─────────▶ │ streaming │                          │
      │                                              └───────────┘                          │
      │                    server closes │     │ network error,           │ app or tab   │
      │                                  ▼     │ 45 s of silence          ▼ hidden       │
      │                          ┌────────────┐ │                    ┌────────┐ visible   │
      │                          │ refetching │ │                    │ paused │ ──────────┤
      │                          │ the scan   │ │                    └────────┘           │
      │                          └────────────┘ ▼                                         │
      │                   at rest │   │ not  ┌──────────────┐                              │
      │                           ▼   └────▶ │ waiting      │ ── timer ────────────────────┤
      │                  ┌────────────────┐  │ (1, 2, 4…30 s│                              │
      └───────────────── │ closed:resting │  └──────────────┘                              │
                         └────────────────┘                                                │
                                 └── user starts late work, a species change or a retry, ──┘
                                     a push for this scan arrives, or the screen is focused
                                     and the refetched scan isn't at rest
```

The first `connecting` of a visit sends no `Last-Event-ID` (full replay); every later one sends the last applied `seq`. `gone` shows the not-found screen.

---

## 18. Chat message

```
 ┌───────────┐ send ┌─────────┐ text/event-stream ┌───────────┐ message_completed ┌──────────┐
 │ composing │ ───▶ │ sending │ ────────────────▶ │ streaming │ ────────────────▶ │ complete │
 └───────────┘      └─────────┘                   └───────────┘                   └──────────┘
                      │   │  ▲                       │        │                      ▲
                      │   │  │ network error:        │ error  │ stream drops         │
                      │   │  │ retry (same key)      ▼ event  ▼                      │
                      │   │  └──────────────    ┌─────────┐ ┌─────────────┐          │
                      │   │                     │ errored │ │ interrupted │ ─ resend (new key) ─▶ sending
                      │   │                     └─────────┘ └─────────────┘          │
                      │   └── 200 application/json { message } (replay) ──────────────┘
                      │ HTTP error (budget_exceeded, 4xx)
                      ▼
                 ┌─────────┐
                 │ failed  │
                 └─────────┘
```

---

## 19. Pending action card

```
 ┌──────────┐ confirm ┌────────────┐ 200 ┌───────────┐
 │ proposed │ ──────▶ │ confirming │ ──▶ │ confirmed │
 └──────────┘         └────────────┘     └───────────┘
   │      │               │ 409 current_plan_id or neighbour_removed ─▶ ┌───────┐ "Ask again" ─▶ composer
   │      │               │                                             │ stale │
   │      │               │ 409 plant_removed ─▶ ┌───────────────┐      └───────┘
   │      │               │                      │ plant_removed │
   │      │               │ 409 expired ─▶ expired└───────────────┘
   │      │               └ network ─▶ retry
   │      │ reject ─▶ ┌──────────┐
   │      │           │ rejected │
   │      │           └──────────┘
   │      └ expires_at passed ─▶ ┌─────────┐
   │                             │ expired │
   │                             └─────────┘
   └ role below editor: never shown
```

---

## 20. Garden map upload

```
 ┌───────┐ save ┌───────────┐ 200 ┌───────┐
 │ draft │ ───▶ │ uploading │ ──▶ │ saved │ ─▶ saveWorldMap(version)
 └───────┘      └───────────┘     └───────┘
    ▲  ▲            │      │
    │  │            │      │ 400 unknown_plant
    │  │            │      ▼
    │  │            │  ┌────────┐ remove pins, new key, same base_version
    │  │            │  │ pruned │ ──────────────────────────────────────▶ uploading
    │  │            │  └────────┘
    │  │            │ 409 current_map_version
    │  │            ▼
    │  │      ┌──────────┐ Replace, confirmed: base_version = current, new key ─▶ uploading
    │  │      │ conflict │
    │  │      └──────────┘
    │  └───────── Reload: read the map, start mapping again
    └──────────── tracking lost: start again
```

---

## 21. Sources

Each link was opened on 2026-10-15. The full list is in [README 21](README.md#21-sources).

| Flow | Documentation |
|---|---|
| 1, 13 | [Clerk Expo quickstart](https://clerk.com/docs/expo/getting-started/quickstart) · [`useSignIn`](https://clerk.com/docs/reference/hooks/use-sign-in) · [`useSSO`](https://clerk.com/docs/reference/expo/native-hooks/use-sso) · [MFA custom flow](https://clerk.com/docs/guides/development/custom-flows/authentication/multi-factor-authentication) · [Expo Router protected routes](https://docs.expo.dev/router/advanced/protected/) · [Google: OAuth in embedded browsers](https://developers.googleblog.com/upcoming-security-changes-to-googles-oauth-20-authorization-endpoint-in-embedded-webviews/) |
| 2 | [expo-location](https://docs.expo.dev/versions/v57.0.0/sdk/location/) |
| 3, 12 | [Expo linking](https://docs.expo.dev/linking/overview/) · [expo-linking](https://docs.expo.dev/versions/v57.0.0/sdk/linking/) |
| 4, 14, 15 | [expo-camera](https://docs.expo.dev/versions/v57.0.0/sdk/camera/) · [expo-image-picker](https://docs.expo.dev/versions/v57.0.0/sdk/imagepicker/) · [expo-image-manipulator](https://docs.expo.dev/versions/v57.0.0/sdk/imagemanipulator/) · [expo-file-system](https://docs.expo.dev/versions/v57.0.0/sdk/filesystem/) · [MDN `capture`](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Attributes/capture) · [Zustand persist](https://zustand.docs.pmnd.rs/reference/middlewares/persist) |
| 5, 8, 17 | [SSE format (WHATWG)](https://html.spec.whatwg.org/multipage/server-sent-events.html) · [`expo/fetch`](https://docs.expo.dev/versions/latest/sdk/expo/#expofetch-api) · [TanStack invalidation](https://tanstack.com/query/latest/docs/framework/react/guides/query-invalidation) |
| 9, 10, 18, 19 | [MDN Streams API](https://developer.mozilla.org/en-US/docs/Web/API/Streams_API) · [openapi-fetch](https://openapi-ts.dev/openapi-fetch/) |
| 11, 20 | [`ARWorldTrackingConfiguration`](https://developer.apple.com/documentation/arkit/arworldtrackingconfiguration) · [`ARWorldMap`](https://developer.apple.com/documentation/arkit/arworldmap) · [Expo Modules API](https://docs.expo.dev/modules/module-api/) |
| 12 | [Expo push overview](https://docs.expo.dev/push-notifications/overview/) · [sending notifications](https://docs.expo.dev/push-notifications/sending-notifications/) · [expo-notifications](https://docs.expo.dev/versions/v57.0.0/sdk/notifications/) |
