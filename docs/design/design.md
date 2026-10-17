# PDM Design

> **Status:** Draft v0.2, 2026-10-17. v0.2: folds in the PR #43 audit: one demo story (Tomato 1 diagnosed on Oct 11 and recovering; today's live scan finds early blight on its neighbour Tomato 2), the capture tint rule, the wording sources, and 3D claims marked as not yet run. v0.1: the app's design direction: product principles from the research, what Apple owns and what we own, four directions mocked across eight screens (the current F0 baseline and three researched directions), the five borrowed ideas built into the mockups, the 3D garden, the insights, and the decisions still open.

This document sets how PDM looks, moves and speaks, so the build-week slices (F1 to F8 in the frontend roadmap on PR #42) build to one direction instead of improvising one. It sits on top of the [frontend architecture](../frontend/README.md), its [flows](../frontend/flows.md) and the [screens](../frontend/screens.md), which stay the source for what each screen contains. This document decides how it looks.

The folder holds the research behind it and the mockups:

| File | What it is |
|---|---|
| [mockups/index.html](mockups/index.html) | Clickable mockups: eight screens in four directions, light and dark, full or reduced motion, a screen compared across directions, and the demo flow played with a simulated finger. Open the file in a browser; the 3D view loads three.js from cdnjs |
| [philosophies.md](philosophies.md) | Ten design philosophies, a taste section, what Apple owns, and the full specs of the three directions (palettes checked for contrast, type, shape, motion, 3D) |
| [landscape.md](landscape.md) | Twelve plant and garden apps and three others: table stakes, the gaps PDM can own, ideas to borrow, what would look generic |
| [inspiration.md](inspiration.md) | Sixteen non-competitor apps (Flighty, Merlin, Windy, Object Capture, Crouton and others) and the patterns worth borrowing, screen by screen |
| [analytics.md](analytics.md) | Every data source we capture, the insights per plant and per garden they allow, what is buildable now, and the backend gaps |
| [garden-3d.md](garden-3d.md) | How to render the 3D garden in Expo SDK 57 (react-three-fiber on expo-gl), the on-device LiDAR mesh view, and one open expo-gl bug |

## Contents

1. [Principles](#1-principles)
2. [What Apple owns and what we own](#2-what-apple-owns-and-what-we-own)
3. [The directions](#3-the-directions)
4. [Recommendation](#4-recommendation)
5. [The demo, screen by screen](#5-the-demo-screen-by-screen)
6. [Rules every direction follows](#6-rules-every-direction-follows)
7. [The 3D garden](#7-the-3d-garden)
8. [Insights](#8-insights)
9. [What changes in the app](#9-what-changes-in-the-app)
10. [Open decisions](#10-open-decisions)

---

## 1. Principles

From the landscape review ([landscape §6](landscape.md#6-gaps-pdm-can-own)), the philosophies ([philosophies §2](philosophies.md#2-what-any-style-must-carry)) and the architecture's judging criteria ([architecture §1](../architecture/README.md#1-goals-and-judging-criteria)).

| Principle | What it means on screen |
|---|---|
| **Show the work** | The analysis is the show, not a spinner. Stages tick with what each found, the box is drawn on the user's own photo, the measured size sits beside it. No plant app does this ([landscape §6.2](landscape.md#62-showing-the-work)) |
| **Honest about uncertainty** | No number on the diagnosis. When the evidence is split, the app says "Not sure yet", shows both candidates at equal weight and asks one question that settles it ([screens §6](../frontend/screens.md#6-wording-for-uncertain-results)) |
| **Context and sources** | The forecast is tied to this plant's diagnosis, and every plan step cites its source (OMAFRA, Cornell). No competitor does either ([landscape §6.3](landscape.md#63-context-and-sources)) |
| **Leave it** | Beneficial insects get a calm card: "Ladybug larva: leave it, it eats aphids" |
| **Measure over time** | Heights, spot sizes and spot cover are drawn with their noise bands, and trends are claimed only from 3 points over 14 days ([analytics §2](analytics.md#2-insights-per-plant)) |
| **Outdoors, one hand** | High contrast in sun, 44 pt targets, the key action within the thumb's reach, and status as a word, a colour and a shape |
| **Not the category look** | No emerald-and-mint palette, no stock monstera, no mascot, no percentage, no upsell in the result ([landscape §8](landscape.md#8-what-would-make-pdm-look-generic)) |

## 2. What Apple owns and what we own

Expo renders native iOS 26 components, so the chrome is Apple's whatever direction we pick: the Liquid Glass floating tab bar from native tabs, the navigation bar and its glass buttons, sheets, menus, alerts, gestures, and SF Pro in all of them. A direction is a layer on top of Apple's design, not an alternative to it ([philosophies §6](philosophies.md#6-what-apple-owns-and-what-we-own)).

| Apple owns | We own |
|---|---|
| Tab bar, navigation bar, toolbar buttons, sheets, menus, alerts, pickers, switches | Content colour and accent: backgrounds, cards, list rows |
| SF Pro in bars, large titles on list screens, tab labels and system buttons | The display face in content heroes: plant names, the diagnosis headline, the plan title, the question |
| System transitions and gestures | Motion inside content: stage ticks, the box draw, task checks, the 3D garden |
| Lock Screen and Dynamic Island frames | What a Live Activity says and shows |

- **Tint.** The accent tints the selected tab and one prominent button per screen (Analyse on capture, Done on a sheet). Nothing tinted sits over a photo, the camera preview or the 3D view: glass that adapts to dark content would make a light-mode accent unreadable (philosophies §6, to check on the iPhone). On capture, Analyse sits on the black control area below the preview, so it uses the dark-scheme accent.
- **Android** gets Material chrome from the same components, with the same content layer.
- **The web build** has no native chrome: we draw the header and tab bar as opaque `surface` with a hairline edge. The mockups imitate glass on every target only so the directions can be compared; the real web build does not.

## 3. The directions

The mockups show four directions on the same eight screens. The baseline is what F0 built; the other three come from [philosophies §7](philosophies.md#7-three-directions-to-mock), deliberately far apart: one restrained, one expressive, one data-forward. Only one of them is green.

| | Current F0 | Herbarium | Seed Packet | Instrument |
|---|---|---|---|---|
| **In three words** | Baseline, as built | Restrained, precise, considered | Warm, crafted, generous | Exact, legible, candid |
| **Idea** | Apple defaults, leaf green on off-white | A pressed specimen sheet: paper and ink, the plant is the colour | An heirloom seed catalogue: printed colour, a soft serif | A field instrument that shows its working: black, white, colour only for status and data |
| **Accent** | `#2F6B47` green | `#2C5C3E` herbarium green, darker than every status | `#7D2B63` beet plum | Ink `#111814`, paper `#EEF2F0` in dark; no hue |
| **Display face** | System | Newsreader (OFL) | Fraunces (OFL) | Atkinson Hyperlegible Next and Mono (OFL) |
| **Region box** | White box | Paper outline, like a pencil | Plum lasso with a pulse | Reticle corners with the size in mm |
| **"Not sure yet"** | Two plates | Two equal plates, in italics | Two equal packets, in italics | A comparison table of the marks that separate the candidates |
| **Motion** | Fades | Ink: 200 to 480 ms, no overshoot | Springs with 8 to 25% overshoot, a stamp on check | Cuts and critically damped readouts |
| **3D garden** | Organic plants, species greens | Architect's model, ink outlines, orthographic | Clay diorama, warm low light, long shadows | Survey model on a grid, height labels, no shadows |
| **Known issues** | Accent and healthy are one green; watch and sick look alike to colour-blind users | Green accent is the category colour, held back by depth and restraint | Fraunces' soft axis needs our own font cut; plum near sick in dark mode | Can feel cold; depends on good photos for warmth |

All six new palettes pass 4.5:1 on every text pair, and their status colours stay apart under simulated colour blindness (smallest ΔE 11, against 4 in the current palette). The hex values are in [philosophies §7.4](philosophies.md#74-palettes) and in [mockups/directions.js](mockups/directions.js).

## 4. Recommendation

**Instrument, with Seed Packet's warmth in the words, not the colours.** The decision is the team's; this is the case for it.

- **It owns the gaps.** Showing the work and honesty are PDM's two strongest claims ([landscape §6](landscape.md#6-gaps-pdm-can-own)). Instrument's run log with stage times, the reticle with the measured size, and the comparison table for "Not sure yet" say both without a word of explanation.
- **It is the furthest from the category.** Every competitor is green or mint ([landscape §8](landscape.md#8-what-would-make-pdm-look-generic)). With no brand hue, every colour on screen means a status or a measurement, so the sick tomato is the one red thing in the 3D garden on video.
- **It works outdoors.** Black on white with Atkinson Hyperlegible is the most legible option in sun, and its numerals were designed not to be confused.
- **It is the cheapest to build well.** No illustrations to commission, no custom font cut, no shadows; the chrome is the system's untouched.
- **Its risk is coldness.** Warmth comes from the user's own photos, the 3D garden and the plain, kind wording of the plan ("This week: two small jobs"), which Seed Packet's research supplies.

Herbarium is the safe second: tasteful and quiet, but green. Seed Packet is the most memorable and the most work.

## 5. The demo, screen by screen

**The story.** Tomato 1 was diagnosed with early blight on Oct 11 and is recovering under its plan. One of that plan's steps is a weekly check of Tomato 2, 40 cm away. Today's check is the demo's live scan: it catches early blight on Tomato 2 early. The data behind every screen is the mock fixture in [analytics §6](analytics.md#seeded-data-for-mock-mode).

What each screen does in every direction. The borrowed ideas are credited to their source; the five strongest from [inspiration §13](inspiration.md#13-the-five-strongest-ideas-for-the-demo) are all in the mockups.

| Screen | What it shows | Borrowed from |
|---|---|---|
| **Garden home** | Large title, the forecast (as days, or Instrument's line of lows with the 0 °C frost threshold), the frost alert, plants by health, today's tasks (among them the weekly check of Tomato 2). The scan action is a round button beside the tab bar, so it never covers content | Camera first is table stakes ([landscape §5](landscape.md#5-table-stakes)) |
| **Capture** | Camera with a framing guide, the LiDAR distance ("32 cm · good distance"), a shot strip, and a **coverage dial** around the shutter that fills one segment per needed shot | Object Capture's coverage ring ([inspiration §7.1](inspiration.md#71-apple-object-capture)) |
| **Live analysis** | The photo with the box drawn on the spots and the measured size, a **phase line** ("Checking telltale signs · 6 s · Next: weighing the evidence"), the stage list, "Not sure yet" with both candidates, and the **question card with picture answers**: two reference crops as Yes and No | Flighty's phase line ([inspiration §3.1](inspiration.md#31-flighty)); Merlin's picture answers ([inspiration §4.1](inspiration.md#41-merlin-bird-id)) |
| **Live Activity** | The scan on the Lock Screen and in the Dynamic Island: stage, elapsed time, a nine-segment bar, what comes next. It alerts only for the question and the result | Flighty and Apple's Live Activity rules ([inspiration §3.2](inspiration.md#32-uber-and-the-live-activity-pattern)) |
| **Diagnosis and plan** | The diagnosis worded from the evidence, what matched (with "measured" on the LiDAR indicator), the risk with its driver, the beneficial larva, the plan as a cited checklist | Oura's words for every state ([landscape §7](landscape.md#7-ideas-to-borrow)); the diagnosis itself carries no confidence word (screens §6) |
| **Plan, step by step** | One step per screen in large type: when, why, how, its source, and a time phrase turned into named tasks ("Adds 5 tasks: Oct 20, 23, 26, 29, Nov 1") | Crouton's step mode ([inspiration §9.2](inspiration.md#92-crouton)) |
| **3D garden** | The garden built from the map, plants sized by measurement and coloured by health. The **date scrubber** runs from the first scan to today, Tomato 1 turns sick on Oct 11, Tomato 2 on Oct 17 after the live scan, and one step past today shows Thursday's frost with covers over the tender plants | Windy's and Moonlitt's timeline ([inspiration §5](inspiration.md#5-alerts-forecasts-and-time)); Polycam's measurement labels ([landscape §7](landscape.md#7-ideas-to-borrow)) |
| **Insights** | Tomato 1's health over time, spot cover 14% → 9% → 5% with range bars and the dates plan steps were done, height with its noise band, humid hours before the diagnosis, and Tomato 2, 40 cm away, where the same problem was also found today (worded "also found on", never "spread from") | [analytics §6](analytics.md#6-the-demos-analytics-moment) |

**The demo flow** in the mockups ("Play the flow") runs home → capture → live analysis → the answer → diagnosis and plan → a checked step → the 3D garden scrubbed into the forecast → insights. It is about 50 seconds, a quarter of the video.

## 6. Rules every direction follows

- **Status is a word, a colour and a shape:** healthy ● circle, watch ▲ triangle, sick ◆ diamond, dead □ hollow square, on chips, 3D labels and the legend ([philosophies §8](philosophies.md#8-rules-shared-by-all-three)).
- **Wording** follows [screens §6](../frontend/screens.md#6-wording-for-uncertain-results): no number on a diagnosis, and bands only on findings and measurements. Charts follow [analytics §2](analytics.md#shared-rules) and [§7](analytics.md#7-privacy-and-wording): no extrapolation, trends only from enough points, and weather shown as a pattern, never a cause.
- **Contrast** is WCAG 2.2 AA: 4.5:1 for text, 3:1 for marks and control edges. Touch targets are at least 44 pt.
- **Every motion has a still version.** Reduced motion removes slides, springs, the grow-in and the auto-orbit; the stage state and the region box read in a still frame. The mockups have a Reduced switch to check this.
- **Haptics** mark real events only: a selection tick per stage, success on a checked task, a tick per scan date on the 3D scrubber. None on the web.
- **Glass only in the chrome.** No content card is glass.

## 7. The 3D garden

- **Data:** only what the garden map stores: bed outlines, plant pins in metres, canopy radius, and each plant's measured height ([architecture §6.8](../architecture/README.md#68-garden-map-lidar)). No mesh, photo or coordinates; the sheet says so. A plant with no measurement is drawn as "not measured", never with an invented height.
- **Stack:** `@react-three/fiber` 9.8.2 on `expo-gl` ~57.0.2 with `three`, procedural geometry, no GLB files or textures, labels as React Native views projected each frame. It should run in Expo Go and on the web from one scene; neither has been run on SDK 57 yet ([garden-3d §6](garden-3d.md#6-recommendation)).
- **Risk:** expo issue #51543: on expo-gl 57.0.2 a WebGL 2 context passes `instanceof WebGLRenderingContext`, and three r163 or later refuses to render. The first task is a one-day spike on the iPhone and a mid-range Android phone with the workaround applied ([garden-3d §1](garden-3d.md#known-issues)).
- **Accessibility:** a plant list is the screen reader's view of the garden, and the 2D map stays the default and the fallback.
- **Performance:** the direction specs describe iOS. On Android and slower phones, use Lambert materials and drop shadows ([garden-3d §2](garden-3d.md#2-performance-budget)).
- **The mockup** uses three.js r128 as a browser global for convenience; the app uses the versions above.

## 8. Insights

The Insights screen in the mockups follows [analytics.md](analytics.md): Tomato 1's status strip, spot cover after treatment, height with its noise band, weather before the diagnosis, and its neighbour. The data for most of it exists; these backend changes are needed ([analytics §5](analytics.md#5-buildable-now-and-backend-work)):

- `GET /plants/{id}/measurements` taking every measured quantity, each point with `min`, `max`, `n` and `image_kind`.
- `TimelineEntry` with `low_evidence`, `primary.kind`, `primary.weather_fit`, `weather_7d` and `organisms`.
- `GET /gardens/{id}/timeline` and `GET /gardens/{id}/measurements`.
- `GET /tasks?plant_id=&plan_id=`.
- `spacing_cm` and `mature_height_cm` on the species care summary.

## 9. What changes in the app

Once a direction is chosen:

1. **Tokens.** Replace the values in `app/src/ui/tokens.ts` with the chosen palette; add the display and number font families and the motion values as tokens.
2. **Fonts.** Load the chosen families with the `expo-font` config plugin (one `fontFamily` per weight); bar titles stay system.
3. **New composites:** `StatusShape`, `PhaseLine`, `CandidateCompare` (plates or table), `PictureAnswer`, `CoverageDial`, `StepMode`, `StatusStrip`, `BandLine` and `StepLanes` (the last three from analytics §2).
4. **Libraries to add, each with its reason in its pull request** (not in [frontend §4](../frontend/README.md#4-libraries) yet): `expo-symbols`, `expo-glass-effect`, `expo-gl`, `three`, `@react-three/fiber`.
5. **Live Activity:** an ActivityKit widget extension, which needs a development build, and a decision on how it is added to the Expo project (open decision 4).
6. **The frontend roadmap** (PR #42) gains this design as the first item of F1, and F8 Polish checks every screen against this document.

## 10. Open decisions

1. **The direction.** Instrument is recommended (section 4). The team picks one, or one direction's structure with another's detail.
2. **The scan action's place.** A round button beside the native tab bar, as in the mockups, or a tab of its own. Check what `NativeTabs` offers on SDK 57 before building.
3. **Glass tint over photos.** Check on the iPhone whether a tinted control stays readable when the glass adapts to dark content (philosophies §9).
4. **Live Activity in Expo.** How the widget extension is added and whether it fits the build week; the Lock Screen design does not depend on it.
5. **The 3D spike.** Run it first; if expo-gl blocks three r163 or later, pin three below r163 (garden-3d §6).
6. **Fonts.** If Seed Packet is chosen, cut Fraunces instances with the soft axis set (philosophies §9).
