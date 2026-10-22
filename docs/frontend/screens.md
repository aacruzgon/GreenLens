# PDM Frontend Screens

> **Status:** Draft v0.4.10, 2026-10-22. v0.4.10: after the second review of #130, the photo slides only while the question is up and counts the labels; the soil checks are a native menu; a soil check finishes with its texture; a scan that can't load says so with Try again, and an offline one shows its cached record (3.3). v0.4.9: after the review of #130, the live analysis shows the snapshot until the replay catches up, moves the photo so the hotspots sit between the cards, puts a label above its oval when another is below, says "Plan updated" for a superseding plan, and opens the soil checks from the More button (3.3). v0.4.8: the live analysis as built in F3: the stage card and its run log, hotspots, the species chip, "Not sure yet", picture answers by `Question.indicator_id`, the ring and the pill, and the result route (3.3). v0.4.7: capture's question and guided modes in full: the photo before Send, the jar test's task and its row opening the camera (3.2). v0.4.6: a password reset by email code, from sign-in and from a sign-up whose email is taken (1, 3.10). v0.4.5: capture's mode bar is Liquid Glass, solid with Reduce Transparency. v0.4.4: capture's pill asks for a missing needed shot before the optional ones, and a failed removal keeps the shot with **Try again**. v0.4.3: capture as built in F2, after the Camera app: the mode bar and the coverage dial replace the shot strip, the instruction drops from the top, and a full mode's shutter retakes its shot. v0.4.2: sign-up takes a phone number, and sign-in asks for a code by text. v0.4.1: no authenticator-app second factor.
> v0.4 applies the third audit: Google and Apple sign-in are hidden only in embedded web views, and the note delete rule names who deletes which note.
> v0.3 applies the second audit and contracts v0.8.1:
>   - **New-plant sheet:** added, with `POST /plants`.
>   - **Scan screen:** its writes include Discard and New scan, and feedback is hidden until submit.
>   - **Timeline:** failed rows show their failure code.
>   - **Species photos:** credits are on the card and the picker.
>   - **Diagnosis:** with `low_evidence`, the closer photo answers the open question.
>   - **Capture:** discarding warns about the jar-test task.
>   - **Sign-in:** the banner targets embedded web views only.
>
> v0.2 applied the first audit and used [contracts](../architecture/contracts.md) v0.8:
>   - **Diagnosis wording:** follows `status` and `low_evidence`, with no confidence badge.
>   - **Garden home:** thumbnails come from `Plant.latest_scan`, the weather from `GET /gardens/{id}/weather`, and garden notes are shown.
>   - **Species:** the card has reference photos, and a new species picker uses `GET /species`.
>   - **Beneficials:** shown with what they eat.
>   - **Roles:** invites are owner-only, and Retry, New scan and alert actions are hidden from viewers.
>   - **Feedback:** scan feedback and message feedback have their UI.
>   - **Membership:** leaving warns that private chats are deleted.
>   - **Capture:** in phone browsers it uses the system camera, and a pending scan can be discarded.
>   - **Sign-in:** in-app browsers get a banner.
>   - **Navigation map:** route names made consistent.
>
> v0.1: Navigation map, screen inventory, wireframes, screen rules, components and wording.

Companion to the [frontend architecture](README.md) and the [flows](flows.md). What each screen reads and sends is defined in the [data contracts](../architecture/contracts.md); the product goals behind it are in the [system architecture](../architecture/README.md), sections 1 and 6.6.

Wireframes show layout and content, not visual design. They are drawn at phone width; the web build uses the same layouts, centred at a readable width on a desktop.

## Contents

1. [Navigation map](#1-navigation-map)
2. [Screen inventory](#2-screen-inventory)
3. [Main screens](#3-main-screens)
   - [3.1 Garden home](#31-garden-home)
   - [3.2 Capture](#32-capture)
   - [3.3 Scan results](#33-scan-results)
   - [3.4 Plan](#34-plan)
   - [3.5 Plant detail and timeline](#35-plant-detail-and-timeline)
   - [3.6 Chat](#36-chat)
   - [3.7 Tasks and alert inbox](#37-tasks-and-alert-inbox)
   - [3.8 Garden map](#38-garden-map)
   - [3.9 Settings, members and invites](#39-settings-members-and-invites)
   - [3.10 Sign-in and onboarding](#310-sign-in-and-onboarding)
   - [3.11 Species picker and scan feedback](#311-species-picker-and-scan-feedback)
4. [Rules every screen follows](#4-rules-every-screen-follows)
5. [Shared components](#5-shared-components)
6. [Wording for uncertain results](#6-wording-for-uncertain-results)
7. [Sources](#7-sources)

---

## 1. Navigation map

```
                              ┌──────────────────────┐
                              │ splash (Clerk loads) │
                              └──────────┬───────────┘
                    signed out           │           signed in
              ┌──────────────────────────┴──────────────────────────┐
              ▼                                                     ▼
   ┌─────────────────────┐                               ┌────────────────────┐   no garden   ┌────────────┐
   │ (auth)              │                               │ SessionGate        │ ────────────▶ │ onboarding │
   │ sign-in · sign-up · │                               │ /me · /gardens     │               └────────────┘
   │ reset-password ·    │                               └─────────┬──────────┘
   │ verify (codes)      │                                         │
   └─────────────────────┘                                         ▼
                                 ┌─────────────────────────── TABS ───────────────────────────┐
                                 │  Garden            Tasks             Chat          Settings │
                                 └────┬─────────────────┬─────────────────┬───────────────┬────┘
                                      │                 │                 │               │
   ┌──────────────────────────────────┼─────────┐       │                 │               ├─▶ gardens/new
   ▼                                  ▼         ▼       ▼                 ▼               ├─▶ gardens/[gardenId]/edit
 plants/[plantId] ◀── pin tap ── gardens/[gardenId]/map tasks/edit   chat/[conversationId]├─▶ gardens/[gardenId]/members
   │  │   │                           │         (sheet)   ▲                │               └─▶ account/delete
   │  │   └─▶ plants/[plantId]/species│ editor, LiDAR     │                │   Garden [+] ─▶ plants/new (sheet)
   │  │       (sheet)                 ▼                   │                │ rescan_requested
   │  │ timeline entry   gardens/[gardenId]/map-capture   │                │ (editors, owners)
   │  ▼                                                   │                │
   │ scans/[scanId] ──── "Ask about this result" ─────────┼────────────────┘
   │   │   │   │                                          │
   │   │   │   ├─▶ scans/[scanId]/result (the diagnosis and plan, F4)
   │   │   │   └─▶ scans/[scanId]/feedback (sheet, reviewer and up)
   │   │   └─▶ capture/[scanId]/guided/[kind] ◀───────────┘ jar-test task (task.scan_id)
   │   └─▶ capture/[scanId]/question/[questionId]
   │
   └── "Scan" ──▶ capture/[scanId] ◀── "Scan" on garden home, chat, task
                       │ submit ──▶ scans/[scanId]

 Outside the guards: invite/[code] ──▶ (sign-in if needed) ──▶ Garden tab
 Push notifications open routes from their payload (flows 12).
```

---

## 2. Screen inventory

| Route | Feature | Shows | Reads | Writes | Who |
|---|---|---|---|---|---|
| `(auth)/sign-in` | auth | Email and password; Google and Apple except in embedded web views (`in_app`, [README 13](README.md#13-platform-adapters-and-the-lidar-module)) | Clerk | Clerk | Signed out |
| `(auth)/sign-up` | auth | New account: email, phone number and password | Clerk | Clerk | Signed out |
| `(auth)/reset-password` | auth | Forgot password: email, then the emailed code and a new password; the code by text follows | Clerk | Clerk | Signed out |
| `(auth)/verify` | auth | A 6-digit code: the sign-up email and phone codes, the code by text at sign-in, or the email code that confirms a new device | Clerk | Clerk | Signed out |
| `(app)/onboarding` | auth, gardens | Display name, first garden, location consent | `GET /me` | `PATCH /me`, `POST /gardens` | Signed in, no garden |
| `(tabs)/index` | gardens | Garden home (3.1) | `GET /gardens/{id}`, `GET /gardens/{id}/weather`, `GET /plants`, `GET /tasks`, `GET /tasks?source=forecast_alert`, `GET /memories?garden_id=`, `GET /gardens/{id}/map` | `PATCH /tasks/{id}`, `POST /memories`, starts a scan | All roles; task changes, notes and "Scan" for editors and owners |
| `(tabs)/tasks` | tasks | Tasks and the alert inbox (3.7) | `GET /tasks` | `POST /tasks`, `PATCH /tasks/{id}`, `DELETE /tasks/{id}` | All read; changes for editors and owners |
| `tasks/edit` (sheet) | tasks | New task, or edit a user or chat task | `GET /plants` | `POST /tasks`, `PATCH /tasks/{id}` | Editor, owner |
| `(tabs)/chat` | chat | The caller's conversations in the active garden | `GET /conversations?garden_id=` | `POST /conversations` | All |
| `(tabs)/settings` | account, gardens, members | Account, gardens, notifications (3.9) | `GET /me`, `GET /gardens` | `PATCH /me` | All |
| `gardens/new` | gardens | Name and location | | `POST /gardens` | Signed in |
| `gardens/[gardenId]/edit` | gardens | Name, location, delete garden | `GET /gardens/{id}` | `PATCH /gardens/{id}`, `DELETE /gardens/{id}` | Owner |
| `gardens/[gardenId]/members` | members | Members, roles, invites (3.9) | `GET /gardens/{id}/members`; `GET /gardens/{id}/invites` for owners only | `POST` and `DELETE` invites, `PATCH` and `DELETE` members | Members: all read. Invites, role changes and removal: owner. Leaving: any member except the last owner |
| `gardens/[gardenId]/map` | map | Top-down map (3.8) | `GET /gardens/{id}/map`, `GET /plants` (all pages) | | All |
| `gardens/[gardenId]/map-capture` | map | AR mapping session | `GET /gardens/{id}/map`, `GET /plants` (all pages) | `PUT /gardens/{id}/map` | Editor, owner; LiDAR devices only |
| `plants/new` (sheet) | plants | Name, and optionally a species from the picker | `GET /species?q=` | `POST /plants` | Editor, owner |
| `plants/[plantId]` | plants | Plant detail, timeline, growth curve (3.5) | `GET /plants/{id}`, timeline, measurements, `GET /memories?plant_id=` | `PATCH /plants/{id}`, `DELETE /plants/{id}`, `POST /memories`, `DELETE /memories/{id}` | All read; edits and notes for editors and owners; deleting the plant or a shared note for owners |
| `plants/[plantId]/species` (sheet) | plants | Species picker (3.11) | `GET /species?q=` | `POST /plants/{id}/species` | Editor, owner |
| `capture/[scanId]` | capture | Guided capture (3.2) | Capture draft, `GET /scans/{id}` | `POST` and `DELETE /scans/{id}/images`, `POST /scans/{id}/submit`, `DELETE /scans/{id}` | Editor, owner |
| `capture/[scanId]/question/[questionId]` | capture | One photo for a question | `GET /scans/{id}` | `POST /scans/{id}/images` with `question_id` | Editor, owner |
| `capture/[scanId]/guided/[kind]` | capture | Squeeze or jar test | `GET /scans/{id}` | `POST /scans/{id}/images`, `POST /tasks` with `scan_id` (jar), `PATCH /tasks/{id}` | Editor, owner |
| `scans/[scanId]` | scans | Live progress and results (3.3, 3.4) | `GET /scans/{id}`, `GET /scans/{id}/events` | `POST /scans/{id}/answers`, `POST /plants/{id}/species`, `PATCH /tasks/{id}`, `POST /scans/{id}/submit` (Retry), `POST /scans` (New scan), `DELETE /scans/{id}` (Discard, while `pending`) | All read; the writes for editors and owners |
| `scans/[scanId]/feedback` (sheet) | scans | Scan feedback (3.11) | | `POST /scans/{id}/feedback` | Reviewer, editor, owner; only once the scan is submitted (contracts 3) |
| `chat/[conversationId]` | chat | Conversation (3.6) | `GET /conversations/{id}` | Messages, images, cards, memories, message feedback | The conversation's owner |
| `account/delete` | account | What is deleted, then deletion | | `DELETE /me` | Signed in |
| `invite/[code]` | members | Invite acceptance | | `POST /invites/{code}/accept` | Anyone with the link; sign-in first |

---

## 3. Main screens

### 3.1 Garden home

```
┌─────────────────────────────────────┐
│ Back garden ▾              [map] [+]│  garden switcher · map · new plant (editor) → plants/new
├─────────────────────────────────────┤
│ ☁ 14 °C · 92% humidity · rain 3 mm  │  WeatherStrip from GET /gardens/{id}/weather
│ Frost possible Thu                  │  forecast frost_risk
├─────────────────────────────────────┤
│ ⚠ Frost tonight. Cover the tomatoes │  alert inbox: open forecast_alert tasks
│                        [Done] [View]│  Done for editors and owners
├─────────────────────────────────────┤
│ Plants                              │
│ ┌─────────┐ ┌─────────┐ ┌─────────┐ │
│ │ [photo] │ │ [photo] │ │ [ leaf ]│ │  Plant.latest_scan.thumbnail_url; an icon without one
│ │Tomato 1 │ │Basil    │ │Pepper   │ │
│ │● sick   │ │● healthy│ │● watch  │ │  StatusChip: word + colour
│ └─────────┘ └─────────┘ └─────────┘ │
├─────────────────────────────────────┤
│ Today                               │  open tasks due today or overdue
│ ☐ Remove the lowest infected leaves │
│ ☐ Check Tomato 2's lower leaves     │
├─────────────────────────────────────┤
│ Garden notes                        │  memories with no plant: shared, plus the caller's own
│ · Everything is in containers       │  [+ Note] for editors and owners
├─────────────────────────────────────┤
│        [ 📷  Scan a plant ]         │  editor, owner
└─────────────────────────────────────┘
  Garden    Tasks    Chat    Settings
```

- **Scan a plant** asks which plant (or a new one by name) and calls `useStartCapture`.
- The weather strip shows `fetched_at` as "Updated 10:00". A `503` hides it with "Weather unavailable".
- `soil_profile_status: "pending"` shows "Soil data loading" under the garden name.
- A plant whose `latest_scan` is `pending` shows "Scan not submitted" on its card.
- **Empty garden:** "Add your first plant by scanning it", with the scan button for editors and owners.

### 3.2 Capture

```
┌─────────────────────────────────────┐   Phone browser variant
│ (✕)       Scan · Tomato 2   Discard │   ┌─────────────────────────────────────┐
│ ┌─────────────────────────────────┐ │   │ ✕  Scan · Tomato 1        [Discard] │
│ │ Leaf close-up                   │ │   ├─────────────────────────────────────┤
│ │ Fill the frame with one affected│ │   │ Whole plant                         │
│ │ leaf, about 30 cm away. Tap the │ │   │ Step back about 1 m and get the     │
│ │ leaf to focus.                  │ │   │ whole plant in the frame.           │
│ │ [▣ ✕]                           │ │   │                                     │
│ │              ───                │ │   │       [ 📷  Take photo ]            │  launchCameraAsync, in the press handler
│ └─────────────────────────────────┘ │   │       [ Choose from library ]       │
│       ┌──                   ──┐     │   ├─────────────────────────────────────┤
│         [ camera, full screen ]     │   │ [✓ Whole] [+ Leaf] [+ Leaf] …       │
│       └──                   ──┘     │   ├─────────────────────────────────────┤
│     ◉ 32 cm · good distance         │   │                        [ Analyse ]  │
│( 1 of 2 needed shots · Add a leaf… )│   └─────────────────────────────────────┘
│                                     │
│   [▤]        (( ◯ ))      [Analyse] │   LiDAR: LidarCameraView with the distance hint.
│ ( •WHOLE PLANT  CLOSE-UP  UNDERSIDE │   Other phones: expo-camera. Desktop: file picker
│   SOIL  FRUIT )                     │   and drag and drop.
└─────────────────────────────────────┘
```

Capture is always dark and follows the Camera app ([design 3.1](../design/design.md#31-conservatory)): the camera fills the screen, and the controls on it are neutral, white and solid dark, never tinted.

- The **mode bar** has one mode per shot kind: WHOLE PLANT · CLOSE-UP · UNDERSIDE · SOIL · FRUIT, in a dark Liquid Glass pill, the chosen one in white on a lighter pill and kept centred. Where iOS has no Liquid Glass, or Reduce Transparency is on, the pill is solid dark ([design 3.1](../design/design.md#31-conservatory)). FRUIT is `flower_fruit`, and its instruction says flower or fruit. A dot marks a mode with a stored shot, a ring a shot on its way, and `!` one to retake. It replaces the strip of shot chips. CLOSE-UP holds two shots.
- The **instruction card** drops from the top with a grabber: the chosen shot's title and instruction, the mode's shots as thumbnails, each stored one with **✕** to remove it, and, when a shot didn't store, the server's message with its hint's icon. A shot the quality check refused is retaken with the shutter; a failed upload has **Try again**, after the app has already retried it twice under the same key. A removal that still fails after two retries leaves the shot stored, with the message and **Try again**; a retake waiting on it is dropped.
- The **framing guide**'s corners sit where the shot's subject goes. The **distance chip** ("32 cm · good distance") appears once the LiDAR module sends distances (roadmap F5).
- The **shutter** is white, with the **coverage dial** around it: one segment per needed shot, the whole plant and a close-up, each turning white once stored. The pill above says "1 of 2 needed shots" and what to add next: a missing needed shot first ("Add a leaf close-up"), so the pill and the dial agree, then the optional shots' instructions. Until the whole plant is stored it says "Start with the whole plant". On a mode whose slots are full, the shutter retakes its last shot: the stored image is deleted, then the new one uploads (flows 4, replacing).
- **Library** sits left of the shutter, **Analyse** right of it, white on the black. **Analyse** is enabled when `whole_plant` is stored and nothing is uploading.
- **✕** closes capture and keeps the draft for later. **Discard** asks "Discard this scan? Its photos will be deleted.", adding "and its jar-test task" when one was started, and deletes the pending scan.
- **Camera access** is asked by the system's own alert when capture opens, with no screen of ours before it ([HIG Privacy](https://developer.apple.com/design/human-interface-guidelines/privacy#Pre-alert-screens-windows-or-views)). After a refusal, the preview says why the camera is needed and offers **Open Settings**; Library still works.
- **Question mode** has one slot, the question's `photo_hint` as the instruction and **Send** instead of Analyse. The title bar says "Photo for a question", and the card leads with the question's text. The photo shows after the shutter; the shutter then retakes it, and **Send** uploads it. There is no **Discard**: nothing is stored before Send. A question that is no longer open shows "This question is no longer open.", and one that isn't a photo request "This question doesn't need a photo.", each with **Close**. ✕ is off while the photo is sending.
- **Guided mode** shows the squeeze or jar steps first, numbered, with **Start**, then one slot, sent with **Send** like question mode. For the jar test, Start adds the task "Photograph the jar test", and the camera's card then says it is on the user's tasks. The task's row opens this screen at the camera, past the steps. Only a user's task with a scan link is the jar test's; a plan's task opens a new scan as before. Start waits until the scan's tasks are all loaded, so it never adds a second jar task. A failed scan takes no check and says so.

### 3.3 Scan results

```
┌─────────────────────────────────────┐
│ ←  Tomato 1 · Oct 18                │
│ ● Analysing                         │  StatusChip from the reducer
├─────────────────────────────────────┤
│ ✓ Plant found   ✓ Leaves   ✓ Species│  StageProgress: done · running · skipped
│ ◐ Checking 9 possible problems      │  candidates_selected
├─────────────────────────────────────┤
│ ┌─────────────────────────────────┐ │
│ │ [photo]  ┌──────┐               │ │  PhotoWithRegions: boxes + masks;
│ │          │ spot │               │ │  tapping a finding highlights its region
│ │          └──────┘               │ │
│ └─────────────────────────────────┘ │
│  ● ○ ○  (whole · close-up · soil)   │  photo strip
├─────────────────────────────────────┤
│ [photo] Is this a tomato?           │  species card: one tap when the models agree
│ Photo: A. Author, CC BY-SA 4.0      │  reference_image.credit, always with the photo
│                     [Yes] [Change]  │  (top 3 with photos when they don't, flows 7)
├─────────────────────────────────────┤
│ Do the spots have rings, like a     │  question card (indicator)
│ target?                             │
│      [Yes]   [No]   [Not sure]      │  editors and owners
├─────────────────────────────────────┤
│ Early blight                        │  diagnosis, worded from status (section 6); no badge
│ ✓ Concentric rings in the spots     │  matched indicators
│ ✓ Started on the lower leaves       │
│ ✓ Spots 8–12 mm across   measured   │  measured badge: answered by LiDAR
│ ✗ Yellow halo                       │  contradicted
│ Risk: fungal spread · high          │  risks with drivers
│   4 days over 90% humidity forecast │
│ Based on: your notes · the photos · │  evidence_basis: user input, photo, modelled
│   weather data                      │
│ Sources: OMAFRA · Cornell           │  CitationList
├─────────────────────────────────────┤
│ Also found                          │
│ 🐞 Ladybug larva · beneficial  high │  organism finding with its ConfidenceBadge
│   Leave it: it eats aphids          │  from preys_on; "Leave it" alone when empty
│ Soil: dry (high) · crusted (medium) │  soil list, never a score
│ Leaf 9.4 cm (high) · plant 62 cm    │  measurements with units and bands
├─────────────────────────────────────┤
│ ☁ Weather at the scan               │  WeatherStrip from ScanDetail.weather
├─────────────────────────────────────┤
│ Plan (3.4)                          │
├─────────────────────────────────────┤
│ [ Ask about this result ] [Feedback]│  chat anchored to the scan · reviewer and up
└─────────────────────────────────────┘
```

- **While analysing**, sections appear as their events arrive; sections with nothing yet are not shown, so the screen grows rather than showing empty boxes.
- **`low_evidence`** replaces the diagnosis block with "We couldn't tell what's wrong from these photos", **Take a closer photo** and "or ask your local extension office". The button answers the open `photo_request` question when there is one, and starts a new scan otherwise (editors and owners).
- **Invasive organisms** show "How to report" with the `reporting_url`. The app never reports anything itself.
- **`failed`** replaces the results with the failure message and **Retry** or **New scan**, for editors and owners.
- **Viewers** see everything except the answer buttons, the species buttons, Retry and New scan. Reviewers also get **Feedback**, which appears once the scan is submitted.

**The live analysis, as built in F3** ([design 3.1](../design/design.md#31-conservatory)). Until the plan is ready, `scans/[scanId]` is the camera's layout, always dark, rather than the list above:

- **Opening a scan** that isn't at rest shows its snapshot until the stream's replay reaches the snapshot's `last_event_seq`, then the live state ([README 10](README.md), step 4); a replayed finding doesn't animate and a replayed event runs no haptic. Offline, the cached record shows instead, with nothing counted as live until a fresh one arrives; with nothing cached, "Couldn't load this scan" and **Try again**.
- **The photo just taken** fills the screen, held still. The scan's photos sit in a solid dark strip at the bottom; it follows the photo with the newest finding, and a tap on a thumbnail shows that photo instead.
- **The stage card** drops from the top: the running step with its seconds and the next ("Checking telltale signs · 6 s · Next: weighing the evidence"). The 15 pipeline stages read as nine steps, the mockup's: finding the plant, identifying the species, looking for disease, measuring spots (only with depth), checking weather and soil, gathering likely problems, checking telltale signs, weighing the evidence, writing your plan. A tap opens the run log under it, with a tick, a note and the time per step; each new tick has a haptic.
- **Hotspots** mark findings on the photo: an oval inside the region's box with a white dot, and a chip below it. A disease finding reads as its symptom ("Leaf spots", "Affected area") with the measured spot size, never the problem's name; an organism keeps its name. Several findings in one box share one hotspot. A label goes above its oval when below would cover another label. While the question is up, with "Not sure yet" over the photo, the photo slides (at most 30% of its height) so the hotspots and their labels sit between the cards; otherwise it stays put. The mask isn't drawn yet: the oval is the box's centre at 70% of its size.
- **"Tomato · Change"** is a chip on the photo while the run has a species suggestion or the plant's species is `suggested`, hidden while the question shows. **Change** opens `plants/[plantId]/species`.
- **"Not sure yet: A or B"** drops under the stage card with both candidates as two equal plates while an indicator question is open. **The question** sits low and compact above the controls; for an indicator with art (`Question.indicator_id`, so far `concentric_rings`), Yes and No are picture answers, otherwise text pills, with **Not sure** under them. After an answer the card goes and "You answered …" shows under the stage card.
- **The controls:** back at the left, the glowing ring in the middle while the run goes on, and **Check your soil** (squeeze or jar test, editors and owners, not on a failed scan) at the right, as a menu on the More button (SwiftUI's `Menu`; an alert on Android and the web). The check's photo shows "Checking your soil photo" until its texture comes back. When the plan is ready ("Plan ready", or "Plan updated" when it replaced another), the ring gives way to **See the diagnosis and plan**, which opens `scans/[scanId]/result` (F4 builds it).
- **Endings:** a failure card with **Retry** or **New scan**; a `low_evidence` card with **Take a closer photo**; after **Not sure**, "Not sure yet: A or B" and the provisional-plan note stay above the pill. A jar test whose photo gave a texture marks its task done.

### 3.4 Plan

```
┌─────────────────────────────────────┐
│ ⚠ Provisional plan                  │  provisional = true
│ We're not sure yet. These steps are │
│ safe for every likely problem.      │
│ ┌─────────────────────────────────┐ │
│ │ Do the spots have rings?        │ │  the open question, at the top
│ │    [Yes]  [No]  [Not sure]      │ │
│ └─────────────────────────────────┘ │
├─────────────────────────────────────┤
│ Summary text from the plan          │
├─────────────────────────────────────┤
│ ☐ Remove the lowest infected leaves │  step: title, timing, category
│   Today · cultural            [1]   │  [1] citation
│ ☐ Check for new spots               │
│   Every 3 days until Nov 1 · monitor│  recurring
│ ○ After the next rain, remove       │
│   splashed leaves · cultural        │  on_trigger: no task until it fires
│ ☐ Apply a sulfur fungicide          │
│   By Oct 22 · chemical              │
│   ⓘ Use only a product registered   │  server's safety note, never edited
│   for this use where you live, and  │
│   follow its label exactly.         │
│ ☐ Check Tomato 2's lower leaves     │  neighbour step (plant_id)
│   40 cm away · monitoring           │  "plant removed" when it no longer exists
├─────────────────────────────────────┤
│ Watch for                           │
│ · New spots on upper leaves in 5 days│
├─────────────────────────────────────┤
│ Your constraints: No copper products│  from confirmed chat revisions
├─────────────────────────────────────┤
│ Sources                             │
│ [1] OMAFRA, Early blight of tomato ↗│
└─────────────────────────────────────┘
```

- Checking a step's box completes its task (`PATCH /tasks/{id} { done: true }`), for editors and owners. Viewers see the boxes read-only.
- After `plan_superseded`, a notice says why ("Updated after your answer", "Updated for pepper", "Updated after the soil check", "Updated from chat").

### 3.5 Plant detail and timeline

```
┌─────────────────────────────────────┐
│ ←  Tomato 1                 [⋯]     │  ⋯: rename, status (editor), delete (owner)
│ Solanum lycopersicum · confirmed    │  species and species_status; "Change" opens the picker
│ ● sick                              │
├─────────────────────────────────────┤
│ Height (cm)                         │  LineChart: MeasurementPoint[]
│  70 ┤                    ●          │  only when the plant has points
│  50 ┤          ●    ●               │  medium confidence or more
│  30 ┤ ●   ●                         │
│     └──────────────────────────     │
│       Sep 20         Oct 18         │
├─────────────────────────────────────┤
│ Timeline                            │
│ Oct 18 [thumb] Early blight · planned│  TimelineEntry rows → scans/[scanId]
│ Oct 02 [thumb] Healthy · planned    │  thumbnail_url; no image when absent
│ Sep 20 [ — ]   Failed: no plant found│  failure_code
├─────────────────────────────────────┤
│ Notes                               │  shared memories + the caller's own
│ · It's in a container (shared)  [✕] │  ✕: an owner for a shared note; the author for their own unshared note
│ [+ Note]                            │  editor, owner
├─────────────────────────────────────┤
│ [ Scan ]   [ Chat about this plant ]│
└─────────────────────────────────────┘
```

- A **`pending` scan** shows "Continue scan" (with a local draft) or "Not submitted" at the top of the timeline, with **Discard**.
- The growth curve is shown on every device; only LiDAR scans add points.

### 3.6 Chat

```
┌─────────────────────────────────────┐
│ ←  About: Tomato 1, Oct 18 scan     │  anchor
├─────────────────────────────────────┤
│              Should I water today? ▕│  user bubble
│ ▏◐ Checking the forecast            │  tool chip (label)
│ ▏Rain is forecast tomorrow, and the │  streaming text
│ ▏soil is already moist at 9 cm…     │
│ ▏Sources: Open-Meteo · OMAFRA       │
│ ▏                         [👍] [👎] │  message feedback
│ ┌─────────────────────────────────┐ │
│ │ Add a task?                     │ │  PendingAction card (editor, owner)
│ │ Check the soil on Thursday      │ │
│ │         [Not now]  [Add task]   │ │
│ └─────────────────────────────────┘ │
│ ┌─────────────────────────────────┐ │
│ │ Remembered: it's in a container │ │  memory_saved
│ │       [Undo]  [Share with garden]│ │  Share: editor, owner
│ └─────────────────────────────────┘ │
│ ▏Interrupted              [Resend]  │  interrupted message
├─────────────────────────────────────┤
│ [📷] Ask about your plants…   [Send]│  up to 3 photos, 2,000 characters
└─────────────────────────────────────┘
```

- The conversation list (Chat tab) groups conversations by anchor (garden, plant, scan), newest first. **New chat** opens one anchored to the garden.
- A photo that fails its upload checks shows the reason under its thumbnail, with **Retry** and **Remove**.
- A thumbs-down opens an optional note before sending.

### 3.7 Tasks and alert inbox

```
┌─────────────────────────────────────┐
│ Tasks          [Open ▾] [Alerts]    │  state filter · alert inbox toggle
├─────────────────────────────────────┤
│ Overdue                             │  grouped by due date in the garden's time zone
│ ☐ Remove infected leaves · Tomato 1 │  source chip: plan
├─────────────────────────────────────┤
│ Today                               │
│ ☐ Cover tomatoes: frost tonight     │  forecast_alert
│ ☐ Photograph the jar test   [📷]    │  user, with scan_id: opens the jar capture
├─────────────────────────────────────┤
│ No date                             │
│ ☐ Order mulch                       │
├─────────────────────────────────────┤
│                         [ + Task ]  │  editor, owner → tasks/edit
└─────────────────────────────────────┘
```

- Swipe actions, for editors and owners: **Done** for every task; **Dismiss** for plan and alert tasks; **Delete** only for user and chat tasks, which can also be edited (contracts 9, tasks). Viewers see the list read-only.
- A repeating task shows its interval; completing it shows the next one.

### 3.8 Garden map

```
┌─────────────────────────────────────┐
│ ←  Map · v5             N↑   [Edit] │  Edit: editor on a LiDAR device
├─────────────────────────────────────┤
│  ┌──────────── Bed A ───────────┐   │  beds: polygons with names and areas
│  │  ●Tomato 1    ●Tomato 2      │   │  pins coloured by plant status
│  │       ◌ canopy               │   │  canopy radius ring when sent
│  └──────────────────────────────┘   │
│        ┌── Bed B ──┐                │
│        │ ●Basil    │                │
│        └───────────┘                │
│                          ── 1 m     │  scale bar
├─────────────────────────────────────┤
│ Tomato 1 · sick · 40 cm to Tomato 2 │  selected pin; tap again → plant page
└─────────────────────────────────────┘
```

- **No map yet:** "Map your garden from an iPhone or iPad with LiDAR" for everyone; editors on such a device get **Start mapping**.
- **Mapping session:** the AR view full screen, with "Tap the bed's corners", "Tap a plant to pin it", the tracking state, and **Save**. The "Which plant?" picker lists every plant of the garden, all pages loaded.

### 3.9 Settings, members and invites

```
┌─────────────────────────────────────┐      ┌─────────────────────────────────────┐
│ Settings                            │      │ ←  Members · Back garden            │
├─────────────────────────────────────┤      ├─────────────────────────────────────┤
│ Alex Doe                     [Edit] │      │ Alex Doe · owner · since Oct 16     │
│                                     │      │ Sam Lee · editor        [Role ▾][✕] │  owner controls
│ Share my location: on               │      │ Jo Park · viewer        [Role ▾][✕] │
├─────────────────────────────────────┤      ├─────────────────────────────────────┤
│ Gardens                             │      │ Invites                    (owner)  │  owners only
│ Back garden · owner       [Manage]  │      │ editor · expires Oct 23  [Share][✕] │
│ Allotment · editor        [Members] │      │ [ + Invite ]                        │
│ [+ New garden]                      │      ├─────────────────────────────────────┤
├─────────────────────────────────────┤      │ [ Leave garden ]                    │  any member, not the last owner
│ Notifications                 [on]  │      └─────────────────────────────────────┘
├─────────────────────────────────────┤
│ [ Sign out ]                        │
│ [ Delete account ]                  │
└─────────────────────────────────────┘
```

- **Manage** (owners) opens `gardens/[gardenId]/edit`, which shows the garden's location ("Toronto, ON · about 1 km") and lets the owner change it or delete the garden.
- **Notifications** is shown only in builds with push.
- **Share my location** is `location_consent`. Turning it off explains that every garden the user owns will be rounded to about 10 km (contracts 9, account).
- **Leave garden** and **✕** (remove) ask first: "Their private chats in this garden will be deleted. Their scans stay with the garden."

### 3.10 Sign-in and onboarding

```
┌─────────────────────────────────────┐      ┌─────────────────────────────────────┐
│ ┌─────────────────────────────────┐ │      │ Your garden                         │
│ │ For Google or Apple sign-in,    │ │      │ Name  [ Back garden            ]    │
│ │ open this page in Safari or     │ │      │                                     │
│ │ Chrome.          [Copy link]    │ │      │ Where is it?                        │
│ └─────────────────────────────────┘ │      │ [ Use my location ]                 │
│   (embedded web views only)         │      │   ○ exact  ● about 1 km             │
│               PDM                   │      │ or postal code or city              │
│ Email     [                    ]    │      │   [ M5V 2T6 or Guelph, ON      ]    │
│ Password  [                    ]    │      │ We use it for weather and soil.     │
│           [ Sign in ]               │      │ Only the garden has a location.     │
│         Forgot password?            │      │                                     │
│ ─────────── or ───────────          │      │            [ Create garden ]        │
│ [ Continue with Google ]            │      └─────────────────────────────────────┘
│ [ Continue with Apple ]             │        (hidden in embedded web views)
│ New here? Create an account         │
└─────────────────────────────────────┘
```

- **Forgot password?** opens "Reset your password" with the typed email: "Send code", then the 6-digit code and a new password, with "Send a new code" until the code is accepted. Once the password has changed, the subtitle says "Your password is changed" and the button reads **Continue**; a reset that timed out goes back to the email. Resetting signs out the account's other sessions, and the code by text still follows ([flows 1](flows.md#1-app-start-and-sign-in)).
- **Sign-up with a taken email** shows "You already have an account" with **Reset password**, instead of the code steps. A taken phone number shows Clerk's message, since its account's email is unknown. Sign-up links back with "Already have an account? Sign in".
- **Verify** shows a 6-digit code field with "Send a new code": the sign-up email code, then the phone's; the code by text at every sign-in; or the email code that confirms a new device. In development builds, the phone steps show Clerk's test numbers (+1 555 555 0100 to 0199, code 424242).
- The postal-code-or-city field sends `postal_code` when it looks like a Canadian postal code, otherwise `city`.

### 3.11 Species picker and scan feedback

```
┌─────────────────────────────────────┐      ┌─────────────────────────────────────┐
│ Which plant is this?                │      │ Was this result right?              │
│ [ Search: tom               ]       │      │  ( ) Correct  ( ) Wrong  ( ) Unsure │  verdict
├─────────────────────────────────────┤      │ What was it? [ pick or type    ]    │  corrected_label: a problem
│ [photo] Tomato                      │      │                                     │  slug or free text
│   Solanum lycopersicum              │      │ Notes [                        ]    │
│ [photo] Tomatillo                   │      │                     [ Send ]        │
│   Physalis philadelphica            │      └─────────────────────────────────────┘
├─────────────────────────────────────┤        reviewer, editor, owner
│ Not listed? [ Type its name ]       │  custom_name, at most 80 characters
└─────────────────────────────────────┘
  editor, owner; each photo with its credit
```

- The picker searches `GET /species?q=` as the user types, and lists `full` species first.
- Each photo shows its credit beneath it ("Photo: A. Author, CC BY-SA 4.0"); a tap opens it larger.
- The same picker, without a plant, fills the species of a new plant (`plants/new`).

---

## 4. Rules every screen follows

| State | Rule |
|---|---|
| Loading | Skeletons in the shape of the content, never a full-screen spinner after the first load. Cached data shows at once and refreshes in place |
| Empty | One sentence that says what goes here and the one action that fills it, if the role allows it |
| Error | The `ErrorState` for the code (README 9, error handling): what happened, what to do, and `request_id` in small text for `internal` |
| Not found | A full screen: "We can't find this", with a way home. The same for anything missing or not visible to the user |
| Offline | A banner; cached screens stay readable; actions that need the network are disabled with "You're offline" |
| Role | Actions the role can't use are hidden (README 9, roles in the UI). Viewers never see an action that would fail |
| Capability | Features the build can't offer are hidden or explained once ("Mapping needs an iPhone or iPad with LiDAR"), never shown broken |
| Signed URLs | Images that fail to load refetch their record once (README 9, signed image URLs) |
| Long lists | Lists page as the user scrolls (at most 100 a page, contracts 1); pickers that must show everything load every page |

---

## 5. Shared components

| Component | Layer | Used on | Built from |
|---|---|---|---|
| `StatusChip` | `ui` | Garden home, plant, scan, map | `Plant.status`, `ScanStatus` |
| `ConfidenceBadge` | `ui` | Findings, measurements, soil (never the diagnosis) | `Observation.confidence_band`, `MeasurementPoint.confidence_band` |
| `StageProgress` | `ui` | Scan results | Reducer stages, `Stage` |
| `PhotoWithRegions` | `ui` | Scan results, chat attachments | `ScanImage`, `Region`, `Observation.region` |
| `LineChart` | `ui` | Plant detail | `MeasurementPoint[]` |
| `CitationList` | `ui` | Diagnosis, plan, chat | `Citation[]` |
| `WeatherStrip` | `features/gardens` (exported) | Garden home, scan results | `WeatherSummary` |
| `QuestionCard` | `features/scans` | Scan results, plan | `Question` |
| `SpeciesCard`, `SpeciesPicker` | `features/plants` (exported) | Scan results, plant detail, new plant | `SpeciesCandidate[]` with `reference_image`, `SpeciesSummary` |
| `DiagnosisCard` | `features/scans` | Scan results | `Diagnosis`, `CandidateScore`, `IndicatorEvidence` |
| `OrganismList` | `features/scans` | Scan results | `Diagnosis.organisms` with `preys_on` and `reporting_url` |
| `PlanChecklist` | `features/scans` | Scan results | `ActionPlan`, `PlanStep`, `Task` |
| `TaskRow` | `features/tasks` (exported) | Tasks, garden home | `Task` |
| `NoteList` | `features/plants` (exported) | Garden home, plant detail | `Memory[]` |
| `PendingActionCard` | `features/chat` | Chat | `PendingAction` |
| `MemoryCard` | `features/chat` | Chat | `Memory` |
| `GardenMapView` | `features/map` | Map | `GardenMap`, `Plant[]` |
| `ShotStrip` | `features/capture` | Capture | Capture model slots |

---

## 6. Wording for uncertain results

The safety rules in architecture 11 apply to how results are written, not only to what the backend sends. The diagnosis has no confidence of its own (contracts 7), so its wording comes from `status` and `low_evidence`. Bands are shown only on findings and measurements.

| Situation | Write | Never write |
|---|---|---|
| `confirmed` | "Early blight", with the matched indicators beneath | "100%", or any number for the diagnosis |
| `uncertain` | "Not sure yet: early blight or Septoria leaf spot", the top candidates, and the question | A single confident name |
| `uncertain` with `low_evidence` | "We couldn't tell what's wrong from these photos. Take a closer photo of an affected leaf, or ask your local extension office." | Any problem's name |
| `healthy` | "Looks healthy" | "Your plant is fine" when only one photo was checked |
| Provisional plan | "We're not sure yet. These steps are safe for every likely problem." | Steps presented as a cure |
| A finding | "Ladybug larva", with its band ("high") | A band on the diagnosis |
| Beneficial organism | "Ladybug larva: leave it, it eats aphids" from `preys_on`; "Beneficial: leave it" without | Any word that suggests removing it |
| Invasive organism | "This may be an invasive species. How to report" | "Report sent" (nothing is sent) |
| Chemical step | The server's safety note, as sent | Any dose, rate or mix the app adds |
| Measurements | "Spots about 2 mm across", with the band | More precision than the value carries |

---

## 7. Sources

Each link was opened on 2026-10-15. The full list is in [README 21](README.md#21-sources).

| Topic | Documentation |
|---|---|
| Routes and sheets | [Expo Router](https://docs.expo.dev/router/introduction/) · [modals and sheets](https://docs.expo.dev/router/advanced/modals/) · [typed routes](https://docs.expo.dev/router/reference/typed-routes/) |
| Sign-in screens | [Clerk custom flows: email and password](https://clerk.com/docs/guides/development/custom-flows/authentication/email-password) · [multi-factor](https://clerk.com/docs/guides/development/custom-flows/authentication/multi-factor-authentication) · [Google OAuth policies](https://developers.google.com/identity/protocols/oauth2/policies) |
| Capture | [expo-camera](https://docs.expo.dev/versions/v57.0.0/sdk/camera/) · [expo-image-picker](https://docs.expo.dev/versions/v57.0.0/sdk/imagepicker/) · [MDN file input](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/input/file) |
| Images, lists, charts, map | [expo-image](https://docs.expo.dev/versions/v57.0.0/sdk/image/) · [FlashList](https://shopify.github.io/flash-list/) · [react-native-svg](https://github.com/software-mansion/react-native-svg) · [Gesture Handler](https://docs.swmansion.com/react-native-gesture-handler/) · [Reanimated](https://docs.swmansion.com/react-native-reanimated/) |
| Chat composer | [Keyboard Controller](https://kirillzyusko.github.io/react-native-keyboard-controller/) |
| Forms | [React Hook Form](https://react-hook-form.com/) · [Zod](https://zod.dev/) |
| Accessibility | [WCAG 2.2 quick reference](https://www.w3.org/WAI/WCAG22/quickref/) |
| Home screen on phones | [Expo PWA guide](https://docs.expo.dev/guides/progressive-web-apps/) · [MDN web app manifest](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest) |
