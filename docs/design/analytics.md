# PDM Analytics

> **Status:** Draft v0.1, 2026-10-17. v0.1: the in-app insights for a plant, a garden and a farm, built only from data PDM already stores, with what each needs from the backend, the demo moment and its mock data, and the privacy and wording rules.

This note designs the insights the app shows its users: charts and short statements about their plants and gardens. It is a companion to [`design.md`](design.md) (same folder), [screens](../frontend/screens.md) and the 3D garden notes in [`garden-3d.md`](garden-3d.md). Every insight here uses data the contracts already define. Where an insight needs a new endpoint or field, section 5 names it. Where it would need data PDM doesn't capture, it is in the "later" list, not in the design.

## Contents

1. [What we capture](#1-what-we-capture)
2. [Insights per plant](#2-insights-per-plant)
3. [Insights per garden](#3-insights-per-garden)
4. [For farms](#4-for-farms)
5. [Buildable now, and backend work](#5-buildable-now-and-backend-work)
6. [The demo's analytics moment](#6-the-demos-analytics-moment)
7. [Privacy and wording](#7-privacy-and-wording)
8. [Sources](#8-sources)

## 1. What we capture

Scans happen when a member takes them, so every series is **irregular**: some plants get a scan a week, others one a season. Each chart plots real dates, never evenly spaced points.

| Data | Comes from | Unit and precision | How often | Kept | Where defined |
|---|---|---|---|---|---|
| Scan: `created_at`, `created_by`, `status`, `failure.code` | Scan | Timestamp; creator's display name | When a member scans; at most 30 a day per user | Until its plant or garden is deleted. `created_by` is cleared when the user deletes their account | contracts §3; `scan` |
| Scan images: `kind`, size, `uploaded_at`, signed thumbnail | Scan | 1 `whole_plant`, up to 2 `leaf_closeup`, 1 each of the other kinds. EXIF stripped, faces blurred | Per scan | With the scan | contracts §3; `scan_image` |
| Observations: disease classifier labels, organisms (`role`, `count` one, few or many, `length_mm`), damage patterns, soil aspects | Scan (classifier, BioCLIP, VLM) | `confidence` 0–1 and a band: high 0.85, medium 0.65, low 0.4 | Per scan and run | With the scan. A rerun replaces a stage's observations; retracted ones carry `retracted_at` | contracts §7 Observation; `observation` |
| Plant height, canopy width | LiDAR, `whole_plant` shot | cm, one value per shot; noise about 2 cm or 5% | Scans from a LiDAR iPhone or iPad only | With the scan. Medium confidence or more enters the growth series | contracts §7 Measurement, Growth |
| Leaf length, leaf area | LiDAR, leaf close-ups | cm, cm²; median, min, max and `n` over the measured leaves | LiDAR close-ups with a leaf mask | With the scan | contracts §7 Measurement |
| Spot diameter, spot count, affected area | LiDAR, leaf close-ups | mm (spots under 4 px are counted, not sized; about 0.2 mm per px at 30 cm), count per leaf, % of leaf area including yellowing | As above. Not on the fallback worker, which has no masks | With the scan | contracts §7 Measurement |
| Hole diameter, hole count | LiDAR, leaf close-ups | mm, count per leaf | As above | With the scan | contracts §7 Measurement |
| Soil texture | Guided squeeze or jar test | A soil observation, `aspect: "texture"` | Any number per scan, after submit | With the scan; never replaced by a run | contracts §3 GuidedCheck; `guided_check` |
| Diagnosis: `status`, `low_evidence`, `primary` (problem, `kind`, `weather_fit`), `candidates`, `organisms` with role and `preys_on`, `risks` with level and drivers | Pipeline | No confidence of its own; risk levels low, medium or high | Per run, and a new one after a late answer or guided check | Every version is stored with `supersedes`; the API returns the current one | contracts §7 Diagnosis; `diagnosis` |
| Action plan: `version`, `reason`, `provisional`, steps (`category`, `when`, `targets`, neighbour `plant_id`), `watch_for` | Plan writer | Text; no doses by design | Per diagnosis, and on re-plans | Every version is stored; the API returns the scan's current plan | contracts §7 ActionPlan; `action_plan` |
| Tasks: `source`, `trigger` (kind, date), `due`, `repeat`, `done_at`, `dismissed_at`, `dismissed_reason`, `plant_id`, `plan_id`, `plan_step_id` | Plan, forecast alert, chat, user | `LocalDate` in the garden's time zone; timestamps | Per plan step, alert or user action | Plan and alert tasks can't be deleted, only dismissed. User and chat tasks can be deleted | contracts §7 Task, §9 Tasks; `task` |
| Weather at the scan: current temp, RH, wind, rain; past 7 days' rain, mean RH, `hours_rh_over_90`, `longest_humid_run_days`; 7-day forecast with `frost_risk`; soil moisture 0–9 and 9–27 cm, soil temp at 6 cm, ET₀ | Weather API (Open-Meteo) | °C, %, km/h, mm, hours, days, m³/m³; a cell of about 1 km | Once per scan, a snapshot | With the scan (`scan.weather`). The shared cache keeps only 2 hours | contracts §7 WeatherSummary; database §4 cleanup |
| Garden weather now | Weather API | As above | The current hour | Not kept | contracts §9 Gardens |
| Soil profile: pH, clay, sand, silt, organic carbon, nitrogen, CEC, bulk density, by depth to 200 cm | Soil API (SoilGrids) | g/kg, %, cmol/kg, kg/dm³ | Once per garden | On the garden, until it moves | contracts §10 SoilProfile |
| Plant: species and its status, `status` (healthy, watch, sick, dead), `created_at` | User and pipeline | Current value only | Changes after each diagnosis or edit | Only the latest status: no history | contracts §3 Plant; `plant` |
| Garden map: bed polygons with `area_m2`, pins with `x_m`, `y_m`, `canopy_radius_m`, `bed_id`; `north_deg` | LiDAR mapping | Metres on a local ground plane; no coordinates | When an editor maps | **Only the current version**; older ones are deleted | contracts §9 Garden map; `garden_map` |
| Neighbours, crowding | Map plus species care | Neighbours within 1.5 m; crowded below 0.75 × `spacing_cm` | Computed when read | Not stored | contracts §9 Garden map |
| Species care: `spacing_cm`, `mature_height_cm` | Knowledge base | cm | Seeded | Reference data | contracts §10 |
| Memories: `kind`, `topic` (watering, feeding, treatment, pruning, harvest…), `text`, `shared`, `created_at` | User notes, chat, pipeline | Free text, dated when written | Irregular | Until deleted. Private chat memories are seen only by their author | contracts §8 Memory; `memory` |
| Scan feedback: `verdict`, `corrected_label` | Reviewer or above | Text | Rare | Deleted with its author's account. No read endpoint | contracts §3; `scan_feedback` |
| Farm records: text chunk, author's display name, time, link, title (`#field-reports`, `Spray log 2026`) | Connector (Slack, Drive) | Free text, up to 2,000 characters | Synced on a schedule | 180 days; deleted on disconnect | connectors §4 |

**Not kept, so never charted:** plant status history (only the current value), map history, positions of a plant before the latest map, daily weather between scans, who completed a task (`done_at` has no user), scan events beyond replay, and harvest amounts.

## 2. Insights per plant

### Shared rules

These apply to every insight in sections 2 to 4.

- **A trend needs 3 points spanning at least 14 days.** With fewer, show the points and no trend line or trend words. 14 days matches the `growth_stalled` rule (contracts §7, growth).
- **Only medium confidence or more** enters a series, as the growth series already does. Low measurements show on their scan, not on charts.
- **Show the noise.** Height and canopy width get a band of ±2 cm or ±5%, whichever is larger: the noise the `growth_stalled` rule uses. Leaf quantities get their own `min`–`max` over the measured leaves.
- **Say "lower" or "higher" only when the bands don't overlap.** Otherwise say "about the same".
- **No extrapolation.** Lines join measured points and stop at the last one.
- **A diagnosis has no number.** Health is drawn as status colour, never as a score or a percentage (screens §6).
- **Stale data says so.** A plant whose last diagnosis is older than 14 days counts as "not checked recently", not as its last status.
- **Draw with `react-native-svg`** in the existing `LineChart` composite and a few new ones (`StatusStrip`, `BandLine`, `StepLanes`). The frontend uses no chart library (frontend §4).

### P1. Health timeline

- **Answers:** "Has this plant been getting better or worse?"
- **Data:** `GET /plants/{id}/timeline`: each scan's `diagnosis_status`, `primary`, `failure_code`, thumbnail.
- **Computation:** one cell per scan, coloured by status: healthy green, uncertain amber, confirmed with the problem's name, failed grey. An `uncertain` result with `low_evidence` is grey "Not sure", not amber: nothing pointed to a problem (needs `low_evidence` on `TimelineEntry`, section 5).
- **Chart:** a horizontal status strip above the existing timeline list, cells placed on a date axis. Tapping a cell opens the scan.
- **Empty and low data:** no scans: "Scan this plant to start its history." One scan: the cell and "One check so far".
- **Uncertainty rule:** gaps of more than 14 days between scans are drawn as gaps, never filled with the earlier colour.

### P2. Growth curve

- **Answers:** "Is it growing, and how fast?"
- **Data:** `GET /plants/{id}/measurements?quantity=plant_height_cm` and `canopy_width_cm`; `mature_height_cm` of the species.
- **Computation:** points at their scan dates. With 3 points over 14 days or more, the growth rate is the change between the first and last point divided by the weeks between them, in cm per week, rounded to whole centimetres. A dashed line marks `mature_height_cm` when the species has one. When the backend adds the `growth_stalled` risk, the chart says "No growth measured in 14 days" with the risk's driver.
- **Chart:** line with a noise band, height and canopy width as two small multiples sharing the date axis.
- **Empty and low data:** no points: "Growth is measured from whole-plant photos on an iPhone or iPad with LiDAR" (shown on every device, screens §3.5). One or two points: the points and their values, no rate.
- **Uncertainty rule:** state a rate only when the change is larger than the noise band at both ends. Otherwise "No clear change yet".
- **Leaf size** (`leaf_length_cm`, `leaf_area_cm2`) is not a growth measure: the close-ups show different leaves each time. It appears only on the scan, with its range, until a later design shows it is useful.

### P3. Spots and affected area after treatment

- **Answers:** "Since I treated it, are the spots getting smaller or fewer?"
- **Data:** each scan's `affected_area_pct`, `lesion_count` and `lesion_diameter_mm` measurements (for pests, `hole_count` and `hole_diameter_mm`), with `min`, `max` and `n`; the plan's done tasks as markers.
- **Computation:** one point per scan and quantity, chosen as the growth series chooses: the highest-confidence current measurement of medium or more. Done plan-step tasks are ticks on the date axis.
- **Chart:** line with `min`–`max` bands, affected area first; spot count and spot size as small multiples beneath. Step ticks labelled by their task title.
- **Empty and low data:** no close-up measurements: "Take a leaf close-up on a LiDAR device to measure spots." One point: "Measured once: spots cover about 14% of the measured leaves."
- **Uncertainty rule:** the caption names the leaves: "on the leaves in the close-ups, which may differ between scans". Say "lower" only with 3 points and when the last `max` is below the first `min`.

### P4. What was done, and what happened next

- **Answers:** "Did we follow the plan, and what did the next check show?"
- **Data:** tasks with this `plant_id` and their `plan_id`, `plan_step_id`, `due`, `done_at`, `dismissed_at`; the next scans on the timeline.
- **Computation:** one lane per plan step, with dots at `done_at`; a repeating step shows each occurrence. Under the lanes, the next scan after each done step, within 21 days, with its status and any P3 change. Tasks dismissed as `plan_superseded` are hidden: the system dismissed them, not a person.
- **Chart:** step lanes (a small Gantt) over the health strip of P1.
- **Empty and low data:** no plan: hidden. Steps but no later scan: "Scan again to see how it's doing", with a **Scan** button.
- **Uncertainty rule:** write "After the steps were done, the next check found…", never "the steps worked" (section 7).

### P5. Recurring problems

- **Answers:** "Has this happened before?"
- **Data:** the timeline's `primary.problem_id` and `diagnosis_status`.
- **Computation:** an episode is a run of `confirmed` scans with the same problem. A recurrence is a new episode after a `healthy` scan or a gap of 21 days or more. Count per calendar year in the garden's time zone.
- **Chart:** none. One line on the plant page: "Early blight: second time this year (July, October)", linking both scans.
- **Empty and low data:** hidden until a problem has two episodes.
- **Uncertainty rule:** only `confirmed` diagnoses count. A diagnosis a reviewer marked wrong is left out once that feedback is readable (section 5).

### P6. Before and after

- **Answers:** "What did it look like then?"
- **Data:** the timeline's `whole_plant` thumbnails and `measurements`.
- **Computation:** the user picks two scans, defaulting to the first and the latest.
- **Chart:** two photos side by side, each with its date, status and height when measured.
- **Empty and low data:** fewer than two scans with a whole-plant photo: hidden.
- **Uncertainty rule:** the photos are not to scale. Heights come only from measurements, never from comparing pixels.

## 3. Insights per garden

### G1. Garden over time (the 3D scrubber)

- **Answers:** "How has the whole garden changed?"
- **Data:** the current map's beds and pins; each plant's timeline and growth series; `Plant.created_at`.
- **Computation:** steps of one week in the garden's time zone, from the first scan to today, snapping to scan dates. At each step a plant's colour is its status as of that date (P1 rules, stale after 14 days), and its size comes from its growth series: linear between measured points, held at the nearest point outside them. Canopy width ÷ 2 gives the radius; without a measured width, the map's `canopy_radius_m`. A plant created after the step's date is hidden.
- **Chart:** the designers' 3D garden ([garden-3d.md](garden-3d.md), [mockup](mockups/garden3d.js)) with a date slider, replacing the mockup's fixed growth fractions with these values. The 2D map (screens §3.8) uses the same colours.
- **Empty and low data:** no map: the plants as a grid of status tiles with the same slider. A plant with no measurements keeps a fixed size and the label "Not measured".
- **Uncertainty rule:** a plant drawn from a point more than 7 days from the step's date is drawn faded. The caption says "Positions are from today's map", because older maps aren't kept.

### G2. Health mix over time

- **Answers:** "Is the garden as a whole doing better than last month?"
- **Data:** every plant's timeline.
- **Computation:** for each week, each plant's status as of the week's end: healthy, watch (uncertain), sick (confirmed), or not checked recently.
- **Chart:** stacked columns per week, as **counts** of plants. Percentages only from 10 plants up.
- **Empty and low data:** fewer than 2 weeks with scans: this week's counts as text: "4 healthy, 1 to watch, 1 sick".
- **Uncertainty rule:** "not checked recently" is its own segment, never folded into healthy. No garden health score.

### G3. Problems by bed and between neighbours

- **Answers:** "Is one bed worse? Did a problem show up on plants next to each other?"
- **Data:** the map's beds (`bed_id` on pins) and neighbours (1.5 m); timelines; the problem's `kind`; crowding from `spacing_cm`.
- **Computation:** per bed, its plants' P1 strips. For each spreading problem (`fungal`, `bacterial`, `viral`, `pest`), neighbour pairs with the same confirmed problem, ordered by first confirmed date. Crowded pins (nearest pin under 0.75 × `spacing_cm`) are listed with both distances.
- **Chart:** small multiples, one row of status strips per bed. On the map, a thin line joins neighbours with the same problem, labelled "Early blight on both · 40 cm apart". Crowded pins get a ring.
- **Empty and low data:** no map: hidden, with the map's empty state. Plants without a pin are listed under "Not on the map".
- **Uncertainty rule:** write "also found on Tomato 2, 8 days later", never "spread from Tomato 1". Positions are today's.

### G4. Weather before problems

- **Answers:** "Do problems show up after humid or wet spells here?"
- **Data:** each scan's weather snapshot (`past_7_days.hours_rh_over_90`, `longest_humid_run_days`, `precip_mm`) and diagnosis, including the primary candidate's `weather_fit` and the risk drivers.
- **Computation:** for each confirmed fungal or bacterial diagnosis, its humid hours and longest humid run in the week before. For comparison, the same values on the garden's healthy scans.
- **Chart:** a dot strip: one row "Fungal or bacterial found", one row "Healthy", each dot a scan placed by humid hours in the week before. Tapping a dot opens the scan.
- **Empty and low data:** fewer than 3 such diagnoses: no chart, only the fact on each diagnosis: "In the week before: 3 humid days in a row, 46 hours over 90% humidity."
- **Uncertainty rule:** no correlation figure and no "humidity causes" sentence. The one causal statement allowed is the knowledge base's, with its citation: "Wet leaves favour early blight" when `weather_fit` is `favourable`. The weather is per scan only, since daily weather between scans isn't kept.

### G5. Task completion

- **Answers:** "Are we keeping up?"
- **Data:** `GET /tasks?garden_id=&state=open|done|dismissed`.
- **Computation:** per week, tasks due that week: done on time (`done_at` on or before `due`, in the garden's time zone), done late, dismissed by a member, still open. `plan_superseded` dismissals are left out. Split by `source`.
- **Chart:** stacked columns per week; a line of text for the current week: "8 of 11 plan tasks done".
- **Empty and low data:** no tasks due yet: hidden.
- **Uncertainty rule:** deleted user and chat tasks can't be counted, so the caption says "plan and alert tasks" when it means only those. No per-member breakdown (section 7).

### G6. Beneficial insects seen

- **Answers:** "Which helpers live in my garden?"
- **Data:** each diagnosis's `organisms` with `role: "beneficial"` and `preys_on`.
- **Computation:** sightings, one per scan and organism, with medium confidence or more.
- **Chart:** under 5 sightings, a list: "Ladybug larva · Pepper · Oct 4 · eats aphids". From 5, a dot timeline per organism.
- **Empty and low data:** "No beneficial insects seen in scans yet."
- **Uncertainty rule:** the VLM reports one, few or many, so the app counts sightings, never insects. A pest and a later beneficial on the same plant are shown in date order without linking them.

### G7. Frost and weather alerts acted on

- **Answers:** "Did we cover the plants when frost was forecast?"
- **Data:** `GET /tasks?garden_id=&source=forecast_alert`, with `trigger.kind`, `trigger.date`, `done_at`, `dismissed_at`.
- **Computation:** per alert: done before or on the trigger date, done after, dismissed, or open after its date.
- **Chart:** a list grouped by trigger, newest first: "Frost, Oct 16: done Oct 15, evening".
- **Empty and low data:** "No alerts yet this season."
- **Uncertainty rule:** alert tasks belong to the garden, not a plant, so no outcome is linked to an alert. Scans in the next 7 days that carry a `frost_damage` risk are listed beneath it as "Checks after the frost".

### G8. Check-up coverage

- **Answers:** "Which plants haven't been checked lately?"
- **Data:** `Plant.latest_scan.created_at` from `GET /plants?garden_id=`.
- **Computation:** plants whose latest scan is older than 14 days, oldest first.
- **Chart:** a list on garden home, each with **Scan**.
- **Empty and low data:** all checked: hidden.
- **Uncertainty rule:** none needed. It only states dates.

## 4. For farms

Farm records are free text, so they add **context**, not numbers.

- **Crew reports on the timeline.** A bed's or plant's strip (P1, G3) shows a marker on days when `#field-reports` messages mention its name. Tapping it lists the messages with author, time and a link to Slack, as the chat cites them (connectors §2).
- **Spray log beside the plan lanes.** P4's lanes show spray-log entries that name the bed as a separate lane, "From the spray log", with product and date. **Never the rate**: the dose check applies (connectors §7).
- **Weekly count per bed:** "3 crew reports mention bed 3 this week". It counts records, not authors.
- **Limit:** structured fields (product, bed, date) would need extraction from the text, which is new capture. It is in the later list.

## 5. Buildable now, and backend work

"Now" means the app can compute the insight from existing endpoints, client-side. For a garden this costs one timeline call and two measurement calls per plant: about 20 calls for 6 plants, fine for the demo, too many for a farm.

| Insight | Status | What it needs |
|---|---|---|
| P1 Health timeline | Now | Better with `low_evidence` on `TimelineEntry` |
| P2 Growth curve | Now, without the mature-height line | `SpeciesSummary.care: { spacing_cm?, mature_height_cm? }`, from the seed's `care` |
| P3 Spots and affected area | Backend | `GET /plants/{id}/measurements?quantity=` accepts every `MeasureQuantity`, and `MeasurementPoint` gains `min`, `max`, `n` and `image_kind`. The data is already in `observation` (`quantity`, `measured_value`, `taken_at`, `plant`) |
| P4 Done and next | Now, filtering tasks on the client | `GET /tasks?plant_id=` (and `plan_id=`) so a plant's tasks are one call |
| P5 Recurring problems | Now | — |
| P6 Before and after | Now | — |
| G1 3D scrubber | Now for a small garden | `GET /gardens/{id}/timeline?from=&to=`: `TimelineEntry` with `plant_id`, across the garden, oldest first; and `GET /gardens/{id}/measurements?quantity=&from=&to=` with `plant_id` |
| G2 Health mix | Now for a small garden | The garden timeline above |
| G3 Bed and neighbour view | Now, without crowding | Crowding needs `SpeciesSummary.care.spacing_cm` |
| G4 Weather before problems | Backend | `TimelineEntry.weather_7d: { hours_rh_over_90, longest_humid_run_days, precip_mm }` from `scan.weather`, and `primary.kind` and `primary.weather_fit`. Without them, one `GET /scans/{id}` per scan |
| G5 Task completion | Now | — |
| G6 Beneficial insects | Backend | `TimelineEntry.organisms: { name, role }[]` (medium confidence or more). Without it, one `GET /scans/{id}` per scan |
| G7 Alerts acted on | Now | — |
| G8 Check-up coverage | Now | — |
| Farm markers and lanes | Backend, on top of the proposed connectors | `GET /gardens/{id}/farm-records?q=&from=&to=`, owner-chosen sources only, returning title, author, time, link and text |
| P5 with feedback | Backend | A read of scan feedback, at least `verdict` on `TimelineEntry` for reviewers and above |

Each new field is a contract change: it updates the `domain` type, its exact-JSON test, `contracts.md` and `openapi.json` in one pull request (AGENTS.md, sources of truth).

### Later: needs new capture

- **Daily weather history** per garden, stored by the scheduler, for a continuous humidity and rain line in G4.
- **Map versions kept,** so G1 can move pins and canopy radii over time.
- **Plant status changes with dates,** for "dead" and manual status edits on the timeline.
- **Structured spray-log and crew-report fields** (product, bed, date) extracted from farm records.
- **Harvest amounts** in kg or count, if users ask for yield charts; today a harvest is only a memory's text.
- **Regional outbreaks** (architecture §8.3 stretch), with its own privacy design.

Not proposed: recording who completed a task. It would enable per-member comparisons, which section 7 rules out.

## 6. The demo's analytics moment

Two insights, about 20 seconds together:

1. **G1, the garden over time.** Drag the slider from Sep 20 to Oct 17. The plants grow by their measured heights. Tomato 1 turns red on Oct 11. The caption reads "Early blight · in the week before: 3 humid days in a row". A line to Tomato 2 reads "40 cm away · checked Oct 13: looks healthy".
2. **P3 with P4, the recovery.** Tap Tomato 1. The affected-area chart drops 14% → 9% → 5%, with bands, under the step lanes: leaves removed Oct 11, mulched Oct 12, checks Oct 14 and Oct 17. The caption: "After the infected leaves were removed, spots covered less of the measured leaves at each check."

### Seeded data for mock mode

The mock fixture store (frontend §15) needs these, using the mockup's garden. Dates are relative to the device's date (today − 27 days and so on), so the demo always looks current.

| Fixture | Values |
|---|---|
| Garden and map | "Backyard", 2 beds (east, south) and a pot; 6 pinned plants as in the mockup: Tomato 1, Tomato 2, Pepper, Basil, Lettuce, Kale |
| Scans | One per plant on Sep 20, Sep 27, Oct 4, Oct 11 and Oct 17, plus Tomato 1 on Oct 14. All LiDAR |
| Heights and widths | Medium or high confidence. Tomato 1: 32, 41, 50, 58, 62 cm tall; 48 cm wide at the end. Others scaled from the mockup's `grow` arrays |
| Tomato 1 diagnoses | Healthy Sep 20 to Oct 4; Oct 11 confirmed early blight, `weather_fit: favourable`, risk `fungal_spread` medium with the driver "3 humid days in a row" |
| Weather snapshots | Oct 11: `hours_rh_over_90` 46, `longest_humid_run_days` 3. Earlier scans: 5 to 12 hours, 0 days |
| Tomato 1 affected area | Oct 11: 14% (9–21, n 3); Oct 14: 9% (6–12, n 3); Oct 17: 5% (3–7, n 3). Spot counts fall the same way |
| Plan and tasks | Plan v1 on Oct 11: remove leaves (done Oct 11), mulch by Oct 13 (done Oct 12), check every 3 days (done Oct 14 and Oct 17), a monitoring step on Tomato 2 (done Oct 13) |
| Tomato 2 | Healthy at every scan, including an extra one on Oct 13 |
| Pepper | Aphids confirmed Sep 27; ladybug larva (beneficial, eats aphids) Oct 4; healthy Oct 11; uncertain Oct 17 ("yellow lower leaves") |
| Alert | Frost on Oct 16: a `forecast_alert` task done Oct 15 |

The mockup's `status` and `note` fields map to these fixtures; its `grow` fractions are replaced by the series.

## 7. Privacy and wording

**Location and weather**
- No chart shows coordinates or a map tile. The garden's `location_label` is the only place name.
- Weather is the garden's cell, at the garden's precision. No comparisons with other gardens or "gardens near you".

**Members in shared gardens**
- Every member sees the same insights, viewers included. No insight ranks or compares members, and none counts who scanned or completed what.
- A scan's creator appears only on that scan, as today. A deleted account's scans stay without a name.
- Private chat memories never appear. Only shared memories and garden notes can annotate a timeline.
- Crew members in farm records aren't PDM users: counts are per bed, and names appear only on the record itself.

**Deletion**
- A deleted plant disappears from every past chart and garden count, because its scans go with it. The garden charts say nothing about it.

**No causal claims from correlation**
- Use "after", "before", "at the same time", "also found on". Never "because", "caused", "worked", "cured" or "spread from".
- The only cause stated is a cited knowledge-base statement, such as a disease's weather fit.
- No forecasts from the charts: no projected heights and no "will recover".

**Units and numbers**
- Metric only. Height and canopy width in whole cm; leaf length to 0.5 cm; leaf area in whole cm²; spot and hole sizes to 0.5 mm; affected area in whole %; counts as integers; temperatures in whole °C; rain in mm; soil moisture as % by volume (m³/m³ × 100).
- "About" before every measured value, with its band, as screens §6 asks: "Spots about 2 mm across (high)".
- Dates in the garden's time zone, short form ("Oct 11").
- Diagnoses carry no number and no band (screens §6). No health score for a plant or a garden.

## 8. Sources

- [Contracts §3, scan REST API](../architecture/contracts.md#3-scan-rest-api): scan lifecycle, `Plant`, `ScanImage`, `GuidedCheck`, feedback
- [Contracts §4, scan events](../architecture/contracts.md#4-scan-events-sse): event catalogue
- [Contracts §7, pipeline records](../architecture/contracts.md#7-pipeline-records): [Measurement](../architecture/contracts.md#measurement), [WeatherSummary](../architecture/contracts.md#weathersummary), [Diagnosis](../architecture/contracts.md#diagnosis), [ActionPlan](../architecture/contracts.md#actionplan), [Task](../architecture/contracts.md#task)
- [Contracts §8, memory](../architecture/contracts.md#memory)
- [Contracts §9, gardens, plants, tasks and account](../architecture/contracts.md#9-gardens-plants-tasks-and-account): timeline, growth series, [garden map](../architecture/contracts.md#garden-map), [deletion cascades](../architecture/contracts.md#deletion-cascades)
- [Contracts §10, knowledge seed format](../architecture/contracts.md#10-knowledge-seed-format): species care, `SoilProfile`
- [`schema.surql`](../../backend/migrations/schema.surql): `scan`, `observation`, `diagnosis`, `action_plan`, `task`, `garden_map`, `memory`, `scan_feedback`, `weather_cache`
- [Database §4, cleanup](../database/README.md#cleanup-no-ttl): weather cache kept 2 hours
- [Architecture §6.7, depth and measurement](../architecture/README.md#67-depth-and-measurement-lidar), [§6.8, garden map](../architecture/README.md#68-garden-map-lidar), [§8.3, forecast alerts](../architecture/README.md#83-forecast-alerts), [§11, security, privacy and safety](../architecture/README.md#11-security-privacy-and-safety)
- [Screens §3.5, plant detail and timeline](../frontend/screens.md#35-plant-detail-and-timeline), [§3.7, tasks](../frontend/screens.md#37-tasks-and-alert-inbox), [§3.8, garden map](../frontend/screens.md#38-garden-map), [§6, wording](../frontend/screens.md#6-wording-for-uncertain-results)
- [Frontend §4, libraries](../frontend/README.md#4-libraries), [§15, mock mode](../frontend/README.md#15-mock-mode)
- [Connectors §4, data](../connectors/README.md#4-data), [§8, privacy](../connectors/README.md#8-privacy-and-security), [§9.1, the farm fixture](../connectors/README.md#91-the-farm-fixture)
- [3D garden rendering notes](garden-3d.md) and the [mockup](mockups/garden3d.js)
