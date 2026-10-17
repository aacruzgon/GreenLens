# PDM Design Landscape

> **Status:** Draft v0.1, 2026-10-17. v0.1: first review of 12 plant and garden apps and 3 apps from other categories, with the patterns PDM should match, the gaps it can own, ideas to borrow and the looks to avoid.

This document reviews the design of products PDM will be compared with, so the app's look and flows are chosen with the category in view. It covers concrete products only. It is a companion to [design.md](design.md) (PDM's own design) and [philosophies.md](philosophies.md) (design philosophies in general), in the same folder, and to the [screens](../frontend/screens.md).

## Contents

1. [Method](#1-method)
2. [The products at a glance](#2-the-products-at-a-glance)
3. [Plant and garden apps](#3-plant-and-garden-apps)
4. [Apps from other categories](#4-apps-from-other-categories)
5. [Table stakes](#5-table-stakes)
6. [Gaps PDM can own](#6-gaps-pdm-can-own)
7. [Ideas to borrow](#7-ideas-to-borrow)
8. [What would make PDM look generic](#8-what-would-make-pdm-look-generic)
9. [Sources](#9-sources)

## 1. Method

- **What was seen.** For each product: its Canadian App Store listing (or Google Play for Plantix), read through Apple's lookup API on 2026-10-17, and the official store screenshots, downloaded and viewed. Ratings and update dates below come from that lookup.
- **What was not.** No app was installed or run. Store screenshots are marketing: they show the flows the vendor chose to show, often staged. So "seen" in this document means seen in official screenshots or documents, not in use.
- **Colours.** Hex values are approximate. They were sampled from the store screenshots by quantising each image to six colours.
- **Unverified.** Claims taken only from reviews, review roundups or third-party summaries are marked *unverified*. Several of those sources are small affiliate or analytics sites; treat them as signals, not facts.

## 2. The products at a glance

| Product | Kind | Rating (CA store, count) | Last update | Diagnosis or ID result | Says how sure it is? |
|---|---|---|---|---|---|
| [PictureThis](#31-picturethis) | ID, diagnosis, care | 4.7 (117,429) | 2026-10-15 | One name; problem chips | No (claims "over 98% accuracy") |
| [Planta](#32-planta) | Care schedule, diagnosis | 4.7 (18,577) | 2026-10-12 | Chat with Dr. Planta, then a plan | No |
| [Greg](#33-greg) | Watering, ID, community | 4.4 (788) | 2026-06-04 | One name, AR scan | No |
| [Plantix](#34-plantix) | Crop diagnosis | 3.6 on Play (138,265) | 2026-10-16 | Label with leader line to the lesion | No |
| [iNaturalist and Seek](#35-inaturalist-and-seek) | Species ID, citizen science | iNat 3.8 (97); Seek 4.7 (2,968) | 2026-09 | Rank ladder; genus when unsure | Yes, by rank |
| [PlantNet](#36-plantnet) | Species ID, science | 4.6 (1,276) | 2026-10-07 | Ranked list with scores | Yes, by score |
| [Agrio](#37-agrio) | Farm diagnosis, alerts | 4.4 (48) | 2026-10-16 | "We're highly confident that it is:" | In words |
| [Plantum](#38-plantum) | ID, diagnosis, care tools | 4.5 (11,277) | 2026-09-02 | Category banner, loupe on lesion | No |
| [Blossom](#39-blossom) | ID, diagnosis, care | 4.5 (7,665) | 2026-03-19 | "Sick plant", causes, Cure button | No |
| [Gardenize](#310-gardenize) | Garden journal | 4.1 (29) | 2025-07-17 | Plant ID only | No |
| [Seed to Spoon](#311-seed-to-spoon) | Food-garden planner | 4.7 (306) | 2026-05-07 | Growbot chat | No |
| [Planter](#312-planter) | Square-foot planner | 4.7 (282) | 2026-08-27 | None | n/a |
| [CARROT Weather](#41-carrot-weather) | Weather | 4.6 (6,307) | 2026-10-09 | n/a | n/a |
| [Oura](#42-oura) | Health ring | 4.8 (16,662) | 2026-10-13 | Scores with word states | In words |
| [Polycam](#43-polycam) | LiDAR scanning | 4.6 (5,191) | 2026-10-13 | n/a | n/a |

Not reviewed: Google Lens, PlantIn, Plant Parent and GrowIt (Glority's sister apps to PictureThis). Lens returns generic visual matches rather than a plant result screen, and the others repeat PictureThis's pattern.

## 3. Plant and garden apps

### 3.1 PictureThis

[App Store](https://apps.apple.com/ca/app/picturethis-plant-identifier/id1252497129). The category leader by ratings. Identifies over 400,000 species, diagnoses disease, sets care reminders, warns of toxic plants and weeds, and sells a Pro subscription.

- **Look.** Saturated emerald green behind every store screenshot, white cards with large rounded corners, a heavy rounded sans for headings. Stock-style flower photos (dahlia, frangipani, monstera). A big green tick for "identified". Problems shown as pale red chips.
- **Result and uncertainty.** The diagnosis screenshot shows a single plant with three red chips: "Not enough light", "Overwatering", "Poor ventilation". The care screen uses verdict lines: "Light meter: Not enough light", "Repotting checker: Pot too small, it needs repotting!". No uncertainty is shown, and the listing claims "over 98% accuracy". Independent studies disagree: 82% correct at genus and 64% at species on toxic plants ([J Med Toxicol 2022](https://pmc.ncbi.nlm.nih.gov/articles/PMC8938528)). A review analysis says it gives one name and "little confidence disclosure", and calls the diagnosis overconfident ([Unstar, 2026](https://unstar.app/blog/plant-identification-apps-ranked-picturethis-plantin-plantnet-2026), *unverified*).
- **Learn from.** Care tools as small verdict cards, each with one plain sentence and a chevron. Units given as a concrete amount: "≈ 1 inch (25mm) of water per watering". Home remedies written as numbered steps with a picture.
- **Avoid.** The "Before / After" split image on a monstera leaf, which promises a cure. Ten in-app purchase tiers. Trial flows that convert to $30 to $40 a year are the top complaint in reviews (*unverified*). A paywall research site logged 17 observed paywall variants in July 2026 ([Lazyweb](https://experiments.lazyweb.com/research/picturethis-paywall-experiments.md), *unverified*).

### 3.2 Planta

[App Store](https://apps.apple.com/ca/app/planta-plant-garden-care/id1410126781). Swedish care-schedule app, now "Plant & Garden Care". The store screenshots carry "App of the Day" and "Editors' Choice 2026" laurels.

- **Look.** The calmest palette in the category: sage background (about #E1E7D9) with deep forest green (about #254724) for headings and primary buttons. Pill-shaped tabs and chips, real home photos rather than studio shots, a bold grotesque headline. Large plant photo at the top of each plant page.
- **Result and uncertainty.** Diagnosis is "Dr. Planta", a premium feature behind a stethoscope tab with "Dr Planta / Hospital / Explore" sections ([support](https://support.getplanta.com/how-can-i-find-dr-planta)). Planta describes it as a chat that confirms the problem and then gives a treatment plan. Its screenshot shows candidate problems as floating chips ("Lack of light", "Thrips", "Dry soil", "Underwatered") and a "Diagnose my plant" button, with a human "Plant expert team" as a fallback. How the result itself is worded is *unverified*.
- **Learn from.** The light meter's "Part sun / Facing West" overlay drawn on the live camera. "Todo / Care info / History" as three tabs on a plant. A "Hospital" for plants under treatment, so sick plants are a tracked state, not a one-off result.
- **Avoid.** Diagnosis behind a paywall and English only. Reviews cite the price and frequent marketing notifications (*unverified*).

### 3.3 Greg

[App Store](https://apps.apple.com/ca/app/greg-plant-identifier-care/id1512912236). Watering plans and a large community. "PlantVision" (Greg 2.0, December 2022) is a 3D scan that claims to identify the species and measure the pot and the distance to the nearest window ([Product Hunt launch](https://www.hunted.space/product/greg/launches/greg-2-0-plantvision)).

- **Look.** Near-black store backgrounds, mint (about #B0F4D4) and pale green in the app, illustrated leaves. A cartoon plant mascot with a face ("Keanu Leaves") and a big "I'm good to go" status bubble. Bright square feature icons.
- **Result and uncertainty.** The scan screenshot draws glowing blue rings around the plant and measurement tags on the pot ("8"") before a bottom sheet names it ("ZZ Plant") with three trait icons. No uncertainty.
- **Learn from.** The scan is the most spatial capture in the category: rings that show the phone is reading depth, and measurements attached to the object. The one-line status per plant ("I'm good to go / All caught up on care") answers "is it fine?" before any detail. Community posts appear under a plant question, so help is in context.
- **Avoid.** Inches only in the screenshots. The mascot voice ("plant babies", "whaddya say?") would undercut a serious diagnosis. Last updated June 2026.

### 3.4 Plantix

[Google Play](https://play.google.com/store/apps/details?id=com.peat.GartenBank&hl=en_CA). Crop doctor for farmers, Android only (no iOS app found in the Canadian store). Covers 30 crops and 780+ problems in 19 languages, 10M+ downloads. Free to farmers; the company earns from its recognition API ([FAO Agritech Observatory](https://agritechobservatory.review.fao.org/en/plantix-crop-doctor)).

- **Look.** Pale mint background (about #ECF7F5), white cards, a blue primary button, Material components. Real field photos of damaged leaves rather than staged houseplants.
- **Result and uncertainty.** The lead screenshot points at a lesion on the live camera with an orange leader line to three stacked labels: "Bacterial Spot" with a warning triangle, "Symptoms: Small yellow-green lesions", "Treatment: Tetracycline Chloride". No uncertainty, and the treatment named is an antibiotic.
- **Learn from.** The leader line tying a label to a spot on the photo. "Disease occurred nearby: There's been an increased number of cases in your area" with "Rising" and "Alert" tags on disease cards. A crop-cycle timeline ("Week 1–2, Seedling stage, Ongoing"). Fertiliser amounts in kg per bag type.
- **Avoid.** A chemical treatment on the first card, with no check that it is legal or right for the user. Play rating 3.6, the lowest here.

### 3.5 iNaturalist and Seek

[iNaturalist](https://apps.apple.com/ca/app/inaturalist/id6475737561) and [Seek](https://apps.apple.com/ca/app/seek-by-inaturalist/id1353224144), both from the nonprofit iNaturalist. iNaturalist turns photos into observations the community confirms; Seek is the private, kid-safe, offline camera.

- **Look.** iNaturalist: white lists, a lime green (about #6F9E34) for the camera button and verified marks, dense data rows, maps with orange observation grids. Seek: deep green (about #204034) headers, an all-caps display face, hexagon badges.
- **Result and uncertainty.** The best in the category. Seek's camera has an "ID meter" of seven dots, one per rank from kingdom to species. The dots fill as the model gets surer, and you can only add an observation when all seven fill. When it stalls, the guide tells you to try another angle or "If you are looking at a leaf, try looking at a flower", and otherwise to take the photo at the coarser rank ([Seek user guide](https://static.inaturalist.org/wiki_page_attachments/SeekUserGuide2020.pdf)). iNaturalist suggests a genus with "We're pretty sure this is in the genus…" when species is unclear, alongside species marked "visually similar" and "seen nearby" ([forum](https://forum.inaturalist.org/t/computer-vision-top-species-suggestion-doesnt-match-pretty-sure-genus-suggestion/75629)). Users report the genus and species rows look almost the same (*unverified*).
- **Learn from.** Uncertainty shown as progress the user can affect. A coarser answer stated plainly instead of a wrong precise one. "Native" and "Introduced" tags on species. Human confirmation shown on the observation ("Research grade").
- **Avoid.** The rebuilt iNaturalist app rates 3.8 against 4.6 for "iNaturalist Classic"; a reminder that a redesign can lose trust. Seek's badges and challenges suit children, not a farm.

### 3.6 PlantNet

[App Store](https://apps.apple.com/ca/app/plantnet/id600547573). Free, ad-free citizen-science identifier from French research institutes, about 20,000 species, strongest on wild European plants.

- **Look.** Utilitarian: white, a small green leaf logo, underlined Latin names, grids of reference photos. The store screenshots date from 2019 (an iOS "Carrier" status bar), so the current app may differ.
- **Result and uncertainty.** A ranked list of candidates. Each has a strip of reference photos ("+2" for more), the Latin and common name, the family, a score and a "Confirm" button. The listing tells users which organs to photograph and why. A review analysis calls the ranked list "the more honest approach" (*unverified*).
- **Learn from.** Reference photos beside each candidate, so the user compares rather than trusts. Asking for the flower, fruit or leaf by name.
- **Avoid.** Weak hierarchy: every candidate looks equal, and the score has no meaning for a lay user.

### 3.7 Agrio

[App Store](https://apps.apple.com/ca/app/agrio-plant-diagnosis-app/id1239193220). Diagnosis for growers and crop advisors, with satellite NDVI field monitoring, regional pest alerts, integrated pest management protocols, and an assistant named Harvie.

- **Look.** White and navy, a teal-green logo, stock Material lists, red warning triangles. Feels like a 2018 enterprise tool.
- **Result and uncertainty.** The result screenshot reads "We're highly confident that it is: *Aceria Oleae*", then a paragraph, then two collapsed sections: "Organic Treatment" and "Conventional Treatment". Confidence is stated in words. What it says when it is not confident is *unverified*.
- **Learn from.** A confidence phrase as the lead-in to the name. Organic and conventional treatments kept apart. Alerts that name the pest and the region ("Fruit fly detected in your region") with a map. Fields grouped by farm, for inspectors.
- **Avoid.** Alerts that read like system errors ("Dear Farmer, we identified anomalies in the field"). A red triangle on every row, so nothing stands out.

### 3.8 Plantum

[App Store](https://apps.apple.com/ca/app/plantum-ai-plant-identifier/id1476047194). Identifier with diagnosis (marked "Beta"), care schedules, a light meter and a "Pot Meter".

- **Look.** Green gradients, white cards, a dark teal (about #305A4E) primary and an orange call to action. Studio shots and a stock model watering a plant.
- **Result and uncertainty.** The diagnosis screenshot shows a pink banner, "Abiotic: Abiotic disorders are caused by nonfectious factors and unsuitable environmental conditions", and a circular loupe magnifying a brown leaf edge. A category, not a cause, and no uncertainty. "Common Problems" sits next to an "Ask Plant Expert / Get Consultation" upsell.
- **Learn from.** The loupe over a lesion. The Pot Meter labels height and width in centimetres on the photo ("Height 19 cm", "Width 12 cm") with a "Calculate Volume" button.
- **Avoid.** "Highly Accurate Plant Identifier" as a headline. Reviews describe a paywall after one free photo and charges after cancelled trials (*unverified*, [summary](https://spark.mwm.ai/kr/apps/plantum-ai-plant-identifier/1476047194)).

### 3.9 Blossom

[App Store](https://apps.apple.com/ca/app/blossom-ai-plant-identifier/id1487453649). Identifier, disease ID, care reminders, an "AI Botanist" and weather alerts. Webby People's Voice winner 2022.

- **Look.** Pale mint (about #EEF7F5) and soft yellow, a cartoon flower mascot with a face, rounded serif display headings ("Your GUIDE to happy plants").
- **Result and uncertainty.** "Sick plant: Peace lily. Overwatering and nutrient deficiency detected" with a "Cure" button, and a "Powered by AI" tab on the card. No uncertainty; "Cure" promises an outcome.
- **Learn from.** "Multisnap": several photos of one plant for a better ID. A care timeline with "Complete" and "Done" states and overdue items in red.
- **Avoid.** The Before/After split again, the mascot, "Cure".

### 3.10 Gardenize

[App Store](https://apps.apple.com/ca/app/gardenize-plant-care-gardening/id1118448120). Swedish garden journal: plants, areas, events, reminders, plant ID.

- **Look.** The most distinctive in the group: kraft-paper texture, off-white (about #F0EEE9), a typewriter monospace for labels ("BASIL", "CHERRY TREE"), dark green accents, hands in soil. A testimonial from Victoria and Vancouver.
- **Result and uncertainty.** ID only; no diagnosis.
- **Learn from.** "Remember what & where": coloured lines drawn on a photo of bare soil to mark where bulbs went. Areas (beds, borders) as first-class objects with their own history.
- **Avoid.** Small rating base and no update since July 2025; the texture can read as a template if overused.

### 3.11 Seed to Spoon

[App Store](https://apps.apple.com/ca/app/seed-to-spoon-garden-planner/id1312538762). Food-garden planner from an Oklahoma family farm: planting dates by location, guides, a journal, Growbot (photo ID of plants, pests and problems, and a chat), freeze and heat-wave alerts, and a seed shop.

- **Look.** Dark kale-leaf photo backgrounds, a slab serif, green headers, dense icon rows with a shopping cart in the top bar.
- **Result and uncertainty.** Growbot's result screen is not in the screenshots (*unverified*).
- **Learn from.** A pest guide split into good and bad bugs ("Ants", "Aphids", "Birds", "Cabbage loopers"). Planting dates calculated for your location on a colour-coded calendar. Freeze alerts.
- **Avoid.** The shop in the main navigation and "Search, Find, Buy, Plant, Succeed: Guaranteed".

### 3.12 Planter

[App Store](https://apps.apple.com/ca/app/planter-garden-planner/id1542642210). Square-foot garden planner with companion planting.

- **Look.** Soil-brown grid (about #775647) with flat vegetable icons in each square, green (about #51A443) for good neighbours. Plain white detail pages with small icon tiles.
- **Result and uncertainty.** No diagnosis.
- **Learn from.** The bed as a top-down grid you can read at a glance. Quick-info tiles with one value each: spacing "1/Square", depth "6 in", sun "Full Sun", water "1 in/week", frost "Semi-Tolerant", harvest "60–120 days". A growing-calendar bar with a "Today" marker and "Start Inside / Transplant / Sow Outside" bands.
- **Avoid.** Imperial units only in the screenshots, which is wrong for Canada.

## 4. Apps from other categories

### 4.1 CARROT Weather

[App Store](https://apps.apple.com/ca/app/carrot-weather-alerts-radar/id961390574). Weather app with selectable personalities; Apple Design Award and App of the Year.

- **Look.** Bright blue (about #027AFD), pixel-art scenes, a layout the user can rebuild ("Smart Layouts").
- **Learn from.** Alert tiers that look different: "CRITICAL" in red, "TIME SENSITIVE" in the normal style, and critical alerts that can break through Do Not Disturb. Hourly forecasts as icons on a curve. Widgets, Live Activities and a watch face that answer one question without opening the app.
- **Avoid.** The snark. Jokes work for rain, not for a diseased tomato crop.

### 4.2 Oura

[App Store](https://apps.apple.com/ca/app/oura/id1043837948). Smart-ring app with Sleep, Activity and Readiness scores.

- **Look.** Dark navy and black, soft gradients, a serif for the daily headline ("Go get 'em!", "Your cardiovascular age is below your actual age"), a thin sans for data.
- **Learn from.** Every number has a word: "Optimal", "Good", "Aligned", "No signs", "Thriving", "Needs care", "Looking good". The headline says what it means in a sentence; charts sit below with "Above / Aligned / Below" bands instead of raw thresholds. "Symptom Radar: No signs" is how to report the absence of a problem.
- **Avoid.** A 0 to 100 score for plant health; it invites the false precision PDM avoids.

### 4.3 Polycam

[App Store](https://apps.apple.com/ca/app/polycam-3d-scanner-lidar-360/id1532482376). LiDAR and photo scanning to 3D models, floor plans and CAD files.

- **Look.** Warm off-white (about #EEECEB), black buttons, italic display headings, the scan itself as the hero.
- **Learn from.** Measurements as small pills attached to edges of the 3D model ("18' 8.750""), with a running total and a ruler, pen and area tool. A "Layers" sheet that toggles floor plan, LiDAR, furniture and ceiling. Orbiting a model with one finger.
- **Avoid.** Pro-tool density; most gardeners need one measurement, not a toolbar.

## 5. Table stakes

Every serious app in the category does these. Missing one will look like an omission, not a choice.

1. **Camera first.** A large capture button in the centre of the tab bar (PictureThis, iNaturalist, PlantNet, Plantum).
2. **A result in seconds,** with the plant's photo at the top and the name large.
3. **Treatment as steps,** numbered or as a checklist, not a paragraph (PictureThis, Plantix, Agrio).
4. **A plant list with per-plant status** and photo thumbnails (Planta, Greg, Gardenize).
5. **Reminders and a care timeline** with done, due and overdue states (Planta, Blossom, PictureThis).
6. **Weather alerts** for frost and heat (Seed to Spoon, Blossom, Plantix).
7. **Reference photos** for the species, so the user can check the match (PlantNet, iNaturalist).
8. **A human fallback**: community answers or an expert (Greg, Plantix, Planta, iNaturalist).
9. **Light and dark appearance** and native-feeling sheets and tab bars.

## 6. Gaps PDM can own

### 6.1 Honest diagnosis

The identifiers have solved honesty (Seek's ID meter, iNaturalist's genus fallback, PlantNet's ranked list). The diagnosers have not: PictureThis, Plantum, Blossom and Plantix give one answer with no hedge, and Agrio says "highly confident" without showing when it is not. Planta's Dr. Planta is a chat, so it probably asks questions (*unverified*), but there the chat is the whole diagnosis. None of the reviewed apps asks one targeted question inside a visual result to settle a specific doubt.

PDM's follow-up question ("Do the spots have rings, like a target?") and its wording bands fill this. **In the demo:** let the question appear inside the live analysis, answer it with one tap, and show the diagnosis wording change from "may be" to "likely". Ten seconds that no competitor can show.

### 6.2 Showing the work

In the screenshots, every app goes from capture straight to a result or a tick. Plantix's leader line and Plantum's loupe hint at localisation, but only in marketing images. None of them draws boxes on the user's own photo as stages finish, or shows that soil and surroundings were checked too.

**In the demo:** start on the capture, then let the stages tick through (plant, leaves, soil, surroundings, knowledge, plan) while boxes land on the spots, each box labelled. End on the diagnosis with the boxes still on the photo.

### 6.3 Context and sources

Weather apps know the forecast; plant apps know the plant; none joins them. Seed to Spoon and Blossom send generic freeze alerts; Plantix and Agrio send regional disease alerts. None says "a humid stretch starts Thursday, and your tomatoes already show early blight: act before then". None of the reviewed apps cites an extension service (OMAFRA, Cornell) for its treatment.

**In the demo:** open the action plan, tap a source chip on one step to show the OMAFRA page title, then cut to a forecast alert that names the plants at risk.

### 6.4 Smaller gaps

- **Beneficials at scan time.** Seed to Spoon has a good-and-bad bugs guide; no app tells you at the moment of the scan that the ladybird larva on the leaf eats aphids. A calm, positive "Leave it" card would be memorable.
- **Measurement that tracks a problem.** Greg and Plantum measure pots. None of the reviewed apps measures a lesion or a plant in millimetres over time. Two scans a week apart with "spot grew from 6 mm to 11 mm" would be new.
- **A garden in 3D.** Planter and Gardenize do 2D beds. Polycam does 3D rooms. No garden app does a 3D map of the garden itself.
- **Shared gardens with roles.** Only Agrio groups fields for inspectors. Community gardens have nothing.
- **Canada.** Metric throughout, frost dates and hardiness for Canadian places, Canadian sources. The US apps default to inches and Fahrenheit in their screenshots.

## 7. Ideas to borrow

| # | Idea | From | Use in PDM |
|---|---|---|---|
| 1 | An ID meter whose dots fill as the model gets surer, with a tip when it stalls ("try looking at a flower") | Seek | Stage progress during analysis, and capture guidance when evidence is thin |
| 2 | Confidence as a lead-in phrase before the name ("We're highly confident that it is:") | Agrio | The diagnosis headline: "Likely early blight", "Could be one of two things", never a percentage |
| 3 | A coarser answer stated plainly ("pretty sure this is in the genus…") | iNaturalist | When species or disease is unclear, name what is known (fungal leaf spot) and what would settle it |
| 4 | A leader line from a spot on the photo to its label and symptom | Plantix | Labels on the detection boxes; tap a label to scroll to that finding |
| 5 | A loupe that magnifies the lesion | Plantum | Tap a box to zoom the crop in place |
| 6 | Reference photos beside each candidate with a Confirm button | PlantNet | Species confirmation and the two-candidate case |
| 7 | Organic and conventional treatments kept apart | Agrio | Action plan sections, organic first for home gardeners |
| 8 | Every score has a word ("Optimal", "Needs care", "No signs") and a headline sentence | Oura | Plant and garden status; "No signs of disease" as a result in its own right |
| 9 | Measurement pills attached to the model, plus a Layers sheet | Polycam | LiDAR mm labels and the 3D garden map's layers (beds, plants, alerts) |
| 10 | Alert tiers with distinct looks, and widgets and Live Activities | CARROT | Frost as critical, humidity as time-sensitive; a Live Activity while a scan runs |
| 11 | Depth rings during capture, measurements on the pot | Greg | LiDAR capture feedback |
| 12 | "Disease occurred nearby" with a "Rising" tag | Plantix | Shared-garden alerts when a member's scan finds a disease |
| 13 | One-value tiles (frost "Semi-Tolerant") and a calendar bar with a "Today" marker | Planter | Species card and forecast strip |
| 14 | Drawing on a photo to remember where things are | Gardenize | Marking beds before the LiDAR map exists, or on the web build |
| 15 | A calm two-colour palette with real, unstaged photos | Planta, Gardenize | Overall tone (see [design.md](design.md)) |
| 16 | A real before-and-after from the plant's own history | PictureThis's split image, used honestly | Comparing two scans of the same plant, labelled with dates |

## 8. What would make PDM look generic

- **The category green.** Saturated emerald or mint on white cards with an orange button is PictureThis, Plantum, Blossom and Plantix at once.
- **Stock hero plants.** Monstera, frangipani, a studio peace lily. PDM's subjects are tomatoes, squash and garden beds; show those, outdoors, in Canadian light.
- **A mascot.** Greg's leaf and Blossom's flower set a cute tone that fights an honest diagnosis.
- **A spinner and a tick.** Hiding the analysis is what everyone does; PDM's analysis is the show.
- **A confidence percentage or an accuracy claim** ("98%", "Highly Accurate"). The project already rules out a percentage on the diagnosis; keep it out of marketing too.
- **"Cure" and Before/After.** Promising outcomes the app cannot guarantee.
- **Diagnosis as a chatbot.** Planta and Seed to Spoon put diagnosis inside chat. PDM's chat should follow a structured result, not replace it.
- **Upsell cards in the result.** "Ask an expert", "Get consultation", a shop tab or product links next to a diagnosis.
- **Badges and streaks.** Seek's gamification suits children, not a small farm.
- **A red triangle on everything.** Agrio's alerts list shows how warnings stop working when every row has one.
- **Inches and Fahrenheit.** Several apps show only imperial units in their screenshots.

## 9. Sources

All App Store and Google Play pages, ratings and screenshots were read on 2026-10-17 (Canadian store).

**App listings**

- PictureThis: <https://apps.apple.com/ca/app/picturethis-plant-identifier/id1252497129>
- Planta: <https://apps.apple.com/ca/app/planta-plant-garden-care/id1410126781>
- Greg: <https://apps.apple.com/ca/app/greg-plant-identifier-care/id1512912236>
- Plantix (Google Play): <https://play.google.com/store/apps/details?id=com.peat.GartenBank&hl=en_CA>
- iNaturalist: <https://apps.apple.com/ca/app/inaturalist/id6475737561>
- iNaturalist Classic: <https://apps.apple.com/ca/app/inaturalist-classic/id421397028>
- Seek by iNaturalist: <https://apps.apple.com/ca/app/seek-by-inaturalist/id1353224144>
- PlantNet: <https://apps.apple.com/ca/app/plantnet/id600547573>
- Agrio: <https://apps.apple.com/ca/app/agrio-plant-diagnosis-app/id1239193220>
- Plantum: <https://apps.apple.com/ca/app/plantum-ai-plant-identifier/id1476047194>
- Blossom: <https://apps.apple.com/ca/app/blossom-ai-plant-identifier/id1487453649>
- Gardenize: <https://apps.apple.com/ca/app/gardenize-plant-care-gardening/id1118448120>
- Seed to Spoon: <https://apps.apple.com/ca/app/seed-to-spoon-garden-planner/id1312538762>
- Planter: <https://apps.apple.com/ca/app/planter-garden-planner/id1542642210>
- CARROT Weather: <https://apps.apple.com/ca/app/carrot-weather-alerts-radar/id961390574>
- Oura: <https://apps.apple.com/ca/app/oura/id1043837948>
- Polycam: <https://apps.apple.com/ca/app/polycam-3d-scanner-lidar-360/id1532482376>

**Vendor documents**

- Seek user guide (2020), the ID meter and what to do when it stalls: <https://static.inaturalist.org/wiki_page_attachments/SeekUserGuide2020.pdf>
- Planta, how to find Dr. Planta: <https://support.getplanta.com/how-can-i-find-dr-planta>
- Agrio website: <https://agrio.app/>
- Greg 2.0 and PlantVision launch: <https://www.hunted.space/product/greg/launches/greg-2-0-plantvision>
- FAO Agritech Observatory, Plantix: <https://agritechobservatory.review.fao.org/en/plantix-crop-doctor>

**Studies**

- Validation of a plant identification application using digital images of toxic plants, J Med Toxicol 2022: <https://pmc.ncbi.nlm.nih.gov/articles/PMC8938528>

**Secondary and community sources (signals, not verified)**

- iNaturalist forum, the "pretty sure" genus suggestion: <https://forum.inaturalist.org/t/computer-vision-top-species-suggestion-doesnt-match-pretty-sure-genus-suggestion/75629>
- Unstar, plant ID apps ranked from 1 to 3 star reviews (May 2026): <https://unstar.app/blog/plant-identification-apps-ranked-picturethis-plantin-plantnet-2026>
- Lazyweb, PictureThis paywall experiments (July 2026): <https://experiments.lazyweb.com/research/picturethis-paywall-experiments.md>
- Plantum review summary: <https://spark.mwm.ai/kr/apps/plantum-ai-plant-identifier/1476047194>
