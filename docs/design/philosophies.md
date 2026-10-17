# PDM Design Philosophies

> **Status:** Draft v0.1, 2026-10-17. v0.1: surveys ten visual design philosophies against PDM's screens, outdoor use and honesty rules, lists the tells of generic design, sets out what Apple's iOS 26 chrome owns and what PDM owns, and specifies three directions to mock on top of it (Herbarium, Seed Packet, Instrument) with contrast-checked palettes, glass-tint checks, type, motion and 3D garden styles.

This is research for choosing PDM's visual direction. It surveys design philosophies that could fit the app, says what each would look like here and what it would cost to build in Expo, separates what Apple's system chrome owns from what is ours, and ends with three directions to mock, each a layer on top of Apple's design language. It is a companion to [design.md](design.md), which holds the design decisions, and to [screens](../frontend/screens.md), which holds the layouts and wording. The stack and pins are in [frontend §3–4](../frontend/README.md#3-stack-and-version-pins).

## Contents

1. [Scope and method](#1-scope-and-method)
2. [What any style must carry](#2-what-any-style-must-carry)
3. [The philosophies](#3-the-philosophies)
4. [Side by side](#4-side-by-side)
5. [Taste](#5-taste)
6. [What Apple owns and what we own](#6-what-apple-owns-and-what-we-own)
7. [Three directions to mock](#7-three-directions-to-mock)
8. [Rules shared by all three](#8-rules-shared-by-all-three)
9. [Open questions](#9-open-questions)
10. [Sources](#10-sources)

---

## 1. Scope and method

- Each philosophy gets its principles with a primary source, real exemplars, what it would look like in PDM, its fit, and its build cost in React Native.
- Links were opened on 2026-10-17. A link marked *unverified* could not be opened from here (403, timeout, or a page with no readable text); the claim next to it comes from a search result or general knowledge.
- Typeface licences were read from the `METADATA.pb` files in [google/fonts](https://github.com/google/fonts). Every open typeface named here is OFL and ships as static weights in an `@expo-google-fonts/*` package (checked on the npm registry), so it loads with `expo-font` on iOS, Android and the web.
- Palettes were checked with a script (WCAG 2.2 relative luminance) and a colour-vision-deficiency simulation (Machado et al. 2009, protanopia and deuteranopia at full severity). Both scripts live outside the repository.

## 2. What any style must carry

| Constraint | What it means for the visuals |
|---|---|
| Outdoor, bright sun | Dark text on a light background reads better in bright light, for all ages ([Piepenbrock et al. 2013](https://documentacion.fundacionmapfre.org/documentacion/publico/pt/bib/143702.do?format=xml), catalogue record, *unverified*: the finding is from its abstract as quoted in search results). Light mode is the outdoor default; dark mode is for evenings. Thin weights and pale greys fail first |
| One hand, dirty hands | 44 pt targets, primary actions in the bottom half, no precision gestures for anything essential |
| Honest uncertainty | No confidence % on a diagnosis. "Not sure yet: early blight or Septoria leaf spot" must look as considered as a firm answer, not like an error ([screens §6](../frontend/screens.md#6-wording-for-uncertain-results)) |
| Status as words plus colour | Every health colour sits beside its word. Green and red of equal lightness look alike to about 1 in 12 men |
| Dynamic Type, light and dark, reduced motion | Layouts reflow at the largest sizes; every motion has a still fallback |
| Three targets | iOS 26 renders native tabs as Liquid Glass; Android renders Material tabs; the web build (the judges' link) renders whatever we draw. A style that depends on one platform's material breaks on the other two |
| 2-minute demo video | The style has to read at video resolution, on a laptop, in four screens and one 3D view |

## 3. The philosophies

### 3.1 Swiss / International Style and Dieter Rams

**Principles.** Objective, grid-based layout; asymmetric composition; sans-serif type; content ordered by hierarchy, not decoration. Rams: good design is useful, understandable, unobtrusive, honest, and "as little design as possible" ([Vitsœ, Rams' ten principles](https://www.vitsoe.com/us/about/good-design)). Müller-Brockmann's grid book is the working manual (*Grid Systems in Graphic Design*, Niggli, 1981; [listing](https://www.slanted.de/?p=792761), *unverified*).

**Exemplars.**
- [Things 3](https://culturedcode.com/things/): two Apple Design Awards; one accent colour, generous whitespace, and short animations that explain where an item went.
- [Teenage Engineering OP-1](https://teenage.engineering/products/op-1): lowercase type, all-caps spec lists, product as hero, nothing ornamental.
- [Vitsœ](https://www.vitsoe.com/): Rams' own shelving company; plain grid, photographs of use, long-lasting over fashionable.

**In PDM.** Off-white and near-black, one accent, status colours only where status is. Type: system font, or [Instrument Sans](https://github.com/google/fonts/tree/main/ofl/instrumentsans) (OFL). Radii 4–8 pt; hairline rules instead of cards. Photos full-width, cropped to the plant. Motion: 150–250 ms ease-out, nothing springy. 3D: white architectural massing model, orthographic, one soft light.

**Fit.** Strong for trust: the honesty principle matches the uncertainty wording. Excellent outdoors. Risk: it can look like every productivity app, and on a 2-minute video restraint reads as "unfinished" unless the details are perfect.

**Build cost.** Low. Tokens and `StyleSheet` already cover it. The cost is in precision: a real 4 pt grid, optical alignment, tabular figures.

### 3.2 Editorial and luxury

**Principles.** Type does the work: a serif display face, wide margins, a measure of 45–75 characters, photographs given room. Bringhurst: typography exists "to honor content" (*The Elements of Typographic Style*, v4.0, Hartley & Marks, 2012; [review](https://typographica.org/typography-books/the-elements-of-typographic-style-4th-edition/)). Luxury adds slowness, muted colour and very few elements per screen.

**Exemplars.**
- [Kinfolk](https://www.kinfolk.com/): large all-caps section heads, sentence-case story titles, photographs as full-bleed tiles.
- [Apartamento](https://www.apartamentomagazine.com/): interiors photographed as lived in, not staged.
- [Aesop](https://www.aesop.com/) (*unverified*, 403): long-form product copy, apothecary labels, warm neutrals.
- Primary: News in Depth, 2026 Apple Design Award winner for Social Impact ([awards](https://developer.apple.com/design/awards/)).

**In PDM.** Parchment, ink, one deep accent. Type: [Newsreader](https://github.com/google/fonts/tree/main/ofl/newsreader) (Production Type, OFL) or [Instrument Serif](https://github.com/google/fonts/tree/main/ofl/instrumentserif) (OFL, regular and italic only) for headlines; system font for body. Radii 0–4 pt. Photos large, captioned like plates. Motion: slow fades, 300–400 ms, ease-in-out. 3D: soft paper diorama, warm light, shallow depth of field.

**Fit.** The serif voice suits a diagnosis that is a considered opinion; italics can carry "not sure yet" with dignity. Weak outdoors when the serif is thin, and slow pacing wastes demo seconds. Risk: luxury signals "for someone else" to a community-garden member.

**Build cost.** Low to medium. Custom fonts load through `expo-font`; `fontWeight` doesn't pick a file on Android, so each weight is its own `fontFamily`.

### 3.3 Apple-native (HIG and Liquid Glass)

**Principles.** Defer to content; clear hierarchy; consistency with the system. Liquid Glass is "a distinct functional layer for controls and navigation" above content; "don't use Liquid Glass in the content layer"; "use Liquid Glass effects sparingly"; over bright media use the clear variant with a 35% dimming layer ([HIG, Materials](https://developer.apple.com/design/human-interface-guidelines/materials); [Apple Newsroom, June 2025](https://www.apple.com/newsroom/2025/06/apple-introduces-a-delightful-and-elegant-new-software-design/)).

**Exemplars.**
- [Tide Guide](https://www.tideguide.com/): 2026 Apple Design Award for Visuals and Graphics; full-screen charts you scrub, a palette that follows the sky, Liquid Glass on controls only.
- [Flighty](https://www.flighty.com/): 2023 Apple Design Award; dense, "pilot-grade" data made readable, and delays explained with their cause.
- [Halide](https://www.halide.cam/): dial-like gestures and type modelled on lens engravings, while staying inside iOS conventions.

**In PDM.** System colours tuned to the brand, SF Pro and New York (Apple-licensed for Apple platforms only, so the web and Android get Roboto or system-ui), SF Symbols, continuous corners at 10–26 pt, grouped lists. Glass on the tab bar, the capture controls and the 3D view's toolbar. Motion: system springs, symbol effects (`bounce`, `pulse`). 3D: clean RealityKit-like render, soft shadows.

**Fit.** Familiar, accessible by default (system components honour Reduce Transparency and Increase Contrast), and the floating glass tab bar comes free with native tabs. Because Expo renders these native components, this is not optional: it is the base every direction sits on (§6). As a whole direction on its own it falls short: on Android and the web, where the judges click, there is no glass, and "looks like an Apple sample app" is not a UI-and-design win.

**Build cost.** Low on iOS. `expo-glass-effect`'s `GlassView` is iOS 26 only and falls back to a plain `View` elsewhere; check `isGlassEffectAPIAvailable()` and `AccessibilityInfo.isReduceTransparencyEnabled()` ([docs](https://docs.expo.dev/versions/v57.0.0/sdk/glass-effect/)). A glass imitation for the web is custom work and usually looks cheap.

### 3.4 Scientific and data-dense (Tufte, field guides, lab reports)

**Principles.** Show the data; maximise the share of ink that is data; remove chartjunk; compare with small multiples; integrate words, numbers and images (Tufte, *The Visual Display of Quantitative Information*, 1983, and *Envisioning Information*, 1990; [books](https://www.edwardtufte.com/books/)). Field guides add the plate: a specimen, the marks that tell it apart, and "similar species" beside it.

**Exemplars.**
- [Our World in Data](https://ourworldindata.org/): every chart carries its source, year and licence, and links to its data.
- [Johnny's Selected Seeds](https://www.johnnyseeds.com/vegetables/tomatoes/): a catalogue that filters by days to maturity and by named disease resistance (early blight, late blight, Fusarium wilt).
- [iNaturalist](https://forum.inaturalist.org/t/better-use-of-location-in-computer-vision-suggestions/915): suggestions are "visually similar" and "seen nearby", and when unsure it says "We're pretty sure this is in the genus…" (main site *unverified*, 403; behaviour from the forum).
- [Windy](https://www.windy.com/): dense weather layers on a map, readable because colour scales are consistent.

**In PDM.** Paper white, black ink, colour only for status and data. Type: [IBM Plex Sans and Mono](https://github.com/google/fonts/tree/main/ofl/ibmplexsans) or [Atkinson Hyperlegible Next and Mono](https://github.com/google/fonts/tree/main/ofl/atkinsonhyperlegiblenext) (all OFL). Radii 2–6 pt, 1 pt rules, tabular numbers, units always shown ("lesion 7 mm"). Imagery: photos with scale bars and labelled regions. Motion: values count, bars fill, 120–240 ms, critically damped. 3D: survey model, height-true cylinders, a 0.5 m ground grid, dimension labels.

**Fit.** The best fit for honest uncertainty: candidates shown side by side as plates, with the distinguishing mark ("rings like a target?") pointed at. Excellent outdoors. Risk: it can feel cold for a home gardener, and dense screens cramp at the largest Dynamic Type sizes.

**Build cost.** Medium. Small SVG charts and overlays are already planned with `react-native-svg`; the cost is layout at large type sizes.

### 3.5 Organic and natural (wabi-sabi, Japanese, botanical illustration, seed catalogues)

**Principles.** Wabi-sabi finds beauty in "things imperfect, impermanent, and incomplete" (Leonard Koren, *Wabi-Sabi for Artists, Designers, Poets & Philosophers*, Imperfect Publishing, 2008; [listing](https://mitpressbookstore.mit.edu/book/9780981484600), *unverified*). Kenya Hara frames emptiness as a capacity to receive meaning, not a lack ([*Designing Design*, Lars Müller](https://www.lars-mueller-publishers.com/designing-design-0)). Botanical plates and seed packets add drawn specimens and a printed, slightly warm colour.

**Exemplars.**
- [Hudson Valley Seed Company Art Packs](https://hudsonvalleyseed.com/pages/art-packs): each packet commissions an artist; packets open into a frameable shape.
- [Muji](https://www.muji.com/) (*unverified*, timed out): unbranded goods, natural materials, quiet layout.
- [Biodiversity Heritage Library](https://www.biodiversitylibrary.org/) (*unverified*, 403): public-domain botanical illustration, a source of reference style rather than assets to paste in.

**In PDM.** Cream, soil brown, leaf and fruit colours at printed saturation. Type: [Fraunces](https://github.com/undercasetype/Fraunces) (Undercase Type, OFL), a soft "Old Style" display serif inspired by Windsor and Cooper. Radii 12–20 pt, paper texture used once at most. Spot illustrations of beneficial insects and tasks. Motion: grow and settle, gentle springs. 3D: clay diorama or painted paper.

**Fit.** Warm and on-topic, and the "leave it, it eats aphids" card becomes a small plate. Outdoors it works if contrast holds. Risk: craft imagery reads as a lifestyle brand rather than a diagnosis tool; illustrations take time; Fraunces' soft and wonky axes are lost in the static files (see §7.2).

**Build cost.** Medium to high. Custom illustration is the cost, not the code.

### 3.6 Warm, playful and tactile

**Principles.** Attractive things work better: design reaches people at visceral, behavioural and reflective levels (Don Norman, [*Emotional Design*](https://jnd.org/emotional-design-why-we-love-or-hate-everyday-things/), 2004). Tactile design adds physical feedback: press states, springs, haptics, sound.

**Exemplars.**
- [Not Boring apps](https://notbor.ing/): designed "like a game using 3D, animation, physics, haptics, and sounds"; (Not Boring) Camera was a 2026 Apple Design Award finalist for Visuals and Graphics ([awards](https://developer.apple.com/design/awards/)).
- [Gentler Streak](https://gentlerstories.com/gentlerstreak/): 2024 Apple Design Award for Social Impact; rest counts, no guilt, a companion "never judging".
- [Duolingo's core tabs refresh](https://blog.duolingo.com/core-tabs-redesign/): "Good design means knowing what to remove and what to keep"; whitespace replaced forced containers.
- [Bruno Simon's portfolio](https://bruno-simon.com/): a three.js world you drive through; the reference for playful web 3D.

**In PDM.** Saturated but warm palette, chunky pressed buttons, rounded type such as [Nunito](https://github.com/google/fonts/tree/main/ofl/nunito) or [Bricolage Grotesque](https://github.com/google/fonts/tree/main/ofl/bricolagegrotesque) (both OFL). Radii 16–24 pt, solid offset shadows. Motion: springs with 10–25% overshoot, haptics on every confirm. 3D: toy diorama, low-poly or clay, bright sun.

**Fit.** Great for the demo's energy and for task completion. Risk: playfulness trivialises a sick plant. A bouncing mascot next to "Septoria leaf spot" undermines trust. Gentler Streak's lesson is the useful one: warmth in tone, not in jokes.

**Build cost.** Medium. Reanimated springs and `expo-haptics` are already in the stack; custom pressed states and illustrations are the work.

### 3.7 Neo-brutalism

**Principles.** "High contrast, blocky layouts, bold colors, thick borders", stark offset shadows, quirky type; best practice still demands readable type, 2–3 colours and contrast checks ([NN/g, Neobrutalism](https://www.nngroup.com/articles/neobrutalism/)).

**Exemplars.**
- [Gumroad](https://gumroad.com/): thick borders, pink and yellow sections, raw layout (described by NN/g).
- Figma's brand refresh and Tony's Chocolonely's shop, both cited by [NN/g](https://www.nngroup.com/articles/neobrutalism/).

**In PDM.** White, black 2–3 pt borders, one loud colour. Type: [Space Grotesk](https://github.com/google/fonts/tree/main/ofl/spacegrotesk) with [Space Mono](https://github.com/google/fonts/tree/main/ofl/spacemono) (OFL). Radii 0 or 8 pt, 4 pt black offset shadows. Motion: snaps, 80–120 ms, no easing curves to speak of. 3D: flat-shaded blocks with black outlines.

**Fit.** Very legible outdoors; memorable on video. Risk: the voice is loud and ironic, which fights honest uncertainty ("NOT SURE YET" in a black box reads as an error). It is also a 2021–2023 web trend, so judges may read it as fashion.

**Build cost.** Low. Borders and solid shadows are trivial in `StyleSheet`; Android elevation isn't needed.

### 3.8 Calm technology

**Principles.** Technology should demand as little attention as possible, inform without overwhelming, work at the edge of attention and move to the centre only when needed, and fail gracefully ([Amber Case, calmtech.com](https://calmtech.com/)). It extends Weiser and Brown: calm technology "engages both the center and the periphery of our attention" ([*Designing Calm Technology*, 1995](https://calmtech.com/papers/designing-calm-technology.html)).

**Exemplars.**
- [Are.na](https://www.are.na/about): "a mindful space"; no ads, members are the only customers.
- [Gentler Streak](https://gentlerstories.com/gentlerstreak/): goals that adjust to how you feel.
- [The Light Phone](https://www.thelightphone.com/) (*unverified*, no readable text): a phone reduced to a few tools.

**In PDM.** Low-saturation neutrals; colour appears only when something needs attention (a frost alert, a sick plant). System or humanist type such as [Literata](https://github.com/google/fonts/tree/main/ofl/literata) (OFL) for reading. Notifications batched; the home screen says "Nothing needs you today" when true. Motion: slow, small, rare. 3D: quiet, overcast lighting.

**Fit.** Matches the product's ethics: frost alerts and tasks at the edge until they matter. Strong on trust. Risk: calm is invisible in a demo; it needs a second philosophy to carry the visuals.

**Build cost.** Low for visuals; the work is in notification and alert rules, which the backend already shapes.

### 3.9 Solarpunk and low-tech

**Principles.** "A future with a human face and dirt behind its ears": reuse over replacement, local resilience, community, and a rejection of "overly smooth, clean, white modernist iPod futures" (Adam Flynn, [*Solarpunk: Notes toward a manifesto*](https://hieroglyph.asu.edu/2014/09/solarpunk-notes-toward-a-manifesto/), 2014). Low-tech design adds frugality you can see.

**Exemplars.**
- [Low←Tech Magazine's solar website](https://solar.lowtechmagazine.com/about/the-solar-website/): images dithered to four greys (about ten times lighter), no custom font, a background that shows the battery level, pages under 0.5 MB.
- [Townscaper](https://www.townscapergame.com/): a toy town grown from coloured blocks on an irregular grid; a model for a friendly, generative 3D garden.

**In PDM.** Sun-yellow, sky-teal and leaf colours, hand-drawn or dithered imagery. Type: system font or [Recursive](https://github.com/google/fonts/tree/main/ofl/recursive) (OFL; its casual axis is lost in static files). Motion: sun-path and weather animations. 3D: low-poly diorama with a sun you can scrub through the day, shadows from the real sun position.

**Fit.** On-message for community gardens and small farms, and a sun-position shadow in the 3D garden is a memorable demo beat. Risk: the aesthetic is still unsettled (Flynn says so), so it drifts into either green-tech cliché or zine clutter.

**Build cost.** Medium. Dithering photos is cheap; a sun-position model (SunCalc-style maths) is a day's work.

### 3.10 Material 3 Expressive

**Principles.** Colour, shape, size, motion and containment used to draw attention to key actions. Google reports 46 studies with more than 18,000 participants; key elements were found up to four times faster, and people over 45 found them about as fast as younger users; a strong minority preferred calmer designs ([Google Design, Expressive Material research](https://design.google/library/expressive-material-design-google-research)). Motion moves to a spring-based physics system ([M3 motion](https://m3.material.io/styles/motion/overview/how-it-works), *unverified*, no readable text).

**Exemplars.**
- Google's own apps (Gmail, Clock, Meet, Drive) since 2025, tracked in [9to5Google's roundup](https://9to5google.com/2025/09/27/google-material-3-expressive-redesign/) (*unverified*, from search results).
- [Material 3 site](https://m3.material.io/) (*unverified*): the shape library and motion tokens.

**In PDM.** Tonal palettes from one seed colour, a large shape vocabulary (pills, scalloped and rounded squares), [Google Sans Flex](https://github.com/google/fonts/tree/main/ofl/googlesansflex) (OFL since its Google Fonts release; the static files lose its roundness axis). Motion: springs everywhere. 3D: not part of the system.

**Fit.** The research is the strongest of any here on findability across ages. Android users get a native feel. Risk: on iOS 26 it clashes with Liquid Glass, and the shape vocabulary is Google's brand, not ours.

**Build cost.** Medium. Tonal palettes and shapes are custom in React Native; there is no M3 Expressive component library for it in our stack.

## 4. Side by side

Ratings are judgements for PDM, 1 (poor) to 3 (strong).

| Philosophy | Trust and honest uncertainty | Outdoor legibility | 2-minute demo | Accessibility | Build cost |
|---|---|---|---|---|---|
| Swiss / Rams | 3 | 3 | 2 | 3 | Low |
| Editorial / luxury | 2 | 2 | 2 | 2 | Low–medium |
| Apple-native / Liquid Glass (the base for all, §6) | 2 | 2 | 2 (iOS), 1 (web) | 3 | Low |
| Scientific / data-dense | 3 | 3 | 2 | 2 | Medium |
| Organic / botanical | 2 | 2 | 3 | 2 | Medium–high |
| Warm / playful / tactile | 1 | 2 | 3 | 2 | Medium |
| Neo-brutalism | 1 | 3 | 3 | 3 | Low |
| Calm technology | 3 | 2 | 1 | 3 | Low |
| Solarpunk / low-tech | 2 | 2 | 3 | 2 | Medium |
| Material 3 Expressive | 2 | 3 | 2 | 3 | Medium |

No single column wins. The pattern is clear, though: the philosophies strong on trust are weak on demo energy, and the reverse. Each direction in §7 pairs one from each side.

## 5. Taste

### What separates a tasteful execution

| Philosophy | Tasteful | Generic |
|---|---|---|
| Swiss / Rams | A real grid you can draw on the screenshot; hierarchy from size and weight alone; one accent used for one meaning | Whitespace without a grid; grey-on-grey text; "minimal" meaning "unfinished" |
| Editorial | A serif chosen for its optical size, set large with tight leading; captions that say something | A serif headline over a stock photo, body still Inter |
| Apple-native | Glass only on the controls layer; content opaque and readable; SF Symbols at the text's weight | Glass on every card; blur behind text; symbols at random weights |
| Scientific | Units and sources on every number; small multiples with shared axes; nothing decorative | Dashboards of donut charts; numbers without units; fake precision ("87.3%") |
| Organic | One illustrator's hand; colours from the plants themselves; texture used once | Watercolour leaves in every corner; beige everything; a leaf logo |
| Playful / tactile | Springs on direct manipulation only; jokes absent where the news is bad | Confetti on every action; a mascot reacting to a sick plant |
| Neo-brutalism | Thick borders that also organise; contrast that passes; one loud colour | Random rotations; four clashing colours; unreadable display type |
| Calm | Silence when nothing is wrong; one alert colour | Empty states that still nag; badges everywhere |
| Solarpunk | Real sun and weather data driving the visuals | Stock "eco" gradients of green and teal; wind-turbine clip art |
| M3 Expressive | Shape changes that mean something (a pressed button morphs) | Every shape at once |

### Tells of generic or AI-generated app design

Avoid all of these. Each is something a judge has seen a hundred times this year.

1. **Indigo-to-purple gradients** (Tailwind's `indigo-500` `#6366F1` into `purple-500` `#A855F7`), especially as button fills or hero backgrounds.
2. **Emoji as icons** ("🌱 My Garden", "💧 Water"), or a greeting like "Good morning, Alex 👋".
3. **Glassmorphism on content**: frosted cards over a blurred gradient blob. HIG says no glass in the content layer.
4. **Everything centred**: centred titles, centred body text, centred empty states. Left-aligned text with a clear edge reads faster.
5. **Uniform card grids**: every item a card with 16 pt radius, 16 pt padding and the same soft shadow (`0 4px 6px rgba(0,0,0,0.1)`), regardless of importance.
6. **Inter on everything**, at 400 and 600, with no display face and no tabular figures.
7. **A sparkles icon or "AI-powered"** to mark machine output. Say what the model did: "Found 3 spots on 2 leaves".
8. **Circular score gauges**: "Plant health 87%". PDM never shows a diagnosis confidence; a gauge would also be false precision.
9. **Gradient text headlines** and neon accent glows in dark mode on pure `#000000`.
10. **Stock illustration sets** (flat people with purple skin tones, floating blobs) and stock photos of perfect plants.
11. **Shimmer skeletons** for data that arrives in 200 ms, and spinners where a stage list could show real progress.
12. **Bouncing everything**: every element springing in on every screen, with no reduced-motion path.
13. **Pill chips for everything**: status, filters, tags and actions all the same pill.
14. **Generic copy**: "Unlock insights", "Your plant journey", "Let's get started!".
15. **A green leaf logo in a rounded square**, the default plant-app identity.
16. **Floating action button plus tab bar plus header actions** competing for the one primary action.

## 6. What Apple owns and what we own

Expo renders native iOS 26 components, so the app's chrome is Apple's whatever direction we pick. Native tabs are the floating Liquid Glass tab bar; the navigation bar, its glass buttons, sheets, menus, alerts and SF Symbols are system components. The directions in §7 are therefore not alternatives to the HIG (§3.3) but a layer on top of it: they differ only in what is ours. Sources: [HIG, Materials](https://developer.apple.com/design/human-interface-guidelines/materials), [HIG, Color › Liquid Glass color](https://developer.apple.com/design/human-interface-guidelines/color#Liquid-Glass-color), [HIG, Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars), [Expo Router native tabs](https://docs.expo.dev/router/advanced/native-tabs/), [Expo Router stack toolbar](https://docs.expo.dev/router/advanced/stack-toolbar/).

| Element | Owner | iOS 26 | Android | Web build |
|---|---|---|---|---|
| Tab bar | Apple | `NativeTabs` (`expo-router/unstable-native-tabs` on SDK 57), Liquid Glass, SF Symbols through `sf` | Material tab bar, Material Symbols through `md`; no glass | Native tabs fall back to "a basic implementation, loosely based on iPad design"; we replace it with headless tabs from `expo-router/ui` in `_layout.web.tsx` |
| Navigation bar, titles, toolbar buttons | Apple | Native stack, SF Pro titles, glass buttons through `Stack.Toolbar` (SDK 55+) | Material top bar, `Stack.Toolbar` (SDK 56+) | `Stack.Toolbar` doesn't render; we draw the header |
| Sheets, menus, alerts, pickers, switches | Apple | System components (`presentation: "formSheet"`) | Material equivalents | React Native Web equivalents, styled by us |
| Gestures | Apple | Edge-swipe back, sheet drag, pull to refresh | Predictive back | Browser back |
| Content colour and accent | Ours | `background`, `surface`, cards, list rows; `accent` also tints the selected tab and one prominent button | Same tokens | Same tokens |
| Display type in content | Ours | Content headings, plant names, the diagnosis headline, captions. Bar titles stay SF Pro | Same files; bar titles Roboto | Same files; bars in `system-ui` |
| Photos, overlays, charts, status chips, illustration | Ours | Region boxes, scale bars, the comparison of candidates, the growth curve | Same | Same |
| Motion inside content and haptics | Ours | Stage ticks, box draw, task check (§7). System transitions stay system | Same; no haptics on the web | Same, with reduced-motion media query |
| 3D garden | Ours | R3F scene in content; its toolbar is a glass `Stack.Toolbar` | Same scene, Material toolbar | Same scene, our toolbar |

**Typography consequence.** Navigation bar titles, large titles, tab labels and system buttons stay in SF Pro, at the system's Dynamic Type sizes. Each direction's display face lives only in content: list screens (garden home, tasks) keep the system large title, and the display face appears in content heroes (the plant name on its page, the diagnosis headline, the plan title, plate captions).

### Accent as a glass tint

The HIG rules ([Liquid Glass color](https://developer.apple.com/design/human-interface-guidelines/color#Liquid-Glass-color)):

- Liquid Glass "has no inherent color" and takes colour from the content behind it. Symbols and labels on small glass elements such as tab bars are monochrome by default, darker over light content and lighter over dark content.
- "Apply color sparingly." To emphasise a primary action, colour the background, not the symbol or text, as the system does for the Done button. Don't colour the background of several controls.
- Over colourful content, prefer a monochrome tab bar or an accent clearly different from the content. A brand colour as accent works best in apps whose content is mostly monochrome.
- Over photos and video, use the clear variant with a 35% dark dimming layer when the content is bright ([Materials](https://developer.apple.com/design/human-interface-guidelines/materials#Liquid-Glass)).

What PDM does with them:

- **Selected tab:** `tintColor` on `NativeTabs` is the direction's `accent`, set with `DynamicColorIOS({ light, dark })`. Unselected items stay system monochrome.
- **One prominent glass button per screen:** Save on the mapping session, Submit on capture, Done on a sheet. Its background is `accent`, its label `accentText`. Every other glass button is untinted.
- **Over photos, the camera and the 3D view:** no tint. The capture screen is a full-screen modal without the tab bar; its glass buttons are clear glass, white symbols, 35% dimming. `DynamicColorIOS` follows the app's light or dark appearance, not the glass's adaptive appearance, so a light-mode accent over glass that has turned dark over a photo would be unreadable (our reading of the two docs; check on the device).

Glass has no fixed colour, so the check uses stand-ins: iOS system grays for light glass (`#F2F2F7`, `#E5E5EA`) and dark glass (`#1C1C1E`, `#2C2C2E`), and a tinted button modelled as the accent at 80% opacity over white, black and mid-grey photo backdrops. The numbers are approximate.

| Direction | Tab tint on light glass | Tab tint on dark glass (dark accent) | Label on tinted prominent button (worst backdrop) | Light accent over glass gone dark |
|---|---|---|---|---|
| Herbarium (green) | 6.18:1 | 7.09:1 | 4.65:1 | 1.80:1, so no tint over photos |
| Seed Packet (plum) | 6.98:1 | 7.76:1 | 5.33:1 | 1.59:1, so no tint over photos |
| Instrument (ink) | 14.36:1 | 12.34:1 | 9.62:1 | 1.06:1. Its accent is black and white, which is what the system's monochrome glass already does, so Instrument sets no tint at all |

All three pass as tints in their own appearance. All three fail over glass that has adapted to dark content, which is why tint is off over photos and the camera.

### Distinct with identical chrome

With the same glass tab bar, nav bar and sheets, a screenshot of each direction still differs in everything inside them:

| Screen | Herbarium | Seed Packet | Instrument |
|---|---|---|---|
| Garden home | Warm paper `#F5F4EE`, hairline rows, plant names in the system font, green selected tab | Cream `#FAF3E4`, rounded cards with colour blocks, a frost spot illustration, plum selected tab | Pure white, black type, the forecast as a line with an amber frost threshold, monochrome tab bar |
| Live analysis | Ink checks; the box draws like a pencil outline | Stage dots sprout; the box lassoes in plum | Run log with times in mono; reticle corners with the lesion's size in mm |
| Diagnosis | Newsreader Italic "Not sure yet", two equal plates | Fraunces Italic, two equal packets, no illustration | Two-column comparison table of distinguishing marks |
| 3D garden | White architect's model with ink outlines | Clay diorama on a cream table, long warm shadows | Survey model on a grid, height labels, no shadows |

### Android and the web

- **Android** gets Material chrome from the same Expo components (the Expo page doesn't name the Material version; Material 3 is expected, *unverified*): the Material tab bar, top app bar and dialogs, Roboto in the chrome, Material Symbols. No glass and no blur. `accent` becomes the selected tab's colour. The content layer is identical to iOS: same tokens, same font files, same overlays, same 3D scene.
- **The web build** has no native chrome. We draw the header and tab bar ourselves as opaque `surface` with a 1 px `border` edge, in `system-ui` (SF Pro on Apple devices, Segoe UI on Windows, Roboto on Android). No imitation of glass; a CSS `backdrop-filter` copy looks cheap next to the real thing and costs contrast. The content layer is again identical. Since the judges open the web build, each mock must be judged on it too, not only on the iPhone.

## 7. Three directions to mock

The three are deliberately far apart: one restrained, one expressive, one data-forward. Only Herbarium uses a green accent; Seed Packet uses plum and Instrument is near-monochrome with colour reserved for status. All three keep the role names of the current tokens, add `danger`, `dangerSoft`, `infoSoft` and the status soft roles, and pass every text pair at 4.5:1 (§7.4). All three share Apple's chrome (§6); what differs is the content layer: background and accent, display type in content, cards and photos, data styling, imagery, motion inside content, and the 3D garden. In the motion tables, a screen entrance is our content arriving inside a screen once its data is there; the push and sheet transitions themselves are the system's.

The current "Field notebook" palette is a good base: it passes contrast (lowest text pair 5.05:1). Research supports refining it rather than keeping it as is: its sage-green-on-off-white is the plant-app category default, the accent and `healthy` are nearly the same colour (ratio 1.06:1), and in light mode `watch` `#8A5300` and `sick` `#B3261E` are almost identical to protanopes and deuteranopes (simulated ΔE 4). Herbarium is Field notebook refined.

### 7.1 Herbarium

**Restrained, precise, considered.** A pressed specimen sheet: the plant is the colour, the interface is paper and ink. Field notebook refined, through Swiss/Rams, the field guide and calm technology.

**Accent: herbarium green `#2C5C3E`.** The one green direction. A deep, slightly blue green, darker than any status colour, so a primary button never reads as "healthy". It is the colour of pressed leaves and of the cloth on old flora volumes, and it says "garden" without the bright-lime cliché.

- **Type.** Display, in content only: [Newsreader](https://github.com/google/fonts/tree/main/ofl/newsreader) 600 for content heroes at Title 1 (28) and Title 2 (22) (plant name, diagnosis headline, plan title), Newsreader 400 Italic for the "Not sure yet" line and plate captions. Body: the system font (SF Pro on iOS, Roboto on Android, `system-ui` on the web), matching the chrome. Numbers use `fontVariant: ['tabular-nums']`. Scale unchanged.
- **Shape.** Radii 6 pt on controls, 10 pt on sheets, 0 on photos. Borders, not shadows: 1 px `border` hairlines between list rows; the only shadow is the system's on sheets. Text fields and toggles draw their edge in `textMuted`, which passes 3:1 against `surface`; `border` (about 1.5:1) is decorative only.
- **Icons.** In content, SF Symbols at Regular, matching body weight; Material Symbols Outlined 400 on Android and the web through `expo-symbols`. Chrome icons are system defaults. Selected tab tinted `accent`.
- **Imagery.** Photos edge to edge, each with a caption line in caption type: "Tomato 1 · Oct 17 · 32 cm away". Region boxes in `text` ink, 1.5 pt, with a small label tab. Beneficial insects get a plate: photo, name, "leave it, it eats aphids", and nothing else.
- **Uncertainty.** "Not sure yet" sets two candidate plates side by side at equal size, with the question card below. Equal size is the visual statement that neither is favoured.

| Motion | What moves | Values |
|---|---|---|
| Page in | Screen content: opacity 0→1, rise 6 pt | `withTiming` 240 ms, `Easing.bezier(0.2, 0, 0, 1)`. Reduced: opacity only, 120 ms |
| Ink tick | Stage row: check path draws, label `textMuted`→`text` | Path 200 ms `bezier(0.3, 0, 0.2, 1)`; colour 160 ms; `Haptics.selectionAsync()`. Reduced: check appears, no draw |
| Specimen outline | Region box: four corner ticks at 0 ms, then the stroke draws by `strokeDashoffset`, then the label fades | Stroke 480 ms `bezier(0.4, 0, 0.2, 1)`; label 160 ms from 400 ms. Reduced: box and label fade in together, 150 ms |
| Struck through | Task check: box fills, a 1.5 pt line draws across the title | Fill 160 ms; line `scaleX` 0→1, 220 ms ease-out; row stays in place until the list refreshes |

Our own expanding panels (the question card, a plate opening) use one spring: stiffness 380, damping 40, mass 1 (damping ratio 1.03, no overshoot, settles in about 315 ms). Reanimated 4's default mass is 4, so mass is always set.

**3D garden: the architect's model.** Beds are extruded from `polygon_m` to 15 cm in warm paper white (`#ECE7DA`, `MeshStandardMaterial`, roughness 0.95, metalness 0). Plants are simple turned forms: a stem cylinder to the measured height and a sphere or cone for the canopy radius, tinted by status at 70% saturation. Every mesh gets an ink outline (`EdgesGeometry`, 1 px, `text` colour). A plant with no height measurement is drawn as an outline-only ghost at the species' typical height, labelled "not measured". Lighting: a hemisphere light (sky `#FFFFFF`, ground `#D8D2C2`, 0.6) plus one directional light from the south-west at 40° with a 1024 px soft shadow. Camera: orthographic, 35° elevation, 45° azimuth, north arrow and a 1 m scale bar, as in the 2D map ([screens §3.8](../frontend/screens.md#38-garden-map)). Camera moves use a critically damped spring (stiffness 120, damping 22, about 540 ms).

**How the demo feels.** Quiet and exact. The garden home opens on a paper-white page with one frost banner in amber words. The capture screen is almost all camera, with glass controls and a distance hint in plain type. In the analysis, each stage ticks in ink, the box draws itself around the lesion like a pencil outline, and the question card asks about rings in a serif you trust. The diagnosis says "Not sure yet" in italics with two plates, the plan reads like a well-edited field guide with its sources underneath, and the closing shot is the garden as a white architect's model with one rust-red plant. Judges should think "this team has taste and is not overselling".

### 7.2 Seed Packet

**Warm, crafted, generous.** A heirloom seed catalogue brought to life: printed colour, a soft serif with character, things that grow when they appear. Organic/botanical and warm/tactile, held back by calm technology where the news is bad.

**Accent: beet plum `#7D2B63`.** Beetroot and radish skin. It is a garden colour that is none of the status hues: far from healthy green, amber watch and crimson sick (ΔE 41 from `sick` in light mode), so a button never looks like a diagnosis. Seed-catalogue covers have long used deep ink colours against cream, and plum against cream has that printed warmth without the purple-gradient tell, because it is used flat, never in a gradient.

- **Type.** Display, in content only: [Fraunces](https://github.com/undercasetype/Fraunces) 600 SemiBold for content heroes and plant names on cards, 400 Italic for asides. The `@expo-google-fonts/fraunces` files are static instances, so the SOFT and WONK axes are fixed at whatever Google cut (*unverified* which values). For the soft look, cut our own instances with `fonttools varLib.instancer` (for example `opsz=48 SOFT=100 WONK=0` for titles, `opsz=12 SOFT=50` for small display) and bundle them; that is an hour's work. Body: the system font. Scale: content heroes at 30 pt in Fraunces (it sets smaller than SF at the same size); bar titles stay system.
- **Shape.** Radii 16 pt on cards, 12 pt on buttons, full pills only for the status chip. Depth from colour blocks, not blur: cards are `surface` on `background`; primary buttons have a 2 pt solid bottom edge in a darker accent that compresses on press. No soft drop shadows.
- **Icons.** In content, SF Symbols at Semibold with hierarchical rendering; Material Symbols Rounded 500 elsewhere. Selected tab tinted `accent`. Plus 6–8 custom two-colour spot illustrations by one hand: seedling, frost, watering can, ladybug, lacewing, pruners, compost, sun. Illustrations never appear on the diagnosis card.
- **Imagery.** Photos in 16 pt rounded frames with a 4 pt `surface` mat, like a packet window. Region boxes in `accent` with a rounded 8 pt corner and a label tab. Beneficials get the packet treatment: illustration, name, "leave it, it eats aphids".
- **Uncertainty.** The diagnosis card drops the playfulness: no illustration, no bounce, Fraunces Italic for "Not sure yet", candidates as two equal packets. Warmth is in the wording of the plan ("This week: two small jobs"), not in jokes.

| Motion | What moves | Values |
|---|---|---|
| Packet open | Screen entrance: cards rise 16 pt and scale 0.98→1, staggered 40 ms | Spring stiffness 260, damping 20, mass 1 (ratio 0.62, 8% overshoot, ~370 ms). Reduced: opacity 150 ms, no stagger |
| Sprout | Stage tick: the stage dot grows 0.6→1 into a check | Spring stiffness 500, damping 18, mass 1 (ratio 0.40, 25% overshoot, peaks at 1.10, ~375 ms); `Haptics.impactAsync(Light)`. Reduced: instant swap |
| Lasso | Region box: stroke draws, then one 1.04 pulse | Draw 560 ms `bezier(0.65, 0, 0.35, 1)`; pulse spring stiffness 300, damping 14 (ratio 0.40). Reduced: fade 150 ms, no pulse |
| Stamp | Task check: press-in to 0.92, release springs back; fill and strike follow | Press spring stiffness 600, damping 30 (ratio 0.61, ~240 ms); release stiffness 420, damping 16 (ratio 0.39, ~410 ms); fill 120 ms; `notificationAsync(Success)`. Reduced: fill only |

**3D garden: the clay diorama.** Beds are rounded slabs (bevelled `ExtrudeGeometry`, 4 cm bevel) with a soil-brown top (`#6B4A32`) and a wooden side colour (`#B98A5E`). Plants are capsule stems and lumpy canopy spheres (an icosphere with light noise), coloured by status in the palette's printed tones, matte (`MeshStandardMaterial`, roughness 0.8). The ground is a cream disc that fades at the edge, so the garden floats like a model on a table. Lighting: warm key light (`#FFE6C4`, 1.2) low from the south at 25° for long soft shadows, a cool fill from the north (`#C9DDF2`, 0.3). Camera: perspective, 30° field of view, 40° elevation, slow orbit only when the user drags. On load, plants grow in from scale 0 with a stagger of 40 ms (spring stiffness 180, damping 16, ratio 0.60, ~440 ms). Reduced motion: no grow-in, no inertia.

**How the demo feels.** Like opening a good seed catalogue in spring. The garden home has warmth: a cream page, plant names in a soft serif, a frost alert with a small frost illustration. Capture is clean camera with glass controls. In the analysis the stage dots sprout into checks with a light tap, the box lassoes the spot, and the question card asks about target rings in friendly words. Then the tone changes: the diagnosis card is still and serious, "Not sure yet" in italics. The plan lands as two small jobs that stamp done with a satisfying press. The closing shot is a clay diorama of the garden growing up out of the table. Judges should think "people will love using this", and still believe the diagnosis.

### 7.3 Instrument

**Exact, legible, candid.** A field instrument that shows its working: black controls, white paper, colour only where a measurement or a status lives. Scientific/data-dense with Swiss structure, and Apple-native discipline about glass.

**Accent: near-monochrome, ink black `#111814` (light) and paper white `#EEF2F0` (dark).** No brand hue at all. With colour reserved for status and data, every coloured mark on screen means something: a crimson pin is a sick plant, an amber line is a frost threshold. That is the strongest possible statement of "we only show what we measured", it maximises contrast in sun, and it makes the 3D garden's health colours pop on video.

- **Type.** [Atkinson Hyperlegible Next](https://github.com/google/fonts/tree/main/ofl/atkinsonhyperlegiblenext) 400, 600 and 700 for all content text (chrome stays SF Pro), and [Atkinson Hyperlegible Mono](https://github.com/google/fonts/tree/main/ofl/atkinsonhyperlegiblemono) 500 for measurements and units ("7 mm", "32 cm", "−2 °C at 05:00"). Both are by the Braille Institute and OFL as published on Google Fonts (the Institute's own download uses its EULA), designed so similar characters (1, l, I; 0, O) are unambiguous, which matters for numbers read in glare. Scale: the 12 pt step is dropped in content; captions are 13 pt minimum.
- **Shape.** Radii 8 pt on controls and cards, 0 on photos and charts. Borders, not shadows: 1 px `border` dividers, controls edged in `textMuted`. Buttons are solid `accent` (black) with `accentText`; secondary buttons are 1.5 pt `text` outlines.
- **Icons.** In content, SF Symbols at Medium, monochrome; Material Symbols Sharp 500 elsewhere. Status uses shapes (§8) in its colour. No tab tint: the chrome is the system's monochrome glass, untouched.
- **Imagery.** Photos edge to edge with a LiDAR scale bar when depth exists ("10 mm"). Region boxes as reticle corners with the measured size beside them ("lesion 7 × 5 mm, band: high"). Confidence bands appear only on findings and measurements, as the words "low", "medium", "high" with a three-segment bar.
- **Uncertainty.** "Not sure yet" is a comparison table, field-guide style: the two candidates as columns, rows for the marks that separate them (rings, yellow halo, lesion size), the column that matches the evidence marked, and the open row ("rings?") linking to the question card.

| Motion | What moves | Values |
|---|---|---|
| Cut | Screen entrance: opacity only, no slide | `withTiming` 180 ms, `bezier(0, 0, 0.2, 1)`. Reduced: none |
| Readout | Stage tick: the row's progress bar fills, elapsed time counts in mono, check appears | Bar spring stiffness 600, damping 49, mass 1 (ratio 1.00, ~240 ms); check 120 ms. Reduced: bar jumps, no count |
| Reticle | Region box: four corner brackets slide in from 12 pt outside to the box, then the size label types in | 260 ms `bezier(0.2, 0, 0, 1)`; label 120 ms. Reduced: brackets and label appear together, 120 ms fade |
| Log | Task check: box fills, the row moves to "Done" | Fill 120 ms linear; Reanimated `LinearTransition.duration(220)`; `selectionAsync()`. Reduced: row moves without animation |

**3D garden: the survey model.** Ground is `background` with a 0.5 m grid in `border` and a 1 m grid in `textMuted`. Beds are extruded outlines: 10 cm walls at 20% `text` opacity with a 1.5 px edge. Plants are height-true cylinders (measured height) topped with a canopy disc (measured radius) in the status colour, slightly emissive (0.15) so they read in dark mode. Leader lines carry labels: "Tomato 1 · 84 cm · sick". Unmeasured plants are dashed outlines labelled "height not measured". Lighting: flat, one hemisphere light at 1.0, no shadows (cheaper, and shadows imply detail we don't have). Camera: orthographic isometric (35.26°, 45°), with a toggle to straight top-down that lines up with the 2D map. Camera moves are critically damped (stiffness 200, damping 28.3, ~415 ms).

**How the demo feels.** Like a well-made instrument that a farmer would trust. The garden home is black type on white with only two colours on it: an amber frost line on the forecast and one crimson diamond on the sick tomato. Capture shows the LiDAR distance as a big mono readout ("32 cm, move closer to 25"). The analysis is a run log: each stage fills and stamps its time, the reticle locks onto the lesion with its size in millimetres, and the question card asks one precise thing. The diagnosis is a two-column comparison that shows exactly why the system is not sure. The plan is a checklist with a source under every step. The closing shot is the survey model rotating to top-down with every plant's height labelled. Judges should think "this is real engineering, and it is honest about its limits".

### 7.4 Palettes

Hex values for each role. Checked with the script: every text pair below passes 4.5:1 in all six palettes (lowest pairs: Herbarium 4.98 and 6.17, Seed Packet 4.96 and 4.73, Instrument 5.08 and 4.69, light and dark). The checked pairs are `text` and `textMuted` on `background`, `surface` and `surfaceMuted`; `accentText` on `accent`; `accent` on `surface`, `accentSoft` and `background`; each status and `danger` and `info` on its soft colour and on `surface`; and `text` on each soft colour. Every status colour and `accent` also passes 3:1 against `background`, for pins, marks and button shapes.

| Role | Herbarium light | Herbarium dark | Seed Packet light | Seed Packet dark | Instrument light | Instrument dark |
|---|---|---|---|---|---|---|
| background | `#F5F4EE` | `#101412` | `#FAF3E4` | `#1B1511` | `#FFFFFF` | `#0B0E0D` |
| surface | `#FFFFFF` | `#181D1A` | `#FFFCF6` | `#251D17` | `#FFFFFF` | `#141917` |
| surfaceMuted | `#ECEBE3` | `#212723` | `#F2E7D0` | `#30261E` | `#F1F3F2` | `#1C2220` |
| border | `#D5D3C7` | `#333B36` | `#E0D0B0` | `#46382C` | `#C8CECB` | `#2F3734` |
| text | `#191E1A` | `#E8ECE6` | `#2A1D12` | `#F6ECDF` | `#0A0F0C` | `#F2F5F3` |
| textMuted | `#545C55` | `#A5AEA7` | `#66523F` | `#C4B39E` | `#454E49` | `#A9B3AE` |
| accent | `#2C5C3E` | `#8BC69F` | `#7D2B63` | `#E9B0DA` | `#111814` | `#EEF2F0` |
| accentText | `#FFFFFF` | `#0E1A12` | `#FFFFFF` | `#2A0D22` | `#FFFFFF` | `#0B0E0D` |
| accentSoft | `#E1EBE2` | `#1E3127` | `#F5E3EF` | `#3C2036` | `#E5E9E7` | `#27302C` |
| danger | `#A8201A` | `#F59BA0` | `#A8201A` | `#FF9F8A` | `#B02A00` | `#FF9466` |
| dangerSoft | `#F8E2DE` | `#3B1D20` | `#FBE3DE` | `#431F19` | `#FDE8DF` | `#3E1E12` |
| info | `#285A80` | `#94C3E8` | `#1F5E7A` | `#8FCBE3` | `#0050A0` | `#7DB8FF` |
| infoSoft | `#E1EAF2` | `#1A2A37` | `#DFEEF3` | `#173040` | `#E3EEFA` | `#13253B` |
| healthy | `#28683F` | `#8AD6C0` | `#1F6A3A` | `#A6D98A` | `#006B3C` | `#8AD6C0` |
| healthySoft | `#E0EEE4` | `#173329` | `#DFF0E2` | `#22331A` | `#DDF2E6` | `#12312A` |
| watch | `#7C4D00` | `#EDBF62` | `#7E4A00` | `#F5C267` | `#8A4B00` | `#FFB547` |
| watchSoft | `#F7EACF` | `#382D16` | `#FBE9C6` | `#3D2C12` | `#FFEDD2` | `#3A2810` |
| sick | `#B0173A` | `#F3A383` | `#B0173A` | `#F590B5` | `#B3123A` | `#FF7F6E` |
| sickSoft | `#F9E1E5` | `#3A2219` | `#FCE2E6` | `#43202E` | `#FDE4E9` | `#3E1A17` |
| dead | `#56636B` | `#A9AFAB` | `#56636B` | `#9C958B` | `#56636B` | `#8E9692` |
| deadSoft | `#E4E7E8` | `#2A2E2C` | `#E8E6E1` | `#302B26` | `#E6E9EB` | `#262C2A` |

**Status colours under colour-vision deficiency.** `sick` is a crimson with some blue in it (not an orange-red) in light mode so that it stays apart from the amber `watch`, and `dead` is a cool slate so that it stays apart from `sick`. The smallest simulated difference between any two status colours is ΔE 11 (Seed Packet light, healthy and sick), against ΔE 4 for watch and sick in the current palette. That is still not enough to rely on colour, which is why §8 adds a shape per status. In dark mode, Seed Packet's `sick` pink sits closer to its plum accent (ΔE 20); the accent is never used for status, so this was accepted to keep the status colours apart.

**`danger` vs `sick`.** They share a hue family on purpose (both mean "bad") but have different jobs: `danger` is for destructive buttons and errors, `sick` for plant status. They never appear in the same component.

## 8. Rules shared by all three

- **Status is word, colour and shape.** Healthy ● circle, watch ▲ triangle, sick ◆ diamond, dead ■ hollow square. The same shapes on chips, map pins, 3D labels and the legend. Shape survives colour blindness, glare and greyscale video compression.
- **Glass only in the chrome.** The rules are in §6: system glass on tabs, bars and toolbar buttons, tint on the selected tab and one prominent button, none over photos, clear glass with 35% dimming on capture. No content card is ever glass.
- **Every motion has a still version.** Reanimated springs default to `ReduceMotion.System` ([withSpring](https://docs.swmansion.com/react-native-reanimated/docs/animations/withSpring/)); timing animations set it explicitly. Nothing communicates only through motion, and the region box and stage state are readable in a still frame. WCAG 2.3.3 (Animation from Interactions) is AAA, not required, but cheap to meet here ([Understanding 2.3.3](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html)).
- **Custom fonts and Dynamic Type.** React Native scales text by the system font scale whatever the family, so custom fonts follow Dynamic Type. Each weight is its own `fontFamily` (Android ignores `fontWeight` for custom files). Load fonts with the `expo-font` config plugin on native so there is no flash of system text; on the web they arrive as `@font-face`.
- **The 3D garden is built from map data only.** Bed polygons, pin positions, canopy radii and the latest height measurement per plant ([contracts §9, Garden map](../architecture/contracts.md#garden-map)); never meshes or photos. A plant without a measurement is drawn as "not measured", never with an invented height. The 2D SVG map stays as the default and the fallback; 3D is a toggle.
- **3D stack.** `@react-three/fiber` 9.8.2 (peer range React 19.0–19.3, React Native 0.78 or later, which SDK 57's React 19.2.3 and React Native 0.86.3 meet), `three` 0.186, and `expo-gl` ~57.0.2 on native ([R3F installation](https://r3f.docs.pmnd.rs/getting-started/installation)). The same scene runs on the web with a normal canvas. Plants are an `InstancedMesh` (at most 200 pins per map). Labels are React Native views positioned by projecting each plant's position each frame; drei's `Html` and `Text` don't work on native ([report](https://discourse.threejs.org/t/cannot-use-all-r3f-and-drei-modules-in-react-native-expo-application/44510)). Test on the iPhone, not the simulator, whose OpenGL ES support R3F calls unreliable. R3F 10 moves React Native support to a separate package that is not ready; stay on 9.
- **Adding libraries.** `expo-symbols` (SF Symbols on iOS, Material Symbols on Android and the web, [docs](https://docs.expo.dev/versions/v57.0.0/sdk/symbols/)), `expo-glass-effect`, `expo-gl`, `three` and `@react-three/fiber` are not in [frontend §4](../frontend/README.md#4-libraries) yet; each needs its reason in the pull request that adds it.

## 9. Open questions

1. **Which direction?** Mock the four demo screens and the 3D garden in all three, light mode, at phone width, then decide. Judge them in sunlight on the phone and as a 1080p screen recording.
2. **Blend after choosing.** The likely outcome is one direction's structure with another's detail, for example Instrument's comparison table for "Not sure yet" inside Herbarium. Decide in [design.md](design.md).
3. **Fraunces instances.** Which SOFT and opsz values Google's static Fraunces files use was not checked. Cut our own instances if Seed Packet wins.
4. **Tint over darkened glass.** Check on the iPhone whether native tabs and toolbar buttons keep a tint readable when the glass adapts to dark content in light mode (§6). Until then, no tint over photos.
5. **3D on the web build.** R3F on the web is standard, but the same scene shared with `expo-gl` native has not been run in this project. Do it on day 1 of the 3D work.
6. **Unverified exemplars.** Aesop, Muji, the Biodiversity Heritage Library, iNaturalist's main site, Merlin, The Light Phone and the Material 3 pages could not be opened from here. None of the recommendations depends on them.

## 10. Sources

**Principles and research**
- Dieter Rams, ten principles: [Vitsœ, Good design](https://www.vitsoe.com/us/about/good-design)
- Josef Müller-Brockmann, *Grid Systems in Graphic Design*: [Slanted listing](https://www.slanted.de/?p=792761) (*unverified*)
- Robert Bringhurst, *The Elements of Typographic Style* v4.0: [Typographica review](https://typographica.org/typography-books/the-elements-of-typographic-style-4th-edition/)
- Apple, Liquid Glass: [Newsroom, June 2025](https://www.apple.com/newsroom/2025/06/apple-introduces-a-delightful-and-elegant-new-software-design/); [HIG, Materials](https://developer.apple.com/design/human-interface-guidelines/materials); [HIG, Motion](https://developer.apple.com/design/human-interface-guidelines/motion)
- Edward Tufte: [books](https://www.edwardtufte.com/books/)
- Leonard Koren, *Wabi-Sabi for Artists, Designers, Poets & Philosophers*: [listing](https://mitpressbookstore.mit.edu/book/9780981484600) (*unverified*)
- Kenya Hara, *Designing Design*: [Lars Müller Publishers](https://www.lars-mueller-publishers.com/designing-design-0)
- Don Norman, *Emotional Design*: [jnd.org](https://jnd.org/emotional-design-why-we-love-or-hate-everyday-things/)
- Neobrutalism: [NN/g](https://www.nngroup.com/articles/neobrutalism/)
- Calm technology: [Amber Case, calmtech.com](https://calmtech.com/); [Weiser and Brown, 1995](https://calmtech.com/papers/designing-calm-technology.html)
- Solarpunk: [Adam Flynn, 2014](https://hieroglyph.asu.edu/2014/09/solarpunk-notes-toward-a-manifesto/)
- Material 3 Expressive: [Google Design research](https://design.google/library/expressive-material-design-google-research); [M3 motion](https://m3.material.io/styles/motion/overview/how-it-works) (*unverified*)
- Display polarity: [Piepenbrock et al., Ergonomics 2013](https://documentacion.fundacionmapfre.org/documentacion/publico/pt/bib/143702.do?format=xml) (*unverified*)
- WCAG 2.2: [Understanding 2.3.3](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html); [quick reference](https://www.w3.org/WAI/WCAG22/quickref/)

**Exemplars**
- [Apple Design Awards](https://developer.apple.com/design/awards/) (2026 winners and finalists) · [Things 3](https://culturedcode.com/things/) · [Teenage Engineering OP-1](https://teenage.engineering/products/op-1) · [Vitsœ](https://www.vitsoe.com/)
- [Kinfolk](https://www.kinfolk.com/) · [Apartamento](https://www.apartamentomagazine.com/) · [Aesop](https://www.aesop.com/) (*unverified*)
- [Tide Guide](https://www.tideguide.com/) · [Flighty](https://www.flighty.com/) · [Halide](https://www.halide.cam/)
- [Our World in Data](https://ourworldindata.org/) · [Johnny's Selected Seeds](https://www.johnnyseeds.com/vegetables/tomatoes/) · [iNaturalist forum](https://forum.inaturalist.org/t/better-use-of-location-in-computer-vision-suggestions/915) · [Windy](https://www.windy.com/)
- [Hudson Valley Seed Art Packs](https://hudsonvalleyseed.com/pages/art-packs) · [Muji](https://www.muji.com/) (*unverified*) · [Biodiversity Heritage Library](https://www.biodiversitylibrary.org/) (*unverified*)
- [Not Boring](https://notbor.ing/) · [Gentler Streak](https://gentlerstories.com/gentlerstreak/) · [Duolingo core tabs](https://blog.duolingo.com/core-tabs-redesign/) · [Bruno Simon](https://bruno-simon.com/)
- [Gumroad](https://gumroad.com/) · [Are.na](https://www.are.na/about) · [The Light Phone](https://www.thelightphone.com/) (*unverified*)
- [Low←Tech Magazine, the solar website](https://solar.lowtechmagazine.com/about/the-solar-website/) · [Townscaper](https://www.townscapergame.com/) · [9to5Google, M3 Expressive roundup](https://9to5google.com/2025/09/27/google-material-3-expressive-redesign/) (*unverified*)

**Typefaces and tools**
- Licences: [google/fonts](https://github.com/google/fonts) (`ofl/` directories for Newsreader, Instrument Sans and Serif, IBM Plex, Atkinson Hyperlegible Next and Mono, Fraunces, Nunito, Bricolage Grotesque, Space Grotesk and Mono, Literata, Recursive, Google Sans Flex) · [Fraunces source](https://github.com/undercasetype/Fraunces) · [Braille Institute, Atkinson Hyperlegible](https://www.brailleinstitute.org/freefont/)
- Apple chrome: [HIG, Color › Liquid Glass color](https://developer.apple.com/design/human-interface-guidelines/color#Liquid-Glass-color) · [HIG, Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars) · [Expo Router native tabs](https://docs.expo.dev/router/advanced/native-tabs/) · [Expo Router stack toolbar](https://docs.expo.dev/router/advanced/stack-toolbar/)
- Expo: [expo-glass-effect](https://docs.expo.dev/versions/v57.0.0/sdk/glass-effect/) · [expo-symbols](https://docs.expo.dev/versions/v57.0.0/sdk/symbols/) · [SDK 57 bundled modules](https://github.com/expo/expo/blob/sdk-57/packages/expo/bundledNativeModules.json)
- Animation and 3D: [Reanimated withSpring](https://docs.swmansion.com/react-native-reanimated/docs/animations/withSpring/) · [React Three Fiber installation](https://r3f.docs.pmnd.rs/getting-started/installation) · [drei on React Native](https://discourse.threejs.org/t/cannot-use-all-r3f-and-drei-modules-in-react-native-expo-application/44510)
