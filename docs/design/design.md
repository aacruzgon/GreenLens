# PDM Design

> **Status:** Draft v0.5, 2026-10-18. v0.5: Conservatory's camera screens keep the full-screen photo and the 3D garden runs full screen, with neutral controls (white shutters, the chosen tab and mode in the text colour) on them and on home; and the 3D garden moves to RealityKit on Metal in the LiDAR module, since the app is iPhone only, with react-three-fiber on expo-gl as the fallback (section 7, open decision 5, garden-3d v0.3). v0.4: adds a fifth direction, Conservatory, in F0's palette, after a premium furniture concept, the Camera app's visual intelligence mode and Apple Music's artist profiles: photos that dissolve into the ground, a capture with a camera mode bar of shots, a live analysis that drops its cards over the photo just taken, a diagnosis tinted from its photo with the name centred over it, a plan mini player, and a turntable 3D garden in the camera's language, with Health, Growth and Frost modes. v0.3: adds the chat: answers with interactive blocks after CopilotKit's Open Intelligent UI, built from the agent's tool results rather than generated code, and the contract change it needs. v0.2: folds in the PR #43 audit: one demo story (Tomato 1 diagnosed on Oct 11 and recovering; today's live scan finds early blight on its neighbour Tomato 2), the capture tint rule, the wording sources, and 3D claims marked as not yet run. v0.1: the app's design direction: product principles from the research, what Apple owns and what we own, four directions mocked across eight screens (the current F0 baseline and three researched directions), the five borrowed ideas built into the mockups, the 3D garden, the insights, and the decisions still open.

This document sets how PDM looks, moves and speaks, so the build-week slices (F1 to F8 in the frontend roadmap on PR #42) build to one direction instead of improvising one. It sits on top of the [frontend architecture](../frontend/README.md), its [flows](../frontend/flows.md) and the [screens](../frontend/screens.md), which stay the source for what each screen contains. This document decides how it looks.

The folder holds the research behind it and the mockups:

| File | What it is |
|---|---|
| [mockups/index.html](mockups/index.html) | Clickable mockups: nine screens in five directions, light and dark, full or reduced motion, a screen compared across directions, and the demo flow played with a simulated finger. Open the file in a browser; the 3D view loads three.js from cdnjs |
| [philosophies.md](philosophies.md) | Ten design philosophies, a taste section, what Apple owns, and the full specs of the three directions (palettes checked for contrast, type, shape, motion, 3D) |
| [landscape.md](landscape.md) | Twelve plant and garden apps and three others: table stakes, the gaps PDM can own, ideas to borrow, what would look generic |
| [inspiration.md](inspiration.md) | Sixteen non-competitor apps (Flighty, Merlin, Windy, Object Capture, Crouton and others) and the patterns worth borrowing, screen by screen |
| [analytics.md](analytics.md) | Every data source we capture, the insights per plant and per garden they allow, what is buildable now, and the backend gaps |
| [garden-3d.md](garden-3d.md) | How to render the 3D garden on the iPhone (RealityKit on Metal, in the LiDAR module), the fallback (react-three-fiber on expo-gl, with one open bug), and the on-device LiDAR mesh view |

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

The mockups show five directions on the same nine screens. The baseline is what F0 built; three come from [philosophies §7](philosophies.md#7-three-directions-to-mock), deliberately far apart: one restrained, one expressive, one data-forward. The fifth, Conservatory, was added after the team liked the premium look of a furniture shopping concept, the Camera app's visual intelligence mode and Apple Music's artist profiles ([section 3.1](#31-conservatory)); it keeps F0's palette. Two of them are green.

| | Current F0 | Herbarium | Seed Packet | Instrument | Conservatory |
|---|---|---|---|---|---|
| **In three words** | Baseline, as built | Restrained, precise, considered | Warm, crafted, generous | Exact, legible, candid | Soft, sculptural, quietly premium |
| **Idea** | Apple defaults, leaf green on off-white | A pressed specimen sheet: paper and ink, the plant is the colour | An heirloom seed catalogue: printed colour, a soft serif | A field instrument that shows its working: black, white, colour only for status and data | A glasshouse showroom: each plant presented like a piece of design furniture, in F0's colours |
| **Accent** | `#2F6B47` green | `#2C5C3E` herbarium green, darker than every status | `#7D2B63` beet plum | Ink `#111814`, paper `#EEF2F0` in dark; no hue | F0's `#2F6B47` green, unchanged, plus a pale sage wash `#E3EBDC` behind heroes |
| **Display face** | System | Newsreader (OFL) | Fraunces (OFL) | Atkinson Hyperlegible Next and Mono (OFL) | Urbanist (OFL), large and tight |
| **Region box** | White box | Paper outline, like a pencil | Plum lasso with a pulse | Reticle corners with the size in mm | A soft oval with a white hotspot dot, the size in its label |
| **"Not sure yet"** | Two plates | Two equal plates, in italics | Two equal packets, in italics | A comparison table of the marks that separate the candidates | Two plates |
| **Motion** | Fades | Ink: 200 to 480 ms, no overshoot | Springs with 8 to 25% overshoot, a stamp on check | Cuts and critically damped readouts | Slow, critically damped springs on the iOS sheet curve; the hotspot breathes |
| **3D garden** | Organic plants, species greens | Architect's model, ink outlines, orthographic | Clay diorama, warm low light, long shadows | Survey model on a grid, height labels, no shadows | A showroom plinth: plaster beds, terracotta pots, studio light, long lens, slow turntable |
| **Known issues** | Accent and healthy are one green; watch and sick look alike to colour-blind users | Green accent is the category colour, held back by depth and restraint | Fraunces' soft axis needs our own font cut; plum near sick in dark mode | Can feel cold; depends on good photos for warmth | F0's two colour issues; leans on the top photo, and a user's photo of a sick leaf is rarely a studio shot |

All six new palettes pass 4.5:1 on every text pair, and their status colours stay apart under simulated colour blindness (smallest ΔE 11, against 4 in the current palette). Conservatory reuses F0's palette, so it keeps F0's results, good and bad. The hex values are in [philosophies §7.4](philosophies.md#74-palettes), section 3.1 and [mockups/directions.js](mockups/directions.js).

### 3.1 Conservatory

**Source.** [Premium Furniture E-commerce App Concept](https://dribbble.com/shots/27161604-Premium-Furniture-E-commerce-App-Concept) on Dribbble: three screens (a welcome over a lifestyle photo, a home with collections, a product page for a velvet sofa). It sells sofas, not diagnoses, so the direction takes its surface and leaves its shopping habits.

**What it takes.**

| In the concept | In PDM |
|---|---|
| One saturated object on a calm ground; the interface stays quiet | F0's own palette, unchanged: leaf green on warm off-white `#F6F7F2`. The interface uses the green only for the one action per screen, so the plant photo and the status marks carry the colour |
| The welcome photo dissolves into the ground instead of sitting in a frame | **Melt:** the garden home's photo fades into a clay wash, and every screen starts with that wash at the top. Plant cards fade the plant into the card, with the name in ink below |
| Huge, tight geometric display type ("Live in Comfort") | Urbanist 600 at −0.03 em: the garden name at 54 pt, the diagnosis at 40 pt. Body, numbers and all chrome stay SF Pro |
| A white hotspot dot on the sofa marks a detail | **Hotspot:** the lesion is a soft oval inside the detector's box with a white dot at its centre and a label below ("Early blight · 8–12 mm"). The dot settles in 520 ms and then breathes |
| The angle thumbnails under the product | The three photos of the scan as rounded thumbnails, the one being read ringed in the text colour |
| "Add to cart": a black pill with the icon in a white disc | **Pill:** one green pill per screen ("See the diagnosis and plan", "Done"); with an icon, the icon sits in a white disc at the left end. Secondary buttons are pale surface pills, like the concept's selected size chip |
| The Camera app's white shutter and plain mode bar | **Controls are neutral, content is green.** The shutters are white everywhere: capture, the 3D garden and home's scan button. The chosen tab, mode and view are in the text colour on a solid or glass pill, never green, so the camera, the garden and home read as one tool |
| Large continuous radii, white cards with a long soft shadow | Cards and photos at 28 pt, a two-layer warm shadow in light mode, a border in dark |

**What it leaves.**

- **Frosted glass on content.** The concept's "Get Started" pill and card captions are glass over photos. Our rule is glass in the chrome only ([philosophies §5](philosophies.md#5-taste), tell 3, and §6), so the round buttons over the photo are the system's own bar buttons and nothing in the content is frosted.
- **Shopping devices.** Prices, discount carousels, "Top Trends" and ratings have no counterpart. The colour swatch rail has none either: a plant's colour is evidence, not an option.
- **Confidence styling.** The concept's large price sits where our diagnosis name sits; the diagnosis gets the size, never a number (section 6).

**Capture and the live analysis, after the Camera app.** Both follow the Camera app's visual intelligence mode (from screenshots the team shared): the camera fills the screen, the answer drops down from the top as a card with a grabber, a chip names what it sees on the subject, and a mode bar switches PHOTO, SIRI and PORTRAIT.

| In the Camera app | In PDM |
|---|---|
| The mode bar: PHOTO · SIRI · PORTRAIT in a glass pill, the chosen one centred | **Modes:** the capture's shots become the mode bar: WHOLE PLANT · CLOSE-UP · UNDERSIDE · SOIL · FRUIT, the `ImageKind`s taken as photos (the squeeze and jar tests stay in guided mode). A dot marks a stored shot, and the dial around the shutter still counts the needed ones. It replaces the row of shot chips in [screens §3.2](../frontend/screens.md#32-capture) |
| The answer as a card dropping from the top | On capture, the chosen shot's instruction ("Fill the frame with one affected leaf, about 30 cm away"). On the live analysis, the running stage with its time and the next one; tapping the card opens the full run log under it |
| "Maybe: Philodendron", a chip on the subject | Findings as chips on the photo: the lesion's dot with "Leaf spots · 8–12 mm", and "Tomato · Change" for the species check. **Never a guess at the diagnosis:** no "Maybe: early blight" chip. The name waits for the result, as the wording rules require (section 6) |
| The shutter glows while it looks | The shutter becomes a glowing ring while the analysis runs, and gives way to "See the diagnosis and plan" when the plan is ready |
| | "Not sure yet" drops down under the stage card. The one question sits low and compact, with small picture answers, so the spots it asks about stay in view between the cards |

The live analysis shows the photo just taken, held still, not a live camera feed: the analysis runs on the uploaded photos on the rig, and the screen doesn't pretend otherwise. The three photos sit in a strip at the bottom. Both screens are always dark, like the camera. The cards and the mode bar are ours, drawn over `LidarCameraView` or `expo-camera`; iOS has no API for the Camera app's mode picker. The photo fills the screen, so the controls on it are neutral: the shutter, the glowing ring and Analyse are white, and the mode bar and the photo strip are solid dark pills with the chosen mode in white. Nothing tinted sits on the photo (section 2); the accent stays on the solid cards. The cards and chips carry content, so they are solid dark, not frosted like the Camera app's card: no content card is glass (section 6).

**The 3D garden, in the same language.** The garden takes the camera's pieces so the three screens feel like one tool:

- **A mode bar:** HEALTH · GROWTH · FROST. Health colours each plant's ring by status. Growth adds each plant's measured height to its label ("Kale · 55 cm"). Frost jumps to Thursday's forecast and puts frost covers on the four tender plants.
- **A card from the top** says what the mode shows ("Frost on Thursday night. Low of −1 °C. Cover the 4 tender plants by Thursday evening") and carries the status legend, so every label's shape has its word on screen.
- **The plant labels are chips** on the scene, the selected one outlined: nothing tinted sits on the 3D view (section 2).
- **The selected plant's card rests as a pill** ("Tomato 1 · ◆ Sick") above the shutter row, so the garden keeps the room. A tap grows it into the full card, like the question on the live analysis: height, width, note, the distance to the nearest plant on the map, and the timeline from Sep 20 to Thursday's forecast. A tap on its header or grabber shrinks it back.
- **A shutter row:** Insights, **Scan** and Ask, all for the selected plant. The shutter scans it, which is the most common next step from the garden.
- **Full screen, like a viewfinder.** The scene runs edge to edge behind the card and the controls, framed so the plants sit in the clear band between them. The Map view keeps its own band, because a flat drawing needs its edges clear.

- **3D or Map.** The switch at the top right swaps the scene for the 2D garden map ([screens §3.8](../frontend/screens.md#38-garden-map)): beds with their names and areas, a pin per plant in its status shape and colour, the canopy ring where it was measured, a 1 m scale bar and north, and **Edit** for editors. The map shares the selection, the date and the modes, so Growth puts heights on the pins and Frost rings the tender plants in the info colour. It draws without WebGL, so when the 3D view can't draw (no WebGL, or three.js didn't load) the garden opens on the map. Distances between pins are given to the nearest 5 cm because pins are tapped in AR. The map works in every direction, not only Conservatory.

Unlike capture and the live analysis, the 3D garden follows the light or dark scheme, because it is not a camera. Its cards are solid, and its controls are neutral, as on the camera: a white shutter, and solid mode and view switches with the chosen one in the text colour. The modes follow the date: Health and Growth show the latest scan, Frost shows Thursday, and dragging the date to Thursday chooses Frost. The demo flow taps FROST when it reaches the forecast.

**The diagnosis, after Apple Music.** The diagnosis follows Apple Music's reimagined artist profiles (iOS 27, from a screenshot the team shared) instead of the furniture concept's product page.

| In Apple Music | In PDM |
|---|---|
| The artist photo runs into a page colour taken from the photo | **Photo tint:** the scan photo fades into a ground tinted from it, so every scan has its own colour. Only Back and In the garden (a map, where Apple Music has More) float over the photo, as clear glass bar buttons |
| The name, centred over the photo's faded edge | The diagnosis and its basis ("Tomato 2 · from 3 photos, your answer and the weather"), in Urbanist at 40 pt |
| Three round buttons: favourite, play, more | Left out. Step by step and Ask are already in the mini player, and In the garden is the bar button at the top right |
| The featured album, a card with the next one peeking | The risk of spread ("Crowded: 40 cm from Tomato 1") and "Also found: ladybug larva", scrolling sideways |
| "Top Songs ›" on the tint, without cards | "Your plan ›" and the other lists on translucent cards over the tint, with no shadows |
| The mini player above the tab bar | "Up next · step 1 of 5: Remove the lowest infected leaves", with play (step by step) and Ask. It is chrome, the iOS 26 tab bar's bottom accessory, so it may be glass. Whether `NativeTabs` exposes the accessory on SDK 57 is *unverified*; without it, the bar is ours and not glass |

The cards are solid fills, not glass: glass stays in the chrome.

**Photo tint rule.** The tint takes the photo's dominant hue and holds its lightness and chroma fixed: about OKLCH L 0.93 in light and 0.24 in dark, chroma at most 0.03. That keeps the ground in one narrow band, so F0's text and status colours stay readable on any photo. For the mockups' leaf the tint is the sage wash (`#E3EBDC`, `#18231D`). Checked on it: every text and status colour passes 4.5:1 on the tint and on the translucent cards (lowest 4.74:1, `healthy` on a light card). Other hues have not been checked; a tomato-red photo gives a faint pink ground, still near the same lightness.

**Palette.** F0's palette unchanged (`app/src/ui/tokens.ts` on `feat/app-scaffold`; `F0_COLORS` in [mockups/directions.js](mockups/directions.js), which both directions share so they cannot drift). The only addition is `wash`, the pale sage behind hero photos and the tint of the mockups' scan screens: `#E3EBDC` in light, `#18231D` in dark. Text on it is 13.5:1 and `textMuted` 5.22:1 in light (13.6:1 and 7.05:1 in dark). Conservatory also inherits F0's two known issues: accent and healthy are nearly one green, and watch and sick look alike to colour-blind users. The status shapes (section 6) carry the difference.

**Glass tint.** As in F0: the green tints the selected tab and the one prominent button; nothing tinted sits over a photo.

**Motion.** Slow and damped, never bouncy, because premium reads as unhurried: screens enter on a critically damped spring (stiffness 170, damping 26) rising 24 pt; stage ticks take 300 ms; pushes and sheets use the iOS sheet curve `bezier(0.32, 0.72, 0, 1)` over 420 ms. Reduced: fades, and the hotspot doesn't breathe.

**3D garden: the showroom.** The garden on a pale sage plinth over F0's off-white, framed between the top card and the plant card: plaster beds, terracotta-brown soil and pots, plants in their species greens with a status ring under each, a warm key light with soft shadows, a 24° long lens (from 16.8 m, so the model doesn't distort) and a slow turntable rotation when idle.

**Risks.** It depends on the top photo more than any other direction: the melt and the photo tint look their best over a clean, well-lit leaf, and users photograph sick plants in shade and clutter. Luxury can also read as "for someone else" to a community-garden member ([philosophies §3.2](philosophies.md#32-editorial-and-luxury)); the plain wording rules (section 6) are what keep it friendly. Urbanist is used only at 22 pt and above, where its geometric shapes stay legible.

## 4. Recommendation

**Instrument, with Seed Packet's warmth in the words, not the colours.** The decision is the team's; this is the case for it.

- **It owns the gaps.** Showing the work and honesty are PDM's two strongest claims ([landscape §6](landscape.md#6-gaps-pdm-can-own)). Instrument's run log with stage times, the reticle with the measured size, and the comparison table for "Not sure yet" say both without a word of explanation.
- **It is the furthest from the category.** Every competitor is green or mint ([landscape §8](landscape.md#8-what-would-make-pdm-look-generic)). With no brand hue, every colour on screen means a status or a measurement, so the sick tomato is the one red thing in the 3D garden on video.
- **It works outdoors.** Black on white with Atkinson Hyperlegible is the most legible option in sun, and its numerals were designed not to be confused.
- **It is the cheapest to build well.** No illustrations to commission, no custom font cut, no shadows; the chrome is the system's untouched.
- **Its risk is coldness.** Warmth comes from the user's own photos, the 3D garden and the plain, kind wording of the plan ("This week: two small jobs"), which Seed Packet's research supplies.

Herbarium is the safe second: tasteful and quiet, but green. Seed Packet is the most memorable and the most work.

Conservatory is the most premium-looking and the closest to what the team liked. It is a different kind of choice: a surface more than a structure. The strongest combination may be **Conservatory's surface with Instrument's evidence**: the melt, the camera-style capture and analysis, the tinted diagnosis, the pill and the mini player, carrying Instrument's run log with stage times, measured sizes and the comparison table for "Not sure yet". The palette is a separate choice: Conservatory keeps F0's green, which keeps F0's two colour issues; Instrument's ink would fix them.

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
| **Chat** | Answers you can use, not only read: text with **answer blocks** built from the agent's tool results (the garden map with the 40 cm spacing, the humid hours before the scan, a frost-cover planner you toggle, Tomato 1's measured trend), tool chips while it works, sources, a remembered fact with Undo, and a card that adds a task only when you confirm. See [Chat: answers you can interact with](#chat-answers-you-can-interact-with) | CopilotKit's [Open Intelligent UI](https://github.com/CopilotKit/OpenIntelligentUI) |
| **Plan, step by step** | One step per screen in large type: when, why, how, its source, and a time phrase turned into named tasks ("Adds 5 tasks: Oct 20, 23, 26, 29, Nov 1") | Crouton's step mode ([inspiration §9.2](inspiration.md#92-crouton)) |
| **3D garden** | The garden built from the map, plants sized by measurement and coloured by health. The **date scrubber** runs from the first scan to today, Tomato 1 turns sick on Oct 11, Tomato 2 on Oct 17 after the live scan, and one step past today shows Thursday's frost with covers over the tender plants | Windy's and Moonlitt's timeline ([inspiration §5](inspiration.md#5-alerts-forecasts-and-time)); Polycam's measurement labels ([landscape §7](landscape.md#7-ideas-to-borrow)) |
| **Insights** | Tomato 1's health over time, spot cover 14% → 9% → 5% with range bars and the dates plan steps were done, height with its noise band, humid hours before the diagnosis, and Tomato 2, 40 cm away, where the same problem was also found today (worded "also found on", never "spread from") | [analytics §6](analytics.md#6-the-demos-analytics-moment) |

**The demo flow** in the mockups ("Play the flow") runs home → capture → live analysis → the answer → diagnosis and plan → a checked step → chat with its blocks and a confirmed task → the 3D garden scrubbed into the forecast → insights. It is about 65 seconds, a third of the video.

### Chat: answers you can interact with

[Open Intelligent UI](https://github.com/CopilotKit/OpenIntelligentUI) (CopilotKit, MIT, read on 2026-10-17) is a chat where, for each turn, the agent picks the presentation: a direct text answer, a native component (it uses A2UI for basic tables), or a custom interactive UI that the model writes as HTML, CSS and JavaScript and that runs in a sandboxed iframe. Its demos explain, compare and "make a tool for the moment": a 3D model with controls, a table that becomes a chart, a calculator, a map with pins. It is built for Next.js on the web with CopilotKit and AG-UI.

**What PDM takes:** an answer can be something you use, and the agent chooses per turn whether text is enough or a component helps. **What PDM leaves:** model-written UI code. Instead, the API builds **answer blocks**, native components from a fixed set, out of records its tools already returned:

- **Honesty.** Every number in a block is a record's (a measurement, a forecast hour, a map distance), never the model's arithmetic. The text answer cites the same records.
- **Safety.** Every string the model writes passes the dose check (contracts §8). A block has no free text of the model's, so nothing reaches the screen around the check.
- **The platform.** The app is React Native; a sandboxed HTML answer would need a WebView inside the chat on every platform.
- **One design.** Blocks use the app's composites and tokens, so a chat answer looks like the rest of the app in every direction.

| Block | Built from | What you can do in it |
|---|---|---|
| Garden map snippet | `get_garden_map`, `get_plant_measurements` | Tap a plant for its status and height; open the 3D garden |
| Weather hours | `get_scan` (its weather snapshot) or `get_forecast` | Tap a day for its hours over 90% humidity |
| Frost cover planner | `get_forecast`, `get_garden_map`, `get_species_profile` | Choose plants to cover; the summary updates; it leads to a `create_task` card |
| Measurement trend | `get_plant_measurements` | Read the points and their bands; open Insights |
| Candidates compared | `compare_lookalikes` | The two problems side by side with how to tell them apart |

Interactions inside a block are local. Anything that changes data still goes through a confirmation card (`create_task`, `complete_task`, `revise_plan`), as today. Each block carries a text summary for screen readers, and the text answer makes sense without it.

**Contract change needed (proposed, not in this PR):** today "tool calls and tool results are stored but never sent to the app" (contracts §8). Blocks need an `answer_block` SSE event `{ v: 1, block: AnswerBlock }`, `blocks: AnswerBlock[]` on `Message`, and `AnswerBlock` as a tagged union by `kind`, each holding the record it shows and the `call_id` of the tool result it came from. The agent picks a block with a read tool such as `show_block(kind, call_id)`, which the API validates against that turn's tool results. As AGENTS.md requires, this lands as a `domain` type change with its exact-JSON test and `contracts.md` in one pull request.

## 6. Rules every direction follows

- **Status is a word, a colour and a shape:** healthy ● circle, watch ▲ triangle, sick ◆ diamond, dead □ hollow square, on chips, 3D labels and the legend ([philosophies §8](philosophies.md#8-rules-shared-by-all-three)).
- **Wording** follows [screens §6](../frontend/screens.md#6-wording-for-uncertain-results): no number on a diagnosis, and bands only on findings and measurements. Charts follow [analytics §2](analytics.md#shared-rules) and [§7](analytics.md#7-privacy-and-wording): no extrapolation, trends only from enough points, and weather shown as a pattern, never a cause.
- **Contrast** is WCAG 2.2 AA: 4.5:1 for text, 3:1 for marks and control edges. Touch targets are at least 44 pt.
- **Every motion has a still version.** Reduced motion removes slides, springs, the grow-in and the auto-orbit; the stage state and the region box read in a still frame. The mockups have a Reduced switch to check this.
- **Haptics** mark real events only: a selection tick per stage, success on a checked task, a tick per scan date on the 3D scrubber. None on the web.
- **Glass only in the chrome.** No content card is glass.

## 7. The 3D garden

- **Data:** only what the garden map stores: bed outlines, plant pins in metres, canopy radius, and each plant's measured height ([architecture §6.8](../architecture/README.md#68-garden-map-lidar)). No mesh, photo or coordinates; the sheet says so. A plant with no measurement is drawn as "not measured", never with an invented height.
- **Stack:** RealityKit on Metal, as a native `GardenView` in the LiDAR module: an `ARView` with the camera off, procedural meshes, no model files or textures, and UIKit labels placed from the 3D positions each frame. The app's store drives it through props (date, mode, selection). The app is iPhone only, so it needs no cross-platform renderer; Expo Go and the web build show the 2D map instead. Nothing has run on the device yet ([garden-3d §7](garden-3d.md#7-recommendation)).
- **Risk:** the Swift work shares the LiDAR module's owner. The first task is a one-day spike on the iPhone. The fallback is react-three-fiber on expo-gl, which runs in Expo Go but has an open bug with three r163 or later, expo issue #51543 ([garden-3d §2](garden-3d.md#known-issues)).
- **Accessibility:** a plant list is the screen reader's view of the garden, and the 2D map stays the default and the fallback.
- **Performance:** about 10 beds and 40 plants, flat materials, one light, a contact shadow per plant, and no frames drawn while idle ([garden-3d §3](garden-3d.md#3-performance-budget)).
- **The mockup** uses three.js r128 as a browser global for convenience; the app draws the same scene with RealityKit.

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
4. **Libraries to add, each with its reason in its pull request** (not in [frontend §4](../frontend/README.md#4-libraries) yet): `expo-symbols` and `expo-glass-effect`. The 3D garden adds no library: RealityKit is part of iOS, and the view lives in the LiDAR module.
5. **Chat blocks:** an `AnswerBlock` renderer in `features/chat` with one composite per kind, after the contract change in section 5.
6. **Live Activity:** an ActivityKit widget extension, which needs a development build, and a decision on how it is added to the Expo project (open decision 4).
7. **The frontend roadmap** (PR #42) gains this design as the first item of F1, and F8 Polish checks every screen against this document.

## 10. Open decisions

1. **The direction.** Instrument is recommended (section 4), with Conservatory's surface on Instrument's evidence as the hybrid to consider. The team picks one, or one direction's structure with another's detail.
2. **The scan action's place.** A round button beside the native tab bar, as in the mockups, or a tab of its own. Check what `NativeTabs` offers on SDK 57 before building.
3. **Glass tint over photos.** Check on the iPhone whether a tinted control stays readable when the glass adapts to dark content (philosophies §9).
4. **Live Activity in Expo.** How the widget extension is added and whether it fits the build week; the Lock Screen design does not depend on it.
5. **The 3D spike.** Run the RealityKit `GardenView` spike first, beside the LiDAR day-1 check. If it slips, fall back to fiber on expo-gl, and pin three below r163 if expo-gl still blocks it (garden-3d §7).
6. **Fonts.** If Seed Packet is chosen, cut Fraunces instances with the soft axis set (philosophies §9).
7. **Answer blocks in chat.** Take the `answer_block` contract change (section 5) into F6, and agree the first set of block kinds with the agent work in backend S5.
8. **Conservatory and real photos.** If Conservatory is chosen, try the melt and the photo tint on ten real photos of sick leaves taken in the garden, not on the mockups' drawings, before building them, and check the tint's contrast on each.
9. **Where the photo tint is computed.** On the phone from the first photo (a library such as `react-native-image-colors`, not yet in frontend §4), or by the API at upload as a field on the photo, which is a contract change.
10. **Camera-style capture for every direction?** The mode bar, the analysis over the photo and the 3D garden's modes are Conservatory's in the mockups, but these screens are mostly chrome, so they could go with whichever direction is chosen. If the team takes them, update [screens §3.2, §3.3 and §3.8](../frontend/screens.md#32-capture).
