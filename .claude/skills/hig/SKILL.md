---
name: hig
description: "Apple Human Interface Guidelines for the PDM app: the map of every guideline page and its skill, how to apply them in Expo / React Native, and how to review a screen against them. Use when designing, building or reviewing any screen or component, or when unsure which guideline page applies."
argument-hint: "[topic, screen or file]"
---

# Apple Human Interface Guidelines (HIG)

Apple's [Human Interface Guidelines](https://developer.apple.com/design/human-interface-guidelines) say how an app on Apple's platforms should look and behave. They apply to the PDM app because it ships on iPhone, even though it is built with Expo rather than Swift. Every page and every group of the guidelines has its own skill, named after its place in Apple's tree:

| Group | Skill |
|---|---|
| Getting started | `/hig-getting-started` |
| Foundations | `/hig-foundations` |
| Patterns | `/hig-patterns` |
| Components (8 subgroups) | `/hig-components`, then `/hig-components-<subgroup>` such as `/hig-components-presentation` |
| Inputs | `/hig-inputs` |
| Technologies | `/hig-technologies` |

A page's skill is its group's skill name plus the page's slug: Sheets is `/hig-components-presentation-sheets`, Color is `/hig-foundations-color`. The whole tree, with every page's link, skill and platforms, is in [index.md](index.md).

## Rules

1. **Read the page live; never work from memory.** Apple revises the guidelines often. `python3 .claude/skills/hig/scripts/hig_page.py <slug> [--section <anchor>]` prints the current page or one section as Markdown.
2. **Translate, don't transliterate.** The guidelines name UIKit and SwiftUI APIs. Use [expo.md](expo.md) for the Expo SDK 57 and React Native equivalent, and keep to [docs/frontend](../../../docs/frontend/README.md): its UI system (section 14), its libraries (section 4) and its platform adapters (section 13).
3. **Link every rule to its section.** Each page skill lists its section links. Link to Apple's page; don't copy its text into the repository. The guidelines are © Apple.
4. **Platforms.** The guidelines govern the iPhone build. On Android, follow [Material Design 3](https://m3.material.io/) where the platforms behave differently: the back gesture, sheets and system controls. On the web build, [WCAG 2.2](https://www.w3.org/WAI/WCAG22/quickref/) applies. Pages for macOS, tvOS, visionOS or watchOS only are marked in their skill. Claude doesn't load those on its own, but they can be used by name.

## Building a screen or component

1. List what the screen contains: navigation, presentation (sheet, alert, menu), controls, text, images, status, input, and system features such as notifications, the camera or location.
2. Find each item's page in [index.md](index.md). Always include `/hig-foundations-accessibility`, `/hig-foundations-layout`, `/hig-foundations-typography`, `/hig-foundations-color` and `/hig-foundations-dark-mode`.
3. Use those pages' skills. Read the relevant sections live, then build with the equivalents in [expo.md](expo.md).
4. Note in the change which sections shaped the decisions, with their links.

## Reviewing a screen, a component or docs/frontend/screens.md

1. Do steps 1 and 2 of building.
2. Read each relevant section live and check the screen against it.
3. Report each finding with:
   - its severity: **must** (breaks a rule, accessibility, or App Store review), **should** (differs from the platform convention) or **could** (polish);
   - the section link;
   - what the screen does now, and the fix in Expo terms.
4. Check the Android and web behaviour of every fix (rule 4).

## Keeping the skills current

`python3 .claude/skills/hig/scripts/build_skills.py` walks Apple's tree again and regenerates every `hig-*` skill and [index.md](index.md). Run it when Apple adds, renames or moves a page, then review the diff. It doesn't touch this file, [expo.md](expo.md) or the scripts.
