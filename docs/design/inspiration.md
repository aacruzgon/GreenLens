# PDM Design Inspiration

> **Status:** Draft v0.1, 2026-10-17. v0.1: first pass over 16 products outside the plant category, with the patterns to borrow, the screen each one improves, a motion and haptics list and the five ideas that matter most for the demo.

This document collects patterns from apps that are not plant or garden apps. It is a companion to [design.md](design.md) (PDM's own design), [philosophies.md](philosophies.md) (design philosophies in general) and [landscape.md](landscape.md) (plant and garden apps, plus CARROT Weather, Oura and Polycam), and to the screen specs in [screens](../frontend/screens.md). To avoid repeating `landscape.md`, this document leaves out Seek's ID meter, Oura's word labels, CARROT's alert tiers and Polycam's measurement pills, and goes deeper on products that solve one of PDM's problems in another field.

## Contents

1. [How to read this](#1-how-to-read-this)
2. [The products at a glance](#2-the-products-at-a-glance)
3. [Live status](#3-live-status)
4. [Field identification](#4-field-identification)
5. [Alerts, forecasts and time](#5-alerts-forecasts-and-time)
6. [Trends without a score](#6-trends-without-a-score)
7. [Capture and measuring](#7-capture-and-measuring)
8. [Spaces in 3D](#8-spaces-in-3d)
9. [Checklists and doing the work](#9-checklists-and-doing-the-work)
10. [Review queues](#10-review-queues)
11. [Screen by screen](#11-screen-by-screen)
12. [Motion and haptics](#12-motion-and-haptics)
13. [The five strongest ideas for the demo](#13-the-five-strongest-ideas-for-the-demo)
14. [Sources](#14-sources)

## 1. How to read this

- Each product has what it is, its philosophy in two lines, a link, one to three patterns with the PDM screen they improve, and a caution.
- Screen names follow [screens](../frontend/screens.md): garden home (3.1), capture (3.2), scan results (3.3, which holds the live analysis, the question card and the diagnosis), plan (3.4), plant detail and timeline (3.5, the insights), chat (3.6), tasks and alert inbox (3.7), garden map (3.8, the 3D garden), members (3.9) and onboarding (3.10).
- Nothing here was run on a device. A claim taken from the product's own pages, Apple's newsroom, Apple's Human Interface Guidelines (HIG) or the team's own help pages is stated plainly. A claim from a third-party review, a showcase site or memory is marked *unverified*.
- PDM's rules still win: no confidence number on the diagnosis, bands only on findings, a source on every step of a plan.

## 2. The products at a glance

| # | Product | Field | Award | What PDM takes |
|---|---|---|---|---|
| 3.1 | [Flighty](https://apps.apple.com/ca/app/flighty-live-flight-tracker/id1358823008) | Flight tracking | ADA 2023, Interaction | A phase, how long it has lasted and what comes next; the cause behind an alert |
| 3.2 | [Uber](https://apps.apple.com/ca/app/uber-rides-eats-and-more/id368677368) and the Live Activity pattern | Rideshare | – | When a Live Activity starts, updates, alerts and ends |
| 4.1 | [Merlin Bird ID](https://apps.apple.com/ca/app/merlin-bird-id-by-cornell-lab/id773457673) | Bird ID | – | Questions answered with pictures; a results list that grows and re-highlights |
| 5.1 | [Watch Duty](https://apps.apple.com/us/app/watch-duty-wildfire-floods/id1574452924) | Wildfire and flood alerts | ADA 2025, Social Impact | Alerts with a lifecycle, vetted before they go out |
| 5.2 | [Windy.com](https://apps.apple.com/ca/app/windy-com-weather-radar/id1161387262) | Weather maps | – | One timeline from the past into the forecast, with play |
| 5.3 | [Tide Guide](https://apps.apple.com/ca/app/tide-guide-charts-tables/id1406371071) | Tides | ADA 2026, Visuals and Graphics | Full-screen charts you press and hold to read |
| 5.4 | [Moonlitt](https://apps.apple.com/ca/app/moonlitt-moon-phase-calendar/id6444718902) | Moon phases | ADA 2026, Interaction | A time slider tuned for feel, with a tap under the finger |
| 6.1 | [Gentler Streak](https://apps.apple.com/ca/app/gentler-streak-workout-tracker/id1576857102) | Fitness | ADA 2024, Social Impact | A healthy range instead of a target; rest that doesn't break the streak |
| 6.2 | [Apple Fitness Trends](https://support.apple.com/en-us/105003) | Health and fitness | – | Recent average against the longer average, as one arrow |
| 7.1 | [Apple Object Capture](https://developer.apple.com/videos/play/wwdc2023/10191) | 3D capture | – | A dial that fills as coverage is gathered, with spoken-style tips |
| 7.2 | [Apple Measure](https://support.apple.com/en-ca/guide/iphone/iphd8ac2cfea/ios) | Measuring | – | Auto-detected box to tap; a ruler that appears when you get close |
| 7.3 | [(Not Boring) Camera](https://apps.apple.com/ca/app/not-boring-camera/id6737783441) | Camera | ADA 2026 finalist, Visuals and Graphics | A shutter and dial that feel physical |
| 8.1 | [Matterport](https://apps.apple.com/ca/app/matterport/id701086043) | Spaces in 3D | – | Dollhouse and floor-plan views of one model, with labels in both |
| 9.1 | [Things 3](https://apps.apple.com/ca/app/things-3/id904237743) | Tasks | ADA 2017 | A checklist inside a to-do; details tucked away until needed |
| 9.2 | [Crouton](https://apps.apple.com/ca/app/crouton-recipe-manager/id1461650987) | Recipes | ADA 2024, Interaction | One step at a time, with times turned into timers |
| 10.1 | [Copilot Money](https://apps.apple.com/us/app/copilot-track-budget-money/id1447330651) | Personal finance | ADA 2024 finalist | A calm "To Review" queue that teaches the model |

Apple Design Award (ADA) results are from Apple's newsroom for [2024](https://www.apple.com/newsroom/2024/06/apple-announces-winners-of-the-2024-apple-design-awards/), [2025](https://www.apple.com/newsroom/2025/06/apple-unveils-winners-and-finalists-of-the-2025-apple-design-awards/) and [2026](https://www.apple.com/newsroom/2026/06/apple-reveals-winners-of-the-2026-apple-design-awards/), and Apple's [2023 winners page](https://developer.apple.com/design/awards/2023/). Things 3's 2017 award is from memory (*unverified*).

## 3. Live status

### 3.1 Flighty

**What it is.** A flight tracker that follows each leg of a trip, from the day before to the gate at the other end. [Website](https://flighty.com).

**Philosophy.** "Pilot-grade data that you can actually understand." Tell people first, say why, then get out of the way so they can "finally just enjoy your trip".

**Borrow.**

1. **Phase, time in phase, next milestone.** Flighty's status reads "Taxiing for 13m, for take off at 5:34PM" ([flighty.com](https://flighty.com)): what is happening, how long it has lasted, what happens next. **Live analysis (3.3):** the running stage reads "Checking 9 possible problems · 6 s" with "Next: diagnosis" under it, instead of a bare spinner. When a stage runs long, the line says so ("Still checking. This one is taking longer than usual"), as a delayed flight does.
2. **The earliest heads-up, with its cause.** Flighty tracks the inbound aircraft 25 hours out and predicts late-arrival delays "up to 6 hours before the airline says anything", and names causes such as ground stops and weather ([flighty.com](https://flighty.com)). **Alerts (3.7):** a frost alert leads with the time and the cause: "Frost likely Thursday 5 am, −2 °C. Clear sky and calm wind." The cause is what makes an early alert believable.
3. **A fixed set of states, each with its own screen layout.** A third-party teardown lists 15 states from "far out" to "completed" (*unverified*, [Crosley](https://blakecrosley.com/guides/design/flighty)). **Garden home (3.1):** a plant card changes shape with the scan state (queued, analysing, needs your answer, done, low evidence), rather than adding a badge to one layout.

**Caution.** Flighty shows probability ranges for groundings. Bands on a forecast risk are fine; the same treatment on the diagnosis is not.

### 3.2 Uber and the Live Activity pattern

**What it is.** The rideshare trip on the Lock Screen and in the Dynamic Island. Apple's HIG uses the rideshare trip as its main example of a Live Activity's lifecycle ([HIG: Live Activities](https://developer.apple.com/design/human-interface-guidelines/live-activities#Starting-updating-and-ending-a-Live-Activity)). Uber's own layout is not described here (*unverified*).

**Philosophy.** A task the person just started stays in view until it ends, and then leaves.

**Borrow.** `landscape.md` already suggests a Live Activity while a scan runs; these are the rules for running it well.

1. **Start, update, alert and end on the HIG's terms.** Start when the person expects it (on submit). Update "only when new content is available" (once per stage). Alert "only for essential updates": for PDM that is a question card and the final result. Don't send a push notification for the same update. End at once when the scan finishes, and keep the summary for a custom dismissal time; the HIG says "in most cases, 15 to 30 minutes is adequate" ([HIG](https://developer.apple.com/design/human-interface-guidelines/live-activities#Starting-updating-and-ending-a-Live-Activity)). **Live analysis and question card (3.3).**
2. **Animate existing elements to their new positions.** The HIG asks to keep the layout and move elements, not remove and re-add them, with animations of at most 2 s ([HIG](https://developer.apple.com/design/human-interface-guidelines/live-activities#Adding-transitions-and-animating-content-updates)). **Scan results (3.3):** the screen grows as sections arrive; existing sections slide down rather than jump.
3. **One activity for several events.** The HIG prefers one Live Activity that rotates through events. **Garden home:** two scans running in one garden share one activity ("2 scans · Tomato 1 needs an answer").

**Caution.** A Live Activity needs a native widget extension; check [docs/frontend](../frontend/README.md) before promising it in the demo.

## 4. Field identification

### 4.1 Merlin Bird ID

**What it is.** The Cornell Lab's bird ID app: by photo, by sound or by a short questionnaire. [Step by Step on the web](https://merlin.allaboutbirds.org).

**Philosophy.** Ask the few questions a birder would ask, in terms a beginner can answer, then show candidates with real photos.

**Borrow.**

1. **Answers as pictures on a scale.** Step by Step asks size against four reference birds (sparrow, robin, dove, goose) with options between them, then up to three colours from swatches, then what the bird was doing ([Pratt critique](https://ixd.prattsi.org/2023/09/design-critique-merlin-bird-id-app/), [Birda review](https://birda.org/merlin-a-comprehensive-app-review/)). The critique credits the visual scale with "reducing error drastically". **Question card (3.3):** "Do the spots have rings, like a target?" shows two small reference crops (rings, no rings) as the answer buttons, plus "Not sure". Older gardeners and second-language users answer a picture faster than a term.
2. **A list that grows, and re-highlights what it hears again.** Sound ID shows the live spectrogram on top; species appear in a list below as they are identified, and a species heard again is highlighted ([Cornell Chronicle](https://news.cornell.edu/node/321931), [Macaulay Library](https://www.macaulaylibrary.org/?p=6741)). **Live analysis (3.3):** findings appear as stages finish ("Leaf spots", "Ladybug larva"), and when a later stage confirms a finding, that row pulses once instead of a new row appearing.
3. **Jump back to the moment.** After recording, you select a species and jump to where its song is in the recording. **Chat (3.6) and diagnosis:** a finding named in a chat answer or a plan step is a link that opens the photo with that region highlighted.

**Caution.** The Pratt critique found first-time users confused about which ID mode to choose. PDM's question card must say why it is asking ("This tells early blight from Septoria").

## 5. Alerts, forecasts and time

### 5.1 Watch Duty

**What it is.** A nonprofit wildfire and flood alert app with a live map, run by volunteer dispatchers, firefighters and reporters. [How it works](https://watchduty.org/how-it-works/overview).

**Philosophy.** Fast and calm: an automated trigger, checked by a person, sent only when life or property is at risk. Apple cited "clarity and reliability".

**Borrow.**

1. **An alert has a lifecycle.** Watch Duty notifies "if there is a potential threat to life or property" and updates "until it is no longer a threat" ([how it works](https://watchduty.org/how-it-works/overview)). **Alerts (3.7):** each alert is a thread: issued ("Frost likely Thursday 5 am"), updated ("Now −3 °C, cover the tomatoes by 9 pm"), ended ("Frost risk over. Lowest was −1 °C at 6 am"). The end message closes the loop and is what makes people trust the next one.
2. **Opt in by place, read in one feed.** Users pick counties; everything arrives in "All Alerts, One Feed" ([watchduty.org](https://www.watchduty.org)). **Alert inbox (3.7) and members (3.9):** alerts are per garden, and a member of three community gardens sees one feed, each row tagged with its garden.
3. **Machine trigger, human check.** An automated system hears the dispatch, a reporter checks cameras and official sources, then the alert goes out ([how it works](https://watchduty.org/how-it-works/overview)). **Shared gardens:** when a member's scan finds a disease that spreads, the "seen nearby" alert to other members can wait for a reviewer's confirmation, and says who confirmed it.

**Caution.** A user on a wildfire forum objected to an alert based on radio traffic rather than an official report (*unverified*; seen only in a search summary). Every PDM alert names its source.

### 5.2 Windy.com

**What it is.** A weather map with layers (wind, rain, temperature) and a timeline. [Website](https://www.windy.com).

**Philosophy.** The map is the forecast; time is a control you hold.

**Borrow.**

1. **One timeline from the past into the forecast, with play.** A slider at the bottom runs from the last hour into the forecast, with a play button ([Windy Community](https://community.windy.com/topic/34152/timeline-graphic-on-bottom-of-windy-app-screen), *unverified* beyond forum descriptions). **3D garden (3.8):** one scrubber runs from the first scan to seven days ahead. Left of "now", plants grow to their measured heights; right of "now", frost and humid stretches tint the beds.
2. **Learn from its complaints.** Users ask for smoother animation because it "jumps at each one hour forecast transition", and report panels covering the slider and a collapsed timeline that is hard to reopen ([feature request](https://community.windy.com/topic/45045/feature-request-wind-animation-over-time), [play button](https://community.windy.com/topic/41035/how-to-play-the-forecast-animation-when-your-favorites-overlay-blocks-the-button)). **3D garden:** interpolate plant heights between scans; never let a sheet cover the scrubber; don't let it collapse.

**Caution.** Windy is dense. PDM shows one layer at a time by default (health colour), with the forecast as a tint, not a second legend.

### 5.3 Tide Guide

**What it is.** Tide charts and tables for any station. ADA 2026 winner for Visuals and Graphics. [App Store](https://apps.apple.com/ca/app/tide-guide-charts-tables/id1406371071).

**Philosophy.** One curve, full screen, beautiful enough to read for pleasure. Apple cited "full-screen charts filled with custom animations", Liquid Glass and a palette that matches the sky ([Apple newsroom](https://www.apple.com/newsroom/2026/06/apple-reveals-winners-of-the-2026-apple-design-awards/)).

**Borrow.**

1. **Press and hold to read the curve.** Holding the chart scrubs through the timeline and shows the height at any minute ([Screensdesign](https://screensdesign.com/showcase/tide-guide-charts-tables), *unverified*). **Insights (3.5):** the growth curve is full width; press and hold to read "62 cm · Oct 4 · measured" at any scan, with a selection haptic at each scan point.
2. **Press a place, get its curve.** Pressing a point on the map shows a mini tide graph for that point at the bottom (*unverified*, same source). **3D garden (3.8):** pressing a plant shows a small sheet with its height curve and health timeline, without leaving the map.
3. **Colour that follows the sky.** Apple credits the "sky-matching palette". **Garden home:** the header can shift with the local time and weather at the garden, a quiet sign the app knows the conditions.

**Caution.** A sky palette must keep text contrast in direct sunlight; test outdoors before shipping it.

### 5.4 Moonlitt

**What it is.** A moon phase tracker. ADA 2026 winner for Interaction. [App Store](https://apps.apple.com/ca/app/moonlitt-moon-phase-calendar/id6444718902).

**Philosophy.** Apple cited "an elegant interface", "best-in-class Liquid Glass integration" and easy onboarding. The team says it spent "months" tuning slider feel, haptics and animation ([gori.me](https://gori.me/en/apps/iphone-app-news/167222/)).

**Borrow.**

1. **A time slider that moves the subject, with a tap under the finger.** Dragging moves time and the moon "glides along its path"; "the tiny vibration that taps back at your fingertip" marks the movement ([gori.me](https://gori.me/en/apps/iphone-app-news/167222/)). **3D garden (3.8):** the growth scrubber plays a selection haptic at each scan date and a stronger tap at "now".
2. **Peek ahead.** You can see how the moon will look on any date. **3D garden:** dragging past "now" previews the forecast on the beds (see 5.2), with a label that says it is a forecast.

**Caution.** The polish took months. For the hackathon, tune one control (the scrubber) well rather than many.

## 6. Trends without a score

### 6.1 Gentler Streak

**What it is.** A fitness tracker built around a personal healthy range rather than daily targets. ADA 2024 winner for Social Impact. [Website](https://gentlerstories.com/gentlerstreak/).

**Philosophy.** "No guilt, no punishment"; it "meets you where you are". Apple praised its "optimistic and encouraging vibe" in place of "insistent reminders".

**Borrow.**

1. **A range, not a target.** The Activity Path shows a recommended range, and a streak forms while you stay within it ([BGR](https://bgr.com/tech/gentler-streak-gets-a-major-redesign-focused-on-your-wellbeing/), [TapSmart](https://www.tapsmart.com/features/better-streaks-apps/)). **Insights (3.5):** a plant's height sits on a shaded band of typical heights for that species at that age, when the knowledge base has one. Inside the band needs no comment; outside it gets one sentence.
2. **Rest is part of the plan.** "Taking a break never means starting over", and users can set a status when injured, sick or on a break ([gentlerstories.com](https://gentlerstories.com/gentlerstreak/)). **Tasks (3.7) and garden home:** "Garden resting for winter" pauses task reminders and frost alerts without hiding history. A task skipped because it rained is "Skipped · rain", not overdue. This matters in Canada, where most gardens sleep for five months.

**Caution.** No streaks for plant care. A missed watering is not a moral failure, and a streak would invite exactly the guilt the app avoids.

### 6.2 Apple Fitness Trends

**What it is.** The Trends view in Apple's Fitness app; Apple Health has a similar trends view. [Apple support](https://support.apple.com/en-us/105003).

**Philosophy.** One comparison anyone can read: your last 90 days against the last 365, as an arrow per metric ([Apple support](https://support.apple.com/en-us/105003), [Macworld](https://www.macworld.com/article/233093/ios-13-and-apple-watch-activity-trends-give-you-the-big-picture.html)). Trends wait for enough history: 180 days of activity before they start.

**Borrow.**

1. **Recent against longer, one arrow, one sentence.** **Insights (3.5) and plant detail:** "Growing faster than last week" with an up arrow, on measurements only (height, leaf size, scans with a problem). For seasonal crops the comparison is the same bed last season, not the past year.
2. **Use one method everywhere.** Dr. Drang found the plotted average and the trend value computed differently, and bars for one metric but dots for another ([leancrew](https://leancrew.com/all-this/2024/11/apple-health-trends/)). **Insights:** one chart style per kind of value, and the number in the sentence is the one on the chart.

3. **Say when there isn't enough history yet.** **Insights empty state:** "Trends start after three measured scans of this plant", rather than a flat line drawn from one point.

**Caution.** An arrow on "health" becomes a score. Arrows go on measurements; health stays in words.

## 7. Capture and measuring

### 7.1 Apple Object Capture

**What it is.** Apple's guided 3D capture on iPhone, shown at WWDC23 and used by many scanning apps. [WWDC23 session](https://developer.apple.com/videos/play/wwdc2023/10191).

**Philosophy.** The camera decides when a shot is good; the person only has to move.

**Borrow.**

1. **A dial that fills with coverage.** A dial around the object shows which areas still need images; the app guides you to them and takes the shots itself ([WWDC Notes](https://wwdcnotes.com/documentation/wwdc23-10191-meet-object-capture-for-ios/)). **Capture (3.2):** a ring split into the shots a scan needs (whole plant, close-up, soil; LiDAR sweep when the phone has one). A segment fills when its shot is accepted. Optional segments are drawn lighter.
2. **Tips in words, while you move.** The guide asks for more light, asks you to move slower and shows an arrow to centre the object. **Capture:** one tip at a time at the top ("Closer to the spotted leaf", "Too dark, find shade or light"), never a stack.
3. **Preview before you finish.** A point-cloud view shows what was captured before you commit. **Capture:** before submitting, a strip of the shots taken with their type, so a bad close-up can be retaken.

**Caution.** Object Capture needs three passes. A plant scan should take under 30 seconds; the ring must never make optional shots feel required.

### 7.2 Apple Measure

**What it is.** The built-in measuring app. [Apple support](https://support.apple.com/en-ca/guide/iphone/iphd8ac2cfea/ios).

**Philosophy.** Do the obvious measurement for the person, and reveal detail only when they lean in.

**Borrow.**

1. **Detect, frame, tap.** "When iPhone detects the edges of a rectangular object, a white box frames the object; tap the white box" to measure it ([Apple support](https://support.apple.com/en-ca/guide/iphone/iphd8ac2cfea/ios)). **Capture and scan results:** when the leaf detector finds a leaf, a white outline frames it; a tap measures it in mm.
2. **A ruler appears when you get close.** "Move iPhone closer to the measurement line until it transforms into a ruler, showing incremental units of length" (same page). **Scan results (3.3):** a measurement line on the photo shows only its value; pinch in and millimetre ticks appear, so the "measured" badge can be checked by eye.

**Caution.** Measure is for one object at a time. PDM measures several; label each value on its object, not in a list away from the photo.

### 7.3 (Not Boring) Camera

**What it is.** A camera app by Andy Allen whose controls are 3D objects with shadows, sounds and custom haptics. ADA 2026 finalist for Visuals and Graphics. [App Store](https://apps.apple.com/ca/app/not-boring-camera/id6737783441).

**Philosophy.** A camera should feel like an object: a shutter that "clicks and buzzes", a "satisfyingly clicky virtual dial" ([TapSmart](https://www.tapsmart.com/apps/not-boring-camera-review/), [Cool Curation](https://coolcuration.com/not-boring-camera-app-review-uk), *unverified*).

**Borrow.**

1. **A shutter you can feel.** **Capture (3.2):** one firm impact haptic and a short sound when a shot is accepted, so a user in bright sun, not looking at the screen, knows it worked.
2. **A detented dial for shot type.** **Capture:** whole, close-up and soil as detents on a thumb-reachable dial, with a selection haptic per detent, instead of a small segmented control.

**Caution.** Reviewers say the "unapologetic vibrancy" is not for daily use, and the app is built on SceneKit, which PDM does not have. Borrow the feel, not the look. The HIG warns that haptics can disturb the camera, so play the tap after the frame is taken ([HIG: Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics#Best-practices)).

## 8. Spaces in 3D

### 8.1 Matterport

**What it is.** 3D tours of buildings made from scans, viewed as a walk-through, a "dollhouse" (the whole model, tilted) or a floor plan (top-down). [Dollhouse upgrade](https://matterport.com/blog/dollhouse-view-just-got-upgrade).

**Philosophy.** One model, several ways in; the overview always one tap away.

**Borrow.**

1. **Two views of one model.** **3D garden (3.8):** the same scene as a top-down plan (beds as shapes, for editing the map) and a tilted dollhouse view (plants as sized, coloured objects, for looking). The camera moves between them rather than cutting, so the user keeps their bearings.
2. **Labels in both views.** Matterport added room labels to the dollhouse, which were once only in the floor plan ([blog](https://matterport.com/blog/dollhouse-view-just-got-upgrade)). **3D garden:** bed and plant names stay readable in the tilted view.
3. **Pick a level from the side.** Viewers select floors by tapping them from the side. **3D garden:** for a farm with several plots, a vertical list of plots at the edge; tapping one flies the camera there.

**Caution.** Free orbit is easy to get lost in. Limit the tilt range and add a "Reset view" control; the HIG's visionOS notes on motion warn that rotating a whole world unsettles people, and the same holds on a phone held at arm's length ([HIG: Motion](https://developer.apple.com/design/human-interface-guidelines/motion)).

## 9. Checklists and doing the work

### 9.1 Things 3

**What it is.** A task manager by Cultured Code. [Features](https://culturedcode.com/things/features/).

**Philosophy.** Content first; extra fields "neatly tucked away in the corner until you need them".

**Borrow.**

1. **A checklist inside a to-do.** Checklists "help you break down the finer details of a to-do" without making it a project ([features](https://culturedcode.com/things/features/)). **Plan (3.4):** a step such as "Remove the spotted lower leaves" carries its sub-steps (clean the shears, bag the leaves, don't compost them), each checkable, while the plan stays three steps long.
2. **Headings as groups.** Headings create "categories, milestones, or whatever you need". **Plan:** "Today", "This week" and "Watch for" as headings, so urgency is the structure.
3. **Details tucked away.** **Plan:** each step shows one line; its citation, dose and safety note open on tap. The source link is always visible as a small label, the rest is not.

**Caution.** Things is a personal tool. PDM's plan is shared; show who checked a step and when.

### 9.2 Crouton

**What it is.** A recipe manager with a cooking mode. ADA 2024 winner for Interaction. [App Store](https://apps.apple.com/ca/app/crouton-recipe-manager/id1461650987).

**Philosophy.** The phone is propped on a counter and your hands are messy; show one step, large, and do the bookkeeping for the cook.

**Borrow.**

1. **One step at a time.** A "step by step mode" shows one step per screen, and tapping an ingredient in the step shows its quantity ([App Store](https://apps.apple.com/ca/app/crouton-recipe-manager/id1461650987)). **Plan (3.4):** a "Do it now" mode for working in the garden: one step per screen in large type, a big Done button at thumb height, and products named in the step tappable for the amount for this bed size.
2. **Times in the text become timers.** Crouton detects times in steps and starts "self-naming timers" with a tap. **Plan and tasks (3.7):** "Check again in 7 days" in a step becomes a one-tap task named "Re-check Tomato 1 lower leaves", due Oct 25.
3. **Hands-free.** Moving between steps "without touching your device", and thumb-pointing gestures in the latest version. **Plan:** large tap targets at thumb height are enough for the demo; skip camera gestures, which a reviewer found "inconsistent at times".

**Caution.** Crouton's steps are written by a cook; PDM's come from a model. Each step in the mode still shows its source label.

## 10. Review queues

### 10.1 Copilot Money

**What it is.** A budgeting app that categorises transactions automatically. ADA 2024 finalist. [Help: Transactions](https://help.copilot.money/en/articles/9554412-transactions-tab-overview).

**Philosophy.** "Polished and deliberately calm", "automation first", with the person confirming rather than entering ([FAQ](https://www.copilot.money/faq)).

**Borrow.**

1. **A "To Review" queue.** New transactions land in a To Review section on the dashboard, marked until reviewed, and can be reviewed in bulk ([help](https://help.copilot.money/en/articles/9554412-transactions-tab-overview), [Mac help](https://help.copilot.money/en/articles/6778561-copilot-for-macos)). **Garden home and members (3.9):** a "Needs you" row collects open question cards and species confirmations across all scans; for a community garden's reviewer, members' new scans waiting for confirmation.
2. **Reviews teach the system, and say so.** Category suggestions start after 30 reviewed transactions (*unverified*, help-centre summary). **Scan feedback (3.11):** after a correction, one line says what the correction changes, in the words the feedback flow actually supports.

**Caution.** A third-party review says Copilot shows the model's confidence per transaction (*unverified*). PDM does not copy that onto diagnoses.

## 11. Screen by screen

| PDM screen or moment | Best borrowed patterns |
|---|---|
| Garden home (3.1) | Card shape follows scan state (Flighty 3.1.3); "Needs you" queue (Copilot 10.1.1); "Resting for winter" status (Gentler Streak 6.1.2) |
| Capture (3.2) | Coverage ring per required shot (Object Capture 7.1.1); detect, frame, tap to measure (Measure 7.2.1); shutter haptic after the frame (Not Boring 7.3.1) |
| Live analysis (3.3) | Phase, time in phase, next step (Flighty 3.1.1); growing list that re-highlights (Merlin 4.1.2); Live Activity lifecycle (HIG 3.2.1) |
| Question card (3.3) | Picture answers on a scale (Merlin 4.1.1); say why it is asking (Merlin caution); the only mid-scan alert (HIG 3.2.1) |
| Diagnosis (3.3) | Findings link back to their region (Merlin 4.1.3); ruler ticks on measured values (Measure 7.2.2); details tucked away (Things 9.1.3) |
| Plan (3.4) | One-step "Do it now" mode (Crouton 9.2.1); checklist inside a step (Things 9.1.1); Today / This week / Watch for (Things 9.1.2) |
| Tasks (3.7) | Times in steps become named tasks (Crouton 9.2.2); "Skipped · rain" without guilt (Gentler Streak 6.1.2) |
| Alerts (3.7) | Issued, updated, ended thread (Watch Duty 5.1.1); lead with time and cause (Flighty 3.1.2); one feed across gardens (Watch Duty 5.1.2) |
| 3D garden (3.8) | Past-to-forecast scrubber with play (Windy 5.2.1); haptic tick per scan date (Moonlitt 5.4.1); plan and dollhouse views (Matterport 8.1.1) |
| Insights (3.5) | Press and hold the curve (Tide Guide 5.3.1); typical-height band (Gentler Streak 6.1.1); recent-against-longer arrow on measurements (Fitness 6.2.1) |
| Chat (3.6) | Named findings as links to the region (Merlin 4.1.3); plain cause in answers (Flighty 3.1.2) |
| Members and shared gardens (3.9) | Reviewer queue (Copilot 10.1.1); human-confirmed "seen nearby" alerts (Watch Duty 5.1.3) |
| Onboarding (3.10) | Ask the season and garden status first (Gentler Streak 6.1.2); explain each mode before use (Merlin caution) |
| Empty states | "No alerts. We'll tell you if frost is forecast" (Watch Duty 5.1.1); a resting garden is a state, not an empty screen (Gentler Streak 6.1.2); "Trends start after three measured scans" (Fitness 6.2.3) |

## 12. Motion and haptics

The HIG's rules shape all of this: use system haptics for their documented meanings, keep them short and tied to discrete events, don't overuse them, make them optional, and keep them away from the camera while it captures ([HIG: Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics)). Motion must be optional and cancellable, and never the only carrier of information ([HIG: Motion](https://developer.apple.com/design/human-interface-guidelines/motion)). The Expo calls are from `expo-haptics`.

| Moment | Motion | Haptic (Expo) | Duration | From |
|---|---|---|---|---|
| Shot accepted | Ring segment fills | `impactAsync(Medium)`, after the frame | Fill under 300 ms (suggested) | Not Boring 7.3, HIG |
| Shot type changed | Dial snaps to detent | `selectionAsync()` per detent | – | Not Boring 7.3, HIG selection |
| Stage finished | Running line turns to a tick; next line slides in | None; stages are frequent | Under 300 ms (suggested) | HIG: avoid motion on frequent events |
| Box drawn on the spot | Box traces in, then label | `impactAsync(Light)` once | Under 2 s | Live Activity limit, HIG |
| Question card appears | Card rises from below; earlier sections move down, not re-render | `impactAsync(Light)` | Under 2 s | HIG Live Activities |
| Finding confirmed again | Row pulses once | None | – | Merlin 4.1 |
| Scan done | Status chip changes text with a numeric-style transition | `notificationAsync(Success)` | – | HIG notification haptics |
| Low evidence or failed | Diagnosis block replaced in place | `notificationAsync(Warning)` / `(Error)` | – | HIG notification haptics |
| Scrubber passes a scan date | Plants grow, interpolated between scans | `selectionAsync()` per date; `impactAsync(Medium)` at "now" | Play: about 1 s per week of history (suggested) | Moonlitt 5.4, Windy 5.2 |
| Press and hold a chart | Value bubble follows the finger | `selectionAsync()` per data point | – | Tide Guide 5.3 |
| Plan step done | Tick, row dims | `selectionAsync()`; `notificationAsync(Success)` on the last step | – | Things 9.1 |
| Live Activity summary | Stays after the scan ends | – | 15–30 min dismissal | HIG Live Activities |

Durations that are documented: Live Activity animations of at most 2 s, and a 15 to 30 minute summary after the activity ends ([HIG](https://developer.apple.com/design/human-interface-guidelines/live-activities)). A teardown of Flighty gives a 0.3 s spring on status badges and a numeric text transition on changing numbers (*unverified*, [Crosley](https://blakecrosley.com/guides/design/flighty)). The rest are suggestions to tune on a device. Under Reduce Motion, replace movement with a cross-fade and keep the haptics, which then carry the event.

## 13. The five strongest ideas for the demo

Ranked by how much each one shows in a two-minute demo, where judges see capture, the live analysis, the result, the plan and the 3D garden.

1. **Flighty's phase line in the live analysis.** "Checking 9 possible problems · 6 s · Next: diagnosis", with stages ticking and the scan carried onto the Lock Screen by a Live Activity that alerts only for the question card. It makes the slowest part of the demo the most watchable, and shows the pipeline is real. *Live analysis, 3.3.*
2. **Windy's and Moonlitt's scrubber in the 3D garden.** Drag from the first scan to today: plants grow to their measured heights and change colour with their health, a tap under the finger at each scan, then past "now" into the frost forecast. Ten seconds of video that explains the whole product. *3D garden, 3.8.*
3. **Merlin's picture answers on the question card.** Two reference crops as the Yes and No buttons. It shows the app asking an honest question instead of guessing, which is PDM's central claim. *Question card, 3.3.*
4. **Object Capture's coverage ring at capture.** The ring fills segment by segment with a tap on each accepted shot, and a white frame snaps around a leaf to measure it in mm. It opens the demo with something physical and new. *Capture, 3.2.*
5. **Crouton's one-step mode for the plan.** One cited step per screen, large type, a timer phrase turned into a named task with one tap. It closes the demo on what the gardener does next, outdoors, one-handed. *Plan, 3.4.*

Next in line: Watch Duty's issued-updated-ended alert thread (alerts) and Gentler Streak's "Resting for winter" (garden home).

## 14. Sources

Checked on 2026-10-17.

- Apple: [2023 Design Award winners](https://developer.apple.com/design/awards/2023/); newsroom [2024](https://www.apple.com/newsroom/2024/06/apple-announces-winners-of-the-2024-apple-design-awards/), [2025](https://www.apple.com/newsroom/2025/06/apple-unveils-winners-and-finalists-of-the-2025-apple-design-awards/), [2026](https://www.apple.com/newsroom/2026/06/apple-reveals-winners-of-the-2026-apple-design-awards/).
- Apple HIG: [Live Activities](https://developer.apple.com/design/human-interface-guidelines/live-activities), [Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics), [Motion](https://developer.apple.com/design/human-interface-guidelines/motion) (read with `.claude/skills/hig/scripts/hig_page.py`).
- Apple support: [Measure dimensions](https://support.apple.com/en-ca/guide/iphone/iphd8ac2cfea/ios); [Track your trends in the Fitness app](https://support.apple.com/en-us/105003). WWDC23 [Meet Object Capture for iOS](https://developer.apple.com/videos/play/wwdc2023/10191) and [WWDC Notes summary](https://wwdcnotes.com/documentation/wwdc23-10191-meet-object-capture-for-ios/).
- Flighty: [flighty.com](https://flighty.com); [Blake Crosley teardown](https://blakecrosley.com/guides/design/flighty) (third party).
- Merlin: [Step by Step](https://merlin.allaboutbirds.org); [Pratt IxD critique](https://ixd.prattsi.org/2023/09/design-critique-merlin-bird-id-app/); [Birda review](https://birda.org/merlin-a-comprehensive-app-review/); [Cornell Chronicle on Sound ID](https://news.cornell.edu/node/321931); [Macaulay Library, behind Sound ID](https://www.macaulaylibrary.org/?p=6741).
- Watch Duty: [watchduty.org](https://www.watchduty.org); [How it works](https://watchduty.org/how-it-works/overview).
- Windy: [windy.com](https://www.windy.com); Windy Community threads on the [timeline](https://community.windy.com/topic/34152/timeline-graphic-on-bottom-of-windy-app-screen), [animation](https://community.windy.com/topic/45045/feature-request-wind-animation-over-time) and the [play button](https://community.windy.com/topic/41035/how-to-play-the-forecast-animation-when-your-favorites-overlay-blocks-the-button).
- Tide Guide: [App Store](https://apps.apple.com/ca/app/tide-guide-charts-tables/id1406371071); [Screensdesign showcase](https://screensdesign.com/showcase/tide-guide-charts-tables) (third party); [Forbes Spain, Nautik](https://forbes.es/en/nautik-magazine/946677/this-is-the-tide-app-that-apple-just-awarded-a-prize-to/).
- Moonlitt: [App Store](https://apps.apple.com/ca/app/moonlitt-moon-phase-calendar/id6444718902); [gori.me](https://gori.me/en/apps/iphone-app-news/167222/).
- Gentler Streak: [gentlerstories.com](https://gentlerstories.com/gentlerstreak/); [BGR redesign](https://bgr.com/tech/gentler-streak-gets-a-major-redesign-focused-on-your-wellbeing/); [TapSmart](https://www.tapsmart.com/features/better-streaks-apps/).
- Apple Fitness and Health Trends: [Macworld](https://www.macworld.com/article/233093/ios-13-and-apple-watch-activity-trends-give-you-the-big-picture.html); [Dr. Drang, leancrew](https://leancrew.com/all-this/2024/11/apple-health-trends/).
- (Not Boring) Camera: [App Store](https://apps.apple.com/ca/app/not-boring-camera/id6737783441); [TapSmart review](https://www.tapsmart.com/apps/not-boring-camera-review/); [Cool Curation review](https://coolcuration.com/not-boring-camera-app-review-uk).
- Matterport: [App Store](https://apps.apple.com/ca/app/matterport/id701086043); [Dollhouse View just got an upgrade](https://matterport.com/blog/dollhouse-view-just-got-upgrade).
- Things 3: [App Store](https://apps.apple.com/ca/app/things-3/id904237743); [features](https://culturedcode.com/things/features/).
- Crouton: [App Store](https://apps.apple.com/ca/app/crouton-recipe-manager/id1461650987).
- Copilot Money: [App Store](https://apps.apple.com/us/app/copilot-track-budget-money/id1447330651); [FAQ](https://www.copilot.money/faq); help: [Transactions tab](https://help.copilot.money/en/articles/9554412-transactions-tab-overview), [Copilot for macOS](https://help.copilot.money/en/articles/6778561-copilot-for-macos).
- Uber: [App Store](https://apps.apple.com/ca/app/uber-rides-eats-and-more/id368677368).
