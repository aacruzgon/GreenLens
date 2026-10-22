# PDM Frontend Architecture

> **Status:** Draft v0.4.10, 2026-10-22. v0.4.10: `fflate`, and `expo-image-manipulator` sampling the diagnosis's photo for its tint (section 4; design open decision 9). v0.4.9: `@expo/ui` gives the live analysis's More button SwiftUI's `Menu` on iOS (section 4). v0.4.8: `AuthBridge` registers the token provider once per sign-in, before the first authed request (section 8). v0.4.7: "Forgot password?" resets the password with an email code (sections 6 and 8). v0.4.6: two-step sign-in is a code by text to the phone given at sign-up, with Clerk's test numbers in development (section 8). v0.4.5: sign-in has no authenticator-app second factor; a new device is confirmed with an email code. v0.4.4: section 4 adds `expo-symbols` and Urbanist, which F0 uses.
> v0.4.3: the build plan (section 18) follows the new [roadmap](roadmap.md): the iPhone first in the Conservatory direction, F0 on Oct 20 to the web build on Oct 25. v0.4.2: the backend's OpenAPI spec is committed, so the app uses generated contract types from day 1 instead of hand-written ones (section 9, generated types; build plan). v0.4.1: the schema links point to `backend/migrations/schema.surql`. v0.4 applies the third audit and contracts v0.8.2: species photo paths carry a content hash, a failed scan never holds a question, push coverage, invalidations for confirmed plan revisions, the closer-photo rule, the `SFSafariViewController` cookie note and the CORS preflight reason.
> v0.3 applies the second audit and contracts v0.8.1:
>   - **Images:** event URLs arrive fresh, and the cache key is the image ID or the URL path; species photos carry their credit.
>   - **Scan stream:** the resting rule counts guided checks on failed scans too.
>   - **Diagnosis:** with `low_evidence`, the closer photo answers the open question.
>   - **Push:** the adapter handles notifications that arrive while the app is open.
>   - **Smaller fixes:** query invalidations, a new-plant sheet, browser detection aimed at embedded browsers, the HTTPS reasons, and CORS for `Content-Type`.
>
> v0.2 applied the first audit, adds phone browsers, and uses the fields [contracts](../architecture/contracts.md) v0.8 added for the screens (section 20):
>   - **Diagnosis:** worded from its status and `low_evidence`, since it has no confidence of its own.
>   - **Uploads:** re-encoded uploads get a new key, and LiDAR shots are never re-encoded.
>   - **Scan stream:** opened with a full event replay; the resting rule counts guided checks.
>   - **Errors and retries:** every error code has a row, and `budget_exceeded` is never retried.
>   - **Chat:** a retried send handles the JSON replay, and the rescan button is gated by role.
>   - **API layer:** invalidations are corrected, and `Transport` matches `openapi-fetch`.
>   - **New contract fields in use:** thumbnails, garden weather, the species list with reference photos, the prey of beneficials, a task's scan, the push payload, deleting a pending scan.
>   - **Browsers:** capture in phone browsers uses the system camera, and in-app browsers get their own sign-in rules.
>   - **Sources:** every section links its documentation.
>
> v0.1: First frontend architecture for the Expo app with Clerk authentication.
> **Checked:** versions and APIs marked ✓ were read from the npm registry and the published packages on 2026-10-15. Nothing here has run in a build yet. Points marked *unverified* come from vendor docs only.

This is the frontend's architecture. It is built on the [system architecture](../architecture/README.md) (sections 3, 6.6–6.8, 7, 8, 9 and 12) and the [data contracts](../architecture/contracts.md), which define every shape the app sends and receives. The two companion documents cover the rest:

| Document | What it holds |
|---|---|
| [README.md](README.md) (this one) | Structure: layers, modules, libraries, auth, API, state, platform, UI system, tooling, builds, contract gaps, sources |
| [flows.md](flows.md) | Behaviour: sequence diagrams for each user flow, and the state machines behind them |
| [screens.md](screens.md) | The product surface: navigation map, every screen with its data, actions, roles and states, and wireframes of the main screens |

## Contents

1. [Goals](#1-goals)
2. [System context](#2-system-context)
3. [Stack and version pins](#3-stack-and-version-pins)
4. [Libraries](#4-libraries)
5. [Layers and import rules](#5-layers-and-import-rules)
6. [Directory layout](#6-directory-layout)
7. [Runtime composition](#7-runtime-composition)
8. [Authentication (Clerk)](#8-authentication-clerk)
9. [API layer](#9-api-layer)
10. [Live streams](#10-live-streams)
11. [Feature modules](#11-feature-modules)
12. [State ownership](#12-state-ownership)
13. [Platform adapters and the LiDAR module](#13-platform-adapters-and-the-lidar-module)
14. [UI system](#14-ui-system)
15. [Mock mode](#15-mock-mode)
16. [Testing and tooling](#16-testing-and-tooling)
17. [Builds, environments and delivery](#17-builds-environments-and-delivery)
18. [Build plan](#18-build-plan)
19. [Open decisions](#19-open-decisions)
20. [Contract changes the screens need](#20-contract-changes-the-screens-need)
21. [Sources](#21-sources)

---

## 1. Goals

- **One codebase for iOS, Android and the web build.** The web build is the live link the judges open (architecture 12). They may open it on a laptop, in their phone's browser, or inside another app's browser (section 13), so all three are first-class targets.
- **Screens render; they don't decide.** Business rules (the scan event model, the capture rules, the role matrix, error handling) live in plain TypeScript modules with no React and no I/O. They are tested on their own and can't drift between screens.
- **One door to the backend.** Only the `api` layer talks HTTP. It owns auth headers, idempotency keys, error parsing and streaming. Features call typed functions and hooks, never `fetch`.
- **Device features behind interfaces.** Camera, LiDAR, location, push and storage are adapters with web and native implementations. A feature asks what the device can do; it never checks `Platform.OS` itself.
- **Built from the contracts before the backend exists.** A mock transport serves the contract shapes and replays scripted scans, so frontend work starts on Oct 16 alongside the backend (section 15).
- **Only data the contracts provide.** Every field a screen shows comes from a contract. Contracts v0.8 added the ones the screens were missing (section 20).
- **Clear about uncertainty.** Diagnoses are worded from their status and `low_evidence`, provisional plans say so, findings show their confidence in words, and errors say what to do next. This follows the safety rules of architecture 11.

---

## 2. System context

```
                     ┌─────────────────────────────┐
                     │ Clerk                       │
                     │ sign-in · sessions          │
                     │ JWKS · webhooks (to the API)│
                     └──────▲──────────────────────┘
                            │ hooks: useSignIn, useSSO, useAuth().getToken()
┌───────────────────────────┴───────────────────────────────────────────────┐
│ PDM APP  (Expo SDK 57 · React Native 0.86 · Expo Router)                   │
│ native on iOS and Android · web build in desktop and phone browsers        │
│                                                                            │
│  routes ─▶ features ─▶ api ──────────────────────────────┐                 │
│                │        │  REST (openapi-fetch)          │                 │
│                │        │  SSE  (streaming fetch)        │                 │
│                ▼        │                                │                 │
│            platform ────┼── camera · LiDAR module · location · push · files │
└─────────────────┬───────┼────────────────────────────────┼─────────────────┘
                  │       │ HTTPS, Authorization: Bearer   │ signed image URLs
   ┌──────────────┘       ▼                                ▼ (no auth header)
   │            ┌────────────────────────────────────────────────┐
   │            │ api (Rust · axum), behind Caddy                │
   │            │ /scans · /scans/{id}/events (SSE) · /plants ·  │
   │            │ /gardens · /tasks · /conversations · /me · …   │
   │            └───────────────────────┬────────────────────────┘
   │                                    │ Expo Push API
   │            ┌───────────────────────▼────────────────────────┐
   └───────────▶│ APNs / FCM ─▶ notification ─▶ route in the app │
   push token   └────────────────────────────────────────────────┘
```

| Boundary | Protocol | Contract |
|---|---|---|
| App ↔ Clerk | Clerk SDK (`@clerk/expo`) | Clerk's API |
| App → API | HTTPS JSON and multipart, `Authorization: Bearer <Clerk session token>` | [contracts](../architecture/contracts.md) 1–3, 7–9 |
| API → App (scan progress) | SSE over a streaming `GET`, resumable with `Last-Event-ID` | contracts 4 |
| API → App (chat) | SSE over a streaming `POST`, not resumable | contracts 8 |
| App → images | Signed, 15-minute URLs, no auth header | contracts 1 (image URLs) |
| API → phone | Expo Push → APNs or FCM, with a `PushPayload` | contracts 9 (push notifications); architecture 8.3, 12 |
| App ↔ LiDAR hardware | The native module `app/modules/lidar` (Swift, Expo Modules API) | Section 13; architecture 6.7, 6.8 |

---

## 3. Stack and version pins

| Item | Pin | Why |
|---|---|---|
| Expo SDK | **57** (`expo@57.0.30`) ✓ | `@clerk/expo` 4.12.0, the latest release, declares `expo: ">=54 <58"`, and so does its current canary ✓. SDK 58 (released 2026-09-29) is outside Clerk's supported range. Move to 58 once Clerk widens it (open decision 1) |
| React Native | 0.86.3 ✓ | Bundled with SDK 57 |
| React | 19.2.3 ✓ | Bundled with SDK 57 |
| Expo Router | `~57.0.28` ✓ | File-based routes, typed routes, `Stack.Protected` guards ✓ |
| Clerk | `@clerk/expo` `^4.12.0` ✓ | The current Expo SDK. `@clerk/clerk-expo` (2.20) is the old package name |
| TypeScript | The version the SDK 57 template pins, `strict: true` | The generated API types need strict null checks to be useful |
| Package manager | npm, with `package-lock.json` committed | The Expo default |

Expo and native packages are installed with `npx expo install <pkg>`, which picks the version bundled with the SDK (for example `react-native-reanimated` 4.5.1 and `react-native-svg` 15.15.4 for SDK 57 ✓, from the SDK's [`bundledNativeModules.json`](https://github.com/expo/expo/blob/sdk-57/packages/expo/bundledNativeModules.json)), never with a plain `npm install`. CI runs `npx expo install --check`.

Docs: [Expo SDK 57 reference](https://docs.expo.dev/versions/v57.0.0/) · [SDK 57 changelog](https://expo.dev/changelog/sdk-57) · [`npx expo install`](https://docs.expo.dev/more/expo-cli/#install) · [`@clerk/expo` 4.12.0 on the npm registry](https://registry.npmjs.org/@clerk/expo/4.12.0).

---

## 4. Libraries

Each concern has one library. A dependency not in this table needs a reason in the pull request that adds it.

| Concern | Library | Notes | Docs |
|---|---|---|---|
| Navigation | `expo-router` | Routes in `src/app/`, typed routes on. Native sheets (`presentation: "formSheet"`) instead of a bottom-sheet library | [Introduction](https://docs.expo.dev/router/introduction/), [modals](https://docs.expo.dev/router/advanced/modals/) |
| Auth | `@clerk/expo`, `expo-secure-store`, `expo-auth-session`, `expo-web-browser`, `expo-crypto` | Section 8. Token cache from `@clerk/expo/token-cache` ✓ | [Expo SDK reference](https://clerk.com/docs/reference/expo/overview) |
| Server state | `@tanstack/react-query` v5 | Caching, retries, background refetch, invalidation driven by scan events. Devtools in development only | [Overview](https://tanstack.com/query/latest/docs/framework/react/overview), [React Native](https://tanstack.com/query/latest/docs/framework/react/react-native) |
| HTTP client | `openapi-fetch` over `expo/fetch` | Typed by the generated `paths`; middleware adds auth, idempotency and error parsing (section 9). `expo/fetch` gives streaming bodies on native ✓ | [openapi-fetch](https://openapi-ts.dev/openapi-fetch/), [`expo/fetch`](https://docs.expo.dev/versions/latest/sdk/expo/#expofetch-api) |
| API types | `openapi-typescript` (dev) | Generates `src/api/generated/schema.ts` from the backend's OpenAPI spec (contracts 1) | [openapi-typescript](https://openapi-ts.dev/introduction) |
| Client state | `zustand` v5 | Only state that isn't on the server: active garden, capture drafts, map draft, preferences (section 12) | [Zustand](https://zustand.docs.pmnd.rs/), [persist](https://zustand.docs.pmnd.rs/reference/middlewares/persist) |
| Native menus | `@expo/ui` (`@expo/ui/swift-ui`) | SwiftUI's `Menu` where the HIG wants a menu, opening from its button: so far the live analysis's **Check your soil**. iOS only, with an alert of the same choices on Android and the web. Included in Expo Go (its SDK 57 page, read Oct 22) | [Expo UI](https://docs.expo.dev/versions/v57.0.0/sdk/ui/), [SwiftUI components](https://docs.expo.dev/versions/v57.0.0/sdk/ui/swift-ui/) |
| Key-value storage | `@react-native-async-storage/async-storage` | Works in Expo Go and on the web. The only secret is Clerk's session, in `expo-secure-store` | [AsyncStorage](https://docs.expo.dev/versions/v57.0.0/sdk/async-storage/) |
| Forms and validation | `react-hook-form`, `zod` v4 | Garden, plant, task, note and feedback forms. Zod also validates the `EXPO_PUBLIC_*` config at start-up | [React Hook Form](https://react-hook-form.com/), [Zod](https://zod.dev/) |
| Images | `expo-image` | Disk cache keyed by image ID, not by signed URL (section 9) | [expo-image](https://docs.expo.dev/versions/v57.0.0/sdk/image/) |
| Overlays, charts, map | `react-native-svg` | Region boxes and masks over photos, the growth curve and the top-down garden map are small SVG drawings. No chart library | [react-native-svg](https://docs.expo.dev/versions/v57.0.0/sdk/svg/) |
| Gestures and animation | `react-native-gesture-handler`, `react-native-reanimated` 4, `react-native-worklets` | Pan and zoom on photos and the map, stage progress | [Gesture Handler](https://docs.swmansion.com/react-native-gesture-handler/), [Reanimated](https://docs.swmansion.com/react-native-reanimated/) |
| Lists | `@shopify/flash-list` v2 | Timeline, tasks, chat messages | [FlashList](https://shopify.github.io/flash-list/) |
| Keyboard | `react-native-keyboard-controller` | Chat composer | [Keyboard Controller](https://kirillzyusko.github.io/react-native-keyboard-controller/) |
| Camera | `expo-camera` | Capture view on phones without LiDAR. LiDAR devices use the module's own view (section 13) | [expo-camera](https://docs.expo.dev/versions/v57.0.0/sdk/camera/) |
| System camera and picker | `expo-image-picker` | Capture in browsers: the system camera on phones, a file picker on desktops. "Choose from library" everywhere | [expo-image-picker](https://docs.expo.dev/versions/v57.0.0/sdk/imagepicker/) |
| Image conversion | `expo-image-manipulator` | HEIC to JPEG for picked photos until the API decodes HEIC (contracts open question 8). Never applied to LiDAR files. Also cuts the diagnosis's photo to 32 px as a PNG for its tint (design open decision 9) | [expo-image-manipulator](https://docs.expo.dev/versions/v57.0.0/sdk/imagemanipulator/) |
| Files | `expo-file-system` | `File` objects for multipart uploads with `expo/fetch`, which rejects React Native's `{ uri }` form parts ✓ | [expo-file-system](https://docs.expo.dev/versions/v57.0.0/sdk/filesystem/) |
| Location | `expo-location` | Garden centroid, with consent (architecture 8.5) | [expo-location](https://docs.expo.dev/versions/v57.0.0/sdk/location/) |
| Push | `expo-notifications`, `expo-device` | Development builds only (architecture 12) | [Push overview](https://docs.expo.dev/push-notifications/overview/), [expo-notifications](https://docs.expo.dev/versions/v57.0.0/sdk/notifications/) |
| Haptics | `expo-haptics` | Shot accepted, retake needed, card confirmed | [expo-haptics](https://docs.expo.dev/versions/v57.0.0/sdk/haptics/) |
| Links | `expo-linking` | Invite links and notification routes | [Linking](https://docs.expo.dev/linking/overview/), [expo-linking](https://docs.expo.dev/versions/v57.0.0/sdk/linking/) |
| PNG decoding | `fflate` | Inflates that 32 px PNG in plain JS for `lib/png.ts`, so the tint works in Expo Go; no native module | [fflate](https://github.com/101arrowz/fflate) |
| Dates | `date-fns` v4, `@date-fns/tz` | `LocalDate` values are in the garden's `time_zone`, never the phone's | [date-fns](https://date-fns.org/), [@date-fns/tz](https://github.com/date-fns/tz) |
| Icons | `@expo/vector-icons` | Works on every target | [Icons](https://docs.expo.dev/guides/icons/) |
| SF Symbols | `expo-symbols` | Tab bar icons on iOS through `NativeTabs`' `sf` (roadmap F0) | [expo-symbols](https://docs.expo.dev/versions/v57.0.0/sdk/symbols/) |
| Display font | `@expo-google-fonts/urbanist` (600 only) with the `expo-font` config plugin | Conservatory's content heroes at 22 pt and above ([design 3.1](../design/design.md#31-conservatory)); also loaded with `useFonts` so it works in Expo Go. The system font stands in if it fails to load | [Fonts](https://docs.expo.dev/develop/user-interface/fonts/) |
| LiDAR | Expo Modules API (Swift), `app/modules/lidar` | Section 13 | [Modules overview](https://docs.expo.dev/modules/overview/), [Module API](https://docs.expo.dev/modules/module-api/), [native views](https://docs.expo.dev/modules/native-view-tutorial/) |
| Tests | `jest-expo`, `@testing-library/react-native` v14 | Section 16 | [Unit testing](https://docs.expo.dev/develop/unit-testing/), [Testing Library](https://callstack.github.io/react-native-testing-library/) |
| Lint | `eslint-config-expo`, `eslint-plugin-import` (`no-restricted-paths`) | The import rules of section 5 are lint errors | [Using ESLint](https://docs.expo.dev/guides/using-eslint/) |

**Considered and not used.**

| Option | Why not |
|---|---|
| NativeWind, Unistyles, Tamagui | Adds a compiler or native dependency for what tokens and `StyleSheet` already do (open decision 2). Unistyles 3 also needs a development build |
| `react-native-mmkv` | Needs a development build, which breaks Expo Go, and adds nothing at this data size |
| `EventSource` and its polyfills | Can only `GET` and can't send the auth header (architecture 9; [MDN `EventSource`](https://developer.mozilla.org/en-US/docs/Web/API/EventSource)) |
| `expo-camera` as the browser capture | Works in browsers, but its web `takePictureAsync` returns a base64 frame from the video stream ([expo-camera](https://docs.expo.dev/versions/v57.0.0/sdk/camera/)), likely below the resolution of the system camera's photo, which matters for small lesions (open decision 9) |
| Victory, Skia charts | One line chart doesn't justify them |
| Redux, MobX | TanStack Query holds the server state, which is most of the state |
| `@gorhom/bottom-sheet` | Expo Router's native sheets cover it without another Reanimated-dependent library |

---

## 5. Layers and import rules

### Layer diagram

Arrows point from the importer to what it imports. Nothing imports upwards.

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ROUTES   src/app/**                                                      │
│ one file per URL: read params, render a feature screen                   │
└───────────────────────────────┬──────────────────────────────────────────┘
                                │ imports features/<name> (index.ts only)
┌───────────────────────────────▼──────────────────────────────────────────┐
│ FEATURES   src/features/<name>/                                          │
│                                                                          │
│   screens/ ──▶ hooks/ (view models) ──▶ model/ (pure logic)              │
│      │            │                                                      │
│      ▼            ├──▶ api hooks and functions                           │
│  components/      ├──▶ platform adapters                                 │
│  (props only)     └──▶ other features' index.ts                          │
└───────┬───────────────┬─────────────────────┬────────────────────────────┘
        │               │                     │
┌───────▼──────┐ ┌──────▼─────────┐ ┌─────────▼──────────┐
│ UI           │ │ API            │ │ PLATFORM           │
│ src/ui       │ │ src/api        │ │ src/platform       │
│ tokens,      │ │ transport,     │ │ camera, LiDAR,     │
│ primitives,  │ │ client, errors,│ │ location, push,    │
│ composites   │ │ SSE, queries   │ │ files, storage     │
└───────┬──────┘ └──────┬─────────┘ └─────────┬──────────┘
        │               │                     │
┌───────▼───────────────▼─────────────────────▼────────────────────────────┐
│ DOMAIN   src/domain                                                      │
│ contract types · role matrix · scan rules · regions · formatting         │
│ no React · no I/O                                                        │
└───────────────────────────────┬──────────────────────────────────────────┘
                                │
┌───────────────────────────────▼──────────────────────────────────────────┐
│ LIB   src/lib    uuid · backoff · assertNever · small generic helpers    │
└──────────────────────────────────────────────────────────────────────────┘
```

### Rules

| Layer | Holds | May import | Never |
|---|---|---|---|
| Routes (`src/app`) | One file per route, plus layouts | `features/*` (their `index.ts`), `ui` | `api`, `platform`, `fetch` |
| `features/<name>` | Screens, feature components, view-model hooks, the feature's `model/` | `api`, `domain`, `ui`, `platform`, `lib`, other features' `index.ts` | Another feature's internals |
| `features/<name>/model` | Pure logic: reducers, state machines, selectors, mappers | `domain`, `lib` | React, `api`, `platform` |
| `api` | Transport, typed client, resource functions, query keys, query and mutation hooks, SSE parser | `domain`, `lib` | `features`, `ui`, `platform`, Clerk (it receives a token function, section 8) |
| `platform` | Adapters with `.native.ts` / `.web.ts` (or `.ios.ts`) implementations, the LiDAR module's JS side | `domain`, `lib` | `features`, `api`, `ui` |
| `ui` | Tokens, primitives, shared composites | `domain` (display types such as `ConfidenceBand`), `lib` | `api`, `features`, `platform` |
| `domain` | Contract types, role matrix, scan rules, display helpers | `lib` | Everything else |
| `lib` | Framework-free helpers | Nothing in `src` | |

### Inside a feature

```
 ScanScreen (screens/)                       rendered by src/app/(app)/scans/[scanId].tsx
     │ calls one view-model hook
     ▼
 useScanScreen(scanId) (hooks/)
     │ ├─ useScan(scanId)                 ── api: GET /scans/{id} through TanStack Query
     │ ├─ useScanEvents(scanId)           ── api: SSE stream (section 10)
     │ ├─ applyScanEvent / effectsOf      ── model: pure reducer and side-effect list
     │ ├─ useCan(gardenId, "scan.answer") ── domain: role matrix
     │ └─ useAnswerQuestion()             ── api: POST /scans/{id}/answers
     │ returns { view, actions }: plain data and callbacks
     ▼
 <StageProgress/> <PhotoWithRegions/> <DiagnosisCard/> <PlanChecklist/> (components/, ui/)
     props only: render in tests and previews with no providers
```

- **A feature's `index.ts` is its public API.** It exports its screens and the few hooks or components other features use, for example `features/capture` exports `useStartCapture`, which chat uses for `rescan_requested`. Lint blocks deep imports into another feature.
- **One view-model hook per screen** joins queries, the pure model and actions. Components below it take props only.
- **`model/` has no React**, so the rules that matter (event order, capture limits, role checks) run in plain Jest tests against fixtures copied from the contracts.

---

## 6. Directory layout

The Expo project is `app/` at the repository root (architecture 13). Routes live in `app/src/app/`, which is Expo Router's default when a project has a `src/` folder ([src directory](https://docs.expo.dev/router/reference/src-directory/)).

```
app/
├─ app.config.ts                  name, scheme "pdm", plugins (expo-secure-store, @clerk/expo, lidar), typed routes, web manifest
├─ eas.json                       development, preview and production profiles
├─ .env.example                   EXPO_PUBLIC_* placeholders (section 17)
├─ modules/lidar/                 Expo native module, Swift (section 13)
│  ├─ ios/                        LidarCameraView, GardenMappingView, depth capture, ARWorldMap storage
│  ├─ src/index.ts                the module's TypeScript surface and its fallback
│  └─ expo-module.config.json
├─ scripts/gen-api.ts             openapi-typescript on backend/openapi.json
└─ src/
   ├─ app/                        routes only (screens.md 1)
   ├─ features/
   │  ├─ auth/                    sign-in, sign-up, code verification, password reset, onboarding, session bootstrap
   │  ├─ gardens/                 garden home, create and edit, location consent, active garden, garden switcher
   │  ├─ members/                 members, roles, invites, invite acceptance
   │  ├─ plants/                  plant list and detail, timeline, growth curve, species confirmation, notes
   │  ├─ capture/                 guided capture, question photos, guided soil checks, upload queue
   │  ├─ scans/                   live progress, findings, diagnosis, questions, plan, feedback
   │  ├─ tasks/                   task list, alert inbox, task editor
   │  ├─ chat/                    conversations, streaming messages, confirmation cards, memories, message feedback
   │  ├─ map/                     garden map view and LiDAR mapping session
   │  └─ account/                 profile, notifications, account deletion
   │     every feature:
   │     ├─ index.ts              public API
   │     ├─ screens/              one component per route the feature owns
   │     ├─ components/           presentational, props only
   │     ├─ hooks/                view-model hooks
   │     └─ model/                pure logic, with its tests beside it
   ├─ api/
   │  ├─ generated/schema.ts      openapi-typescript output; never edited by hand
   │  ├─ transport.ts             Transport interface; live and mock implementations
   │  ├─ client.ts                openapi-fetch client and its middleware
   │  ├─ auth.ts                  setTokenProvider (section 8)
   │  ├─ errors.ts                ApiError, ErrorBody parsing, retry policy
   │  ├─ idempotency.ts           keys per user action
   │  ├─ sse.ts                   SSE parser over a byte stream
   │  ├─ pages.ts                 follows next_cursor for lists that must be complete
   │  ├─ queryKeys.ts             the one query-key factory
   │  ├─ queryClient.ts           defaults: retries, stale times
   │  ├─ resources/               scans, plants, gardens, members, tasks, chat, memories, map, me
   │  └─ mock/                    mock transport, fixture store, scripted runs (section 15)
   ├─ domain/
   │  ├─ types.ts                 contract types the app uses, re-exported from generated
   │  ├─ permissions.ts           the role matrix of contracts 1
   │  ├─ scan.ts                  statuses, stages, resting rule, image kind limits
   │  ├─ diagnosis.ts             wording from Diagnosis.status and low_evidence (screens.md 6)
   │  ├─ regions.ts               normalised boxes to view coordinates
   │  └─ format.ts                units, confidence bands, local dates
   ├─ platform/                   capabilities, browser, capture, lidar, location, push, files, storage, haptics, linking
   ├─ ui/
   │  ├─ tokens.ts, theme.tsx
   │  ├─ primitives/              Text, Button, Card, Chip, Badge, Banner, Sheet, TextField, …
   │  └─ composites/              ConfidenceBadge, StatusChip, PhotoWithRegions, LineChart, CitationList, …
   ├─ lib/
   └─ config/env.ts               zod-validated EXPO_PUBLIC_* values
```

---

## 7. Runtime composition

### Provider tree

```
<ClerkProvider publishableKey tokenCache>          session, kept in expo-secure-store on phones
 └─ <ClerkLoaded>                                  splash screen stays up until here
     └─ <AuthBridge/>                              hands getToken() to api/auth.ts
     └─ <QueryClientProvider client>               server state
         └─ <ThemeProvider>                        tokens, light and dark
             └─ <CapabilitiesProvider>             LiDAR, mapping, push, camera, browser kind (section 13)
                 └─ <PushListener/>                taps and arrivals (flows 12)
                 └─ <RootStack>                    Expo Router
                     ├─ Stack.Protected guard={!isSignedIn}  → (auth)/*
                     ├─ Stack.Protected guard={isSignedIn}   → (app)/*
                     │   └─ <SessionGate>          GET /me, GET /gardens, active garden
                     │       └─ (tabs) and stacked screens
                     └─ invite/[code]              outside the guards
```

Docs: [Expo Router protected routes](https://docs.expo.dev/router/advanced/protected/) · [authentication in Expo Router](https://docs.expo.dev/router/advanced/authentication/).

### Request and event data flow

```
 user taps                                      server pushes
    │                                                │
    ▼                                                ▼
 component ──callback──▶ view-model hook       SSE bytes ─▶ api/sse.ts parser
                           │                                    │ ScanEvent
                           ▼                                    ▼
                  mutation (TanStack Query)      model/applyScanEvent ─▶ new view state
                           │                     model/effectsOf      ─▶ [invalidate qk.scan(id), …]
                           ▼                                    │
               api/resources/*.ts                               ▼
                           │                      queryClient.invalidateQueries
                           ▼                                    │
          openapi-fetch + middleware                            ▼
          auth → idempotency → errors              refetch GET /scans/{id}
                           │                                    │
                           ▼                                    ▼
                  Transport.fetch  ─── HTTPS ───▶ api ─────▶ cache updated ─▶ components re-render
```

The rule: **events change what the screen shows; the cache is refreshed by refetching records**, never by writing event data into it by hand. The reducer's state is the scan's live view; the query cache holds the records.

---

## 8. Authentication (Clerk)

### Token flow

```
 Clerk session (secure store on phones, Clerk's browser session on the web)
        │
        ▼
 useAuth().getToken()  ── AuthBridge registers it ──▶ api/auth.ts setTokenProvider(fn)
                                                            │
 api request ─▶ auth middleware ─▶ token = await fn() ──────┘
                     │
                     ▼
        Authorization: Bearer <token> ─▶ API verifies against Clerk's JWKS
                     │
             401 unauthenticated?
                     │ yes, first time
                     ▼
        fn({ skipCache: true }) and retry once ─▶ 401 again ─▶ signOut()
```

- The api layer never imports Clerk. `AuthBridge`, a component inside `ClerkProvider`, registers `getToken` with `setTokenProvider` and clears it on sign-out.
- `AuthBridge` registers the provider once per sign-in, in a layout effect, so it is in place before the first authed request (`SessionGate`'s `GET /me` goes out in a child's effect). `@clerk/expo`'s `useAuth` returns a new `getToken` on every render, so the bridge keeps the latest in a ref: re-registering on every render would end the api layer's session and drop a refresh in flight (#97).
- Clerk caches its session token and refreshes it close to expiry, so asking for it on every request is cheap ([session tokens](https://clerk.com/docs/guides/sessions/session-tokens)).
- Code outside React (the push handler) reads the token with `getClerkInstance()` ✓.

### Sign-in

Custom screens built with Clerk's hooks, `useSignIn`, `useSignUp` and `useSSO` ✓, so sign-in matches the app's design and works in Expo Go, development builds and the web build (open decision 3).

| Method | Flow | Where it's offered |
|---|---|---|
| Email and password | Sign-up sends an email code; `verify` takes it | Everywhere, including in-app browsers |
| Google, Apple | `useSSO` in a browser session (`expo-web-browser`), returning to the `pdm` scheme; a redirect on the web | Native builds, browsers, and the system's in-app views (`SFSafariViewController`, Custom Tabs). **Not in embedded web views** (`in_app`: Instagram, Facebook, LinkedIn): Google refuses OAuth in embedded browsers ([Google's policy](https://developers.googleblog.com/upcoming-security-changes-to-googles-oauth-20-authorization-endpoint-in-embedded-webviews/)), so the app hides both and shows "Open in Safari or Chrome" instead (section 13) |
| Phone and two-step sign-in | Sign-up asks for a phone number and verifies it with a code by text after the email code. Once signed in, that phone is reserved for the second factor, so every later sign-in asks for a code by SMS (`signIn.mfa.sendPhoneCode`, `verifyPhoneCode`). The authenticator app and backup codes are off (Oct 19, the team). On the development instance, Clerk's test numbers (+1 XXX 555-0100 to 0199) send no text and take the code 424242 | Everywhere |
| Forgot password | `(auth)/reset-password`: `signIn.create({ identifier })`, then `signIn.resetPasswordEmailCode` sends a code to the account's email, verifies it and sets the new password, signing out other sessions. The sign-in then continues as after a password: the code by text, or the new-device code. Offered on sign-in, and on sign-up when the email is taken (`form_identifier_exists` on `email_address`) | Everywhere |
| New device (Device Trust) | An account without a second factor that signs in on a new device gets `needs_client_trust`: the app sends an email code (`signIn.mfa.sendEmailCode`) and `verify` checks it | Everywhere |

Clerk Core 3 changed the sign-in and sign-up calls (`signUp.password(...)`, `signUp.verifications.sendEmailCode()` in the quickstart, *unverified*). The screens follow the docs of the installed version. They all live in `features/auth`, so a change touches one folder. `app.config.ts` lists the `expo-secure-store` and `@clerk/expo` config plugins (*unverified*, Clerk quickstart).

Docs: [Expo quickstart](https://clerk.com/docs/expo/getting-started/quickstart) · [`useAuth`](https://clerk.com/docs/reference/hooks/use-auth) · [`useSignIn`](https://clerk.com/docs/reference/hooks/use-sign-in) · [`useSignUp`](https://clerk.com/docs/reference/hooks/use-sign-up) · [`useUser`](https://clerk.com/docs/reference/hooks/use-user) · [`useSSO`](https://clerk.com/docs/reference/expo/native-hooks/use-sso) · [email and password custom flow](https://clerk.com/docs/guides/development/custom-flows/authentication/email-password) · [MFA custom flow](https://clerk.com/docs/guides/development/custom-flows/authentication/multi-factor-authentication) · [managing MFA](https://clerk.com/docs/guides/development/custom-flows/account-updates/manage-mfa) · [forgot password](https://clerk.com/docs/guides/development/custom-flows/authentication/forgot-password) · [Expo web support](https://clerk.com/docs/guides/development/web-support/overview) · [the `Clerk` object](https://clerk.com/docs/reference/objects/clerk).

### Session lifecycle

| Moment | What the app does |
|---|---|
| App start | Splash until Clerk is loaded, then the guard picks `(auth)` or `(app)` |
| First sign-in | `GET /me`. The API creates the user on the first request if the webhook hasn't yet (architecture 4). If `display_name` is empty, the app sends Clerk's full name or username with `PATCH /me`, or asks for one in onboarding |
| No garden | Onboarding: create a garden with location consent or a postal code. An invite link skips this |
| Sign-out | Delete this device's push token (`DELETE /me/push-tokens/{token}`, the token URL-encoded), clear the query cache, the zustand stores and capture drafts, then `signOut()` |
| Account deletion | Explain what is deleted and what stays with shared gardens (architecture 5), `DELETE /me`, show `deleted_gardens` and `transferred_gardens`, sign out locally. The API deletes the account at Clerk |

The sequence diagrams are in [flows.md](flows.md#1-app-start-and-sign-in).

### What the backend needs from Clerk

The webhook and header rules are in contracts 1 and 9 (auth webhook). The token checks below are to be confirmed when the API's auth middleware is written (*unverified*, Clerk docs):

- The API verifies session tokens against the Clerk instance's JWKS, checks `exp`, `nbf` and `iss`, and checks `azp` against the allowed origins (the web build's origin) ([manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification)).
- `sub` is the Clerk user ID, stored as `user.auth_id` (contracts 9, auth webhook).
- Clerk sends webhooks through Svix. The adapter verifies the `svix-id`, `svix-timestamp` and `svix-signature` headers before parsing, and maps `user.created` and `user.deleted` to `AuthWebhookEvent` ([Clerk webhooks](https://clerk.com/docs/guides/development/webhooks/overview), [Svix verification](https://docs.svix.com/receiving/verifying-payloads/how)).
- A display name in the session token would need a custom claim ([customising session tokens](https://clerk.com/docs/guides/sessions/customize-session-tokens), open decision 7).

---

## 9. API layer

### Generated types

```
 backend domain crate ──utoipa──▶ backend/openapi.json (committed)
                                         │ npm run gen:api (openapi-typescript)
                                         ▼
                               app/src/api/generated/schema.ts (committed)
                                         │ re-exported by name
                                         ▼
                               app/src/domain/types.ts ──▶ features import from here only
```

- CI regenerates both files and fails when either differs from what is committed, so a contract change can't reach one side only.
- The backend's spec is committed as `backend/openapi.json`, generated from the `domain` crate's types, which are the contracts' source of truth ([contracts 1](../architecture/contracts.md#source-of-truth)). `domain/types.ts` re-exports the generated types from the start; no contract type is written by hand.

### Request pipeline

```
 resources/scans.ts  scans.answer(id, body, key)
        │
        ▼
 openapi-fetch client (typed by paths)
        │
        ├─ 1. auth middleware ............ Authorization: Bearer <token>
        ├─ 2. idempotency middleware ..... Idempotency-Key: <key of this user action>
        ├─ 3. error middleware ........... non-2xx → ApiError(code, message, retryable, retry_after_s, details, request_id)
        ▼
 Transport.fetch  ── live: expo/fetch (native), window.fetch (web) │ mock: api/mock (section 15)
```

```ts
// src/api/transport.ts
// openapi-fetch calls fetch with a Request object ✓; expo/fetch accepts one ✓.
export interface Transport {
  fetch(input: string | Request, init?: RequestInit): Promise<Response>;
}

// src/api/resources/scans.ts (outline)
export const scans = {
  get: (id: Id) => client.GET("/scans/{id}", { params: { path: { id } } }),
  create: (body: CreateScanRequest, key: IdempotencyKey) => ...,
  uploadImage: (id: Id, part: ShotUpload, key: IdempotencyKey) => ...,   // multipart (section 11)
  submit: (id: Id) => ...,
  answer: (id: Id, body: AnswerRequest, key: IdempotencyKey) => ...,
};
export function useScan(id: Id) { return useQuery({ queryKey: qk.scan(id), queryFn: ... }); }
```

A response body that isn't an `ErrorBody` (a proxy's 502, for example) becomes `internal` with `retryable: true`.

**Lists.** Every list is paged, at most 100 items a page (contracts 1). Screens that show a list page through it as the user scrolls. Lists that must be complete, such as the plants for the map and for the map's "Which plant?" picker (a map holds up to 200 pins, contracts 9), follow `next_cursor` to the end through `api/pages.ts`.

Docs: [openapi-fetch API](https://openapi-ts.dev/openapi-fetch/api) · [auth middleware](https://openapi-ts.dev/openapi-fetch/middleware-auth).

### Error handling

One table maps every code in contracts 2 to its behaviour, in `api/errors.ts` and the `ui` error components, so every screen reacts the same way:

| Code | App behaviour |
|---|---|
| `invalid_request` | A form shows `details.problem` beside `details.field`. The map's `unknown_plant` follows [flows 11](flows.md#11-garden-mapping-session) |
| `unauthenticated` | Refresh the token once and retry, then sign out (section 8) |
| `forbidden` | Normally prevented by the role matrix (below). Shown as "Ask an owner to change your role", with `required_role` |
| `invalid_signature` | An image URL expired: refetch its record once for a fresh URL (signed image URLs, below) |
| `not_found` | A "not found" screen with a way home. Never "you don't have access": the API hides that on purpose |
| `invalid_state` | By its details: `hint: "new_scan"` offers a new scan. `current_plan_id` and `hint: "neighbour_removed"`: the chat card offers **Ask again** ([flows 10](flows.md#10-confirmation-cards-and-memories)). `hint: "plant_removed"`: the card says the plant was removed. `current_map_version`: **Reload** or **Replace** ([flows 11](flows.md#11-garden-mapping-session)). Otherwise the screen refetches its record |
| `request_in_progress` | Wait `retry_after_s`, then retry with the same key |
| `lease_lost` | Workers only; never reaches the app |
| `upload_too_large`, `unsupported_media_type` | A picked photo is re-encoded once (smaller JPEG) and uploaded as a **new action with a new key**, because the first response is stored under the old key (contracts 1). A LiDAR shot is never re-encoded (contracts 3): the message is shown with a retake |
| `image_quality` | The capture screen shows `message`, an icon for `details.hint` (`tap_to_focus`, `more_light`, `avoid_glare`, contracts 2), and asks for a retake |
| `missing_required_shot`, `too_many_images` | Shown on the shot list; normally prevented by the capture model |
| `idempotency_conflict` | A bug: one key was sent with two different bodies. Log `request_id`, drop the key and show a generic error |
| `rate_limited` | A banner; retried after `retry_after_s` |
| `budget_exceeded` | A banner that says when it resets (`resets_at`). `retryable: false`, so never retried |
| `internal` | `message` with `request_id` in small text |
| `processing_unavailable`, `upstream_unavailable` | Retried with backoff, waiting `retry_after_s` when sent |

**Retries.** TanStack Query retries `409 request_in_progress` and `429 rate_limited` after `retry_after_s`, and `5xx` responses with `retryable: true` with backoff (waiting `retry_after_s` on `503` when sent), at most 3 times. It never retries `budget_exceeded` or any other `4xx`. A retry always sends the same key and the **same bytes**, because the idempotency fingerprint covers the file bytes (contracts 1).

Docs: [TanStack retries](https://tanstack.com/query/latest/docs/framework/react/guides/query-retries) · [important defaults](https://tanstack.com/query/latest/docs/framework/react/guides/important-defaults).

### Idempotency keys

A key belongs to a **user action**, not to a request (contracts 1):

```
 user taps "Send answer" ──▶ key = randomUUID()  ──▶ POST (key) ──✗ network error
                                     │                     │
                                     │ same action ──────▶ POST (same key, same body) ──▶ 202
                                     ▼
                              key dropped on success or on a final failure
 user taps again later  ──▶ new key
 the body must change (re-encoded file, pruned map pins, resend) ──▶ new key
```

`useIdempotentMutation` makes the key when the user acts (`expo-crypto`), reuses it for every retry of that action and drops it when the action ends. Capture shots keep their key and their exact upload file (after any HEIC conversion) in the capture draft, so a shot retried after the app restarts sends the same bytes and doesn't upload twice. A chat "resend" is a new action with a new key, as the contract requires.

### Signed image URLs

Image URLs expire after 15 minutes (contracts 1). The app:

- caches images in `expo-image` under a key that survives re-signing (`cacheKey`): the image ID when the record has one (`ScanImage.id`, `latest_scan.thumbnail_image_id`, `TimelineEntry.thumbnail_image_id`, chat `image_id`), otherwise the URL's path without its query (masks, species photos). A fresh signature therefore doesn't download the photo again. A species photo's path carries a hash of its content (contracts 9, species), so a reseed that replaces a photo changes its key and the old one is never shown next to the new credit;
- gives scan and conversation queries a stale time of 10 minutes, under the expiry, so URLs are refreshed by refetching the record, as the contract says;
- on a load error (`403 invalid_signature`), invalidates the record's query once and loads the new URL.

The app can't build an image URL from an image ID: URLs are signed by the server ([storage 3](../storage/README.md#3-how-images-are-served)). Every image the app shows comes with a `url` on its record: `ScanImage.url`, `Plant.latest_scan.thumbnail_url`, `TimelineEntry.thumbnail_url`, `SpeciesCandidate.reference_image`, `SpeciesSummary.reference_images`, chat attachments and masks.

**URLs inside events** (`Region.mask_url` in a `finding`, `SpeciesCandidate.reference_image` in `species_suggested`) are signed when the event is sent or replayed, and species photos are filled in from `species_id` then (contracts 4), so they are fresh on arrival. A screen kept open longer than 15 minutes can still see one expire. The load-error rule then refetches the record that holds the same image, `qk.scan(id)` for masks and `qk.plant(plantId)` for species photos, and shows its URL instead.

### Query keys and invalidation

All keys come from `api/queryKeys.ts`, so invalidation rules can name them:

| Key | Endpoint | Invalidated by |
|---|---|---|
| `qk.me()` | `GET /me` | `PATCH /me` |
| `qk.gardens()`, `qk.garden(id)` | `GET /gardens`, `GET /gardens/{id}` | Garden create, edit, delete; invite accepted; leaving a garden |
| `qk.weather(gardenId)` | `GET /gardens/{id}/weather` | Stale after 30 minutes; refetched on focus. Garden edit (new location) |
| `qk.species(q)`, `qk.speciesItem(id)` | `GET /species`, `GET /species/{id}` | Never: the knowledge base changes only with a migration. Stale after 10 minutes for the signed URLs |
| `qk.members(gardenId)`, `qk.invites(gardenId)` | members, invites (owner) | Role change, removal, invite create or revoke |
| `qk.plants(gardenId)`, `qk.plant(id)` | `GET /plants`, `GET /plants/{id}` | Plant create, edit, delete; species confirmed; map upload (new plants); scan create, submit and delete, and `failed` (`latest_scan`); `plan_ready` and `plan_superseded` (the plant's status is set at `planned`, architecture 8.1). Stale after 10 minutes for the thumbnail URLs |
| `qk.timeline(plantId)` | `GET /plants/{id}/timeline` | `plan_ready`, `plan_superseded`, `failed`, scan create and delete. Stale after 10 minutes for the thumbnail URLs |
| `qk.measurements(plantId)` | `GET /plants/{id}/measurements` | `plan_ready`, `plan_superseded` |
| `qk.scan(id)` | `GET /scans/{id}` | Answers, feedback, confirmed `revise_plan`, `plan_ready`, `plan_superseded`, `guided_check_done` |
| `qk.tasks(gardenId, filter)` | `GET /tasks` | Task create, edit, delete; confirmed `create_task`, `complete_task` and `revise_plan`; `plan_ready`, `plan_superseded`; deleting a pending scan (its jar-test task goes with it) |
| `qk.conversations(gardenId, anchor?)`, `qk.conversation(id)` | conversations | A message completed, a conversation created |
| `qk.memories(gardenId, plantId?)` | `GET /memories` | Note added, memory shared or deleted, `memory_saved`, `plan_ready` (a pipeline memory is added, architecture 8.1) |
| `qk.map(gardenId)` | `GET /gardens/{id}/map` | Map upload |

**Deleting a plant or a garden** (`202`, cascade in the background, contracts 9) also invalidates the garden's tasks, conversations, memories and map, and removes the deleted records' queries.

Docs: [query keys](https://tanstack.com/query/latest/docs/framework/react/guides/query-keys) · [query invalidation](https://tanstack.com/query/latest/docs/framework/react/guides/query-invalidation).

### Roles in the UI

`domain/permissions.ts` is the role matrix of contracts 1 as data: `can(role, action)` for actions such as `scan.create`, `scan.delete_pending`, `scan.answer`, `scan.feedback`, `species.confirm`, `map.capture`, `task.write`, `plant.write`, `plant.delete`, `memory.note`, `memory.share`, `memory.delete_shared`, `invites.manage`, `members.manage`, `garden.manage`. `useCan(gardenId, action)` reads `Garden.my_role`. Actions a role can't use are hidden, not disabled. The server still enforces every rule; this only keeps viewers from seeing actions that would fail.

---

## 10. Live streams

Both streams are an HTTP request whose response body streams, read through the transport (architecture 9). `api/sse.ts` turns the bytes into events. It handles `id:`, `event:` and multi-line `data:` fields, `:` comments (the 15-second keepalive), `\r\n` line endings and events split across chunks. It is pure and tested on every chunk boundary.

```
 Response.body (ReadableStream<Uint8Array>)
        │ TextDecoder (stream: true)
        ▼
 line splitter ── "id: 17" ─────────┐
                  "event: finding" ─┤
                  "data: {...}" ────┤──▶ blank line ──▶ { id, event, data } ──▶ JSON.parse ──▶ ScanEvent
                  ": keepalive" ────┘      (dispatch)                        (v must be 1; unknown types dropped)
```

Docs: [the SSE format (WHATWG)](https://html.spec.whatwg.org/multipage/server-sent-events.html) · [MDN, using server-sent events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events) · [MDN Streams API](https://developer.mozilla.org/en-US/docs/Web/API/Streams_API).

### Scan events

`useScanEvents` in `features/scans` joins the snapshot and the stream:

1. Load `GET /scans/{id}` (`ScanDetail`) for the records: images, observations, diagnosis, plan, open question, guided checks and `last_event_seq`.
2. If the scan is at rest (below), stop: the records are the whole screen, and the stage list isn't shown.
3. Otherwise open `GET /scans/{id}/events` **without** `Last-Event-ID`, so the server replays the scan's whole history, a key-range read (architecture 5; contracts 4, transport: resume). `ScanDetail` carries no stage progress, no candidate count and no `needs_confirmation`, so only the history rebuilds them.
4. Fold each event into the pure reducer `applyScanEvent(state, event)`. Until the replay reaches `last_event_seq`, the screen shows the snapshot's records and doesn't animate the replayed events.
5. Run the side effects of live events, listed by the pure `effectsOf(event)`.
6. On a reconnect within the same visit, send `Last-Event-ID` with the last applied `seq`.

**The reducer** (contracts 4, event catalogue):

| Event | State change |
|---|---|
| any | Ignored when `seq` ≤ the last applied `seq`; unknown `type` ignored |
| `run_started` | `run` updated; the listed `rerun_stages` reset; findings whose `pipeline_stage` is among them removed |
| `status_changed` | `status` |
| `stage_started`, `stage_completed` | The stage's progress, with `skipped.reason` |
| `finding`, `finding_retracted` | Findings by observation ID |
| `weather_loaded`, `candidates_selected` | Weather strip; "Checking N possible problems" |
| `species_suggested` | The species card: one tap to confirm when `needs_confirmation` is false, the top 3 and "Something else" when it is true |
| `species_confirmed` | Hides the card |
| `question`, `question_resolved` | The question card |
| `guided_check_done` | The check's result or failure |
| `diagnosis` | The current diagnosis |
| `plan_ready`, `plan_superseded` | The plan ID and `provisional`; a "plan updated" notice for a supersede |
| `failed` | `failure`, with **Retry** when `retryable`, otherwise **New scan**, both for editors and owners |

**Its effects:**

| Event | Effects |
|---|---|
| `plan_ready`, `plan_superseded` | Invalidate `qk.scan(id)`, `qk.plant(plantId)`, `qk.plants(gardenId)`, `qk.timeline(plantId)`, `qk.measurements(plantId)`, `qk.tasks(gardenId)`, `qk.memories(gardenId)` |
| `species_confirmed` | Invalidate `qk.plant(plantId)`, `qk.plants(gardenId)` |
| `guided_check_done` | Invalidate `qk.scan(id)` |
| `failed` | Invalidate `qk.timeline(plantId)`, `qk.plant(plantId)`, `qk.plants(gardenId)` (`latest_scan.status`) |

The reducer is tested against the example run in contracts 4, a species-change rerun, a retracted finding, a late answer after `planned`, a full replay, and a replay that repeats `seq` values.

**The resting rule.** `domain/scan.ts` holds the same rule as the server's close table (contracts 4, when the stream closes). A scan is at rest when no guided check is `analysing`, and it is either `failed`, or `planned` with no `open_question` (open or `answering`). A `failed` scan never holds a question: failing it supersedes the question first (contracts 3), so the reducer sees `question_resolved` before `failed`, and both rules agree.

**Connection rules.** The state machine is in [flows.md](flows.md#17-scan-event-stream).

- When the server closes a stream, the app refetches `GET /scans/{id}`. If the record is at rest, the stream stays closed. Otherwise, after a network error or a close the record doesn't explain, the app reconnects with backoff (1, 2, 4 … 30 s) and `Last-Event-ID`.
- No bytes for 45 seconds, three missed keepalives, counts as a broken stream.
- The stream closes when the app goes to the background (or the browser tab is hidden) and reopens on return, from the last applied `seq`. In between, push carries the main changes (a plan, a new plan version, a question, a failure) to the scan's creator on native builds ([contracts 9](../architecture/contracts.md#push-notifications)); everyone else sees them on the next read.
- A scan at rest gets its stream back when the user starts late work on it (an answer, a question photo, a guided check, a species change, a retry), when a push arrives for it, or when its screen is focused again and the refetched record isn't at rest.
- Only the screen that shows a scan holds its stream. Other screens read records and refetch on focus.

### Chat messages

`POST /conversations/{id}/messages` answers with a stream that can't be resumed (contracts 8). `features/chat/model/messageReducer.ts` builds the assistant message from `message_started`, `text_delta`, `tool_started` / `tool_finished` (small "Checking the forecast" chips, from `label`), `citation`, `pending_action`, `memory_saved`, `rescan_requested`, `message_completed` and `error`.

- The user's message shows at once and is replaced by the stored one when the stream completes.
- **A send checks the response's `Content-Type`.** `text/event-stream` goes to `messageReducer`. `application/json` is an idempotent replay of a message that already finished (`200 { message }`, contracts 1), taken as complete.
- A send that fails before any response (network error) is retried with the **same** key, which may return that replay.
- If the stream breaks after it started, the message shows as interrupted with **Resend**, a new action with a new key.
- Errors before the stream starts are ordinary `ApiError`s (a `budget_exceeded` banner, for example). An `error` event ends the message with its text.
- `rescan_requested` shows **Take a new photo** only when `useCan(gardenId, "scan.create")`. `request_rescan` is offered to every role (contracts 8), so a viewer or reviewer sees the instructions as text only.

The sequence is in [flows.md](flows.md#9-chat-turn).

---

## 11. Feature modules

Each feature's `model/` is the logic that lives outside React and is tested on its own.

| Feature | Screens and parts | Model (pure) | Endpoints |
|---|---|---|---|
| `auth` | Sign-in, sign-up, verify, onboarding | Sign-in step, onboarding step, which sign-in methods the browser allows | Clerk; `GET` / `PATCH /me` |
| `gardens` | Garden home, create and edit, location consent, garden switcher, garden notes, weather strip | Location input from consent, a postal code or a city | `/gardens`, `GET /gardens/{id}/weather`, `GET /memories?garden_id=` |
| `members` | Members, role change, removal and leaving, invites with the share sheet, invite acceptance | Who may leave or be demoted (never the last owner) | `/gardens/{id}/members`, `/gardens/{id}/invites` (owner), `/invites/{code}/accept` |
| `plants` | Plant list, new plant sheet, plant detail, timeline, growth curve, species confirmation card, species picker, plant notes | Growth series to chart points | `/plants`, `/plants/{id}/timeline`, `/plants/{id}/measurements`, `/plants/{id}/species`, `/species`, `/memories` |
| `capture` | Guided capture, question photo, guided soil checks | Shot plan and upload state machine | `/scans`, `/scans/{id}/images`, `/scans/{id}/submit`, `DELETE /scans/{id}` (discard), `POST /tasks` with `scan_id` (jar test) |
| `scans` | Live progress, photo with regions, findings, diagnosis, question card, plan checklist, scan feedback | `applyScanEvent`, `effectsOf`, the resting rule, plan step display | `/scans/{id}`, `/scans/{id}/events`, `/scans/{id}/answers`, `/scans/{id}/feedback` |
| `tasks` | Task list, alert inbox, task editor | Grouping by due date in the garden's time zone; which fields each source may edit | `/tasks` |
| `chat` | Conversation list, conversation, composer with photos, cards, remembered facts, message feedback | `messageReducer`, pending-action card state | `/conversations`, `/messages/{id}/feedback`, `/pending-actions`, `/memories` |
| `map` | Map view, mapping session | Map draft and upload conflicts | `/gardens/{id}/map`, `GET /plants` (all pages) |
| `account` | Profile, notifications, account deletion | | `/me`, `/me/push-tokens` |

### Capture

The capture model is a state machine over **shot slots**, built from the kind limits in contracts 3 (diagram in [flows.md](flows.md#14-capture-slot)):

| Slot | Required | Shown |
|---|---|---|
| `whole_plant` | Yes | Always, first |
| `leaf_closeup` × 2 | No | "Something looks wrong?" |
| `leaf_underside` | No | Suggested, with the pest hint |
| `soil_surface` | No | Suggested |
| `flower_fruit` | No | "If it has flowers or fruit" |

- **Each shot uploads as soon as it is taken**, so a blurry shot is caught before the next one (architecture 6.6). Replacing a stored shot deletes it first. **Submit** is enabled when `whole_plant` is stored and nothing is uploading.
- **The draft** (scan ID, slots, the exact upload file for each, idempotency keys) is persisted, so a capture interrupted by the app closing resumes on the still-`pending` scan. **Discard** deletes the scan (`DELETE /scans/{id}`, contracts 3) and the draft. When a jar test was started on it, the confirmation says its task is deleted too, which the server does with the scan.
- **Uploads** build the multipart body from `expo-file-system` `File` objects, which `expo/fetch` accepts ✓; browsers use the picker's web `File`. A LiDAR shot adds the `depth`, `depth_confidence` and `depth_meta` parts from the module, and the photo is the module's JPEG, unchanged. `depth.stored: false` in the response is a small note, never an error: the photo is kept (contracts 3).
- **Picked photos** in HEIC are converted to JPEG before the first upload attempt (contracts open question 8), and the converted file is what the draft keeps. In browsers the conversion needs a browser that decodes HEIC; where it can't, the original is sent and an `unsupported_media_type` answer is shown with "Choose a JPEG or take the photo again" (*unverified*: what each phone browser hands over).
- **Distance hints** on LiDAR devices come from the module's distance events and the ranges per kind in `capture/model/distanceRanges.ts`: 0.5–3 m for the whole plant, 25–45 cm for leaf shots, 30–80 cm for soil and fruit (architecture 6.7). They move with the day-1 check.
- **Question photos** reuse the capture view with one slot of the question's `photo_kind`, uploaded with `question_id` and no submit.
- **Guided soil checks** show the squeeze or jar instructions, then one shot. Starting a jar test creates a task to photograph the jar later (`POST /tasks` with `scan_id`, contracts 9), so the task opens the jar capture for the right scan on any device. A jar that hadn't settled (`settled: false`, contracts 6) gets no texture result, and the task stays open.

### Scan results

The scan screen renders the records and the reducer's state, top to bottom: status and stage progress; the photo strip with region boxes and masks, a finding's region highlighted on tap; the species card while the plant's species is `suggested`; the question card; the diagnosis, worded from its `status` and `low_evidence`, with matched, contradicted and unsure indicators, a "measured" badge on indicators answered by measurement, risks, the evidence basis and citations; organisms by role (beneficial ones with what they eat, from `preys_on`, invasive ones with "How to report"); soil findings as a list, never a score (architecture 6.4); measurements with units and their confidence; the weather strip; the plan; and scan feedback for reviewers and up. Layout in [screens.md](screens.md#33-scan-results).

**The diagnosis has no confidence of its own.** `Diagnosis` and `CandidateScore` carry a `status`, `low_evidence` and a `score`, never a `confidence_band` (contracts 7). `domain/diagnosis.ts` words the result from `status` and `low_evidence` ([screens.md 6](screens.md#6-wording-for-uncertain-results)). With `low_evidence` the screen names no problem: it asks for a closer photo and suggests a local extension office (architecture 6.1). **Take a closer photo** answers the open `photo_request` question when there is one (`capture/[scanId]/question/[questionId]`), which keeps the scan's history and re-plans it as a late answer; without an open `photo_request`, even when another question is open, it starts a new scan. Confidence bands appear only on findings and measurements, which have them.

**The plan** is a checklist of steps with their timing (`now`, `by_date`, `recurring`, `on_trigger`), category, citations and the server's safety note on chemical steps. A provisional plan has a banner and the open question at the top. A neighbour step whose plant no longer exists shows "plant removed" and no task (contracts 7). "Ask about this result" opens a chat anchored to the scan.

### Chat cards

A `PendingAction` card shows the server's `card` text and the preview: the task, or the plan changes (changed and removed steps, added constraints) with "This changes the plan for everyone in the garden". Confirm and reject call `/pending-actions/{id}/confirm` and `/reject`. The `409` cases are in [flows 10](flows.md#10-confirmation-cards-and-memories). Cards appear only for editors and owners, because only they are offered write tools. Assistant messages have thumbs up and down (`POST /messages/{id}/feedback`) with an optional note.

### Garden map

- **Viewing** works on every device: an SVG plan of the beds and pins, pins coloured by plant status, north shown when `north_deg` is set, pan and zoom, a tap opens the plant.
- **Mapping** (LiDAR devices, editors) opens the module's `GardenMappingView`. The map model holds the draft: beds from tapped corners, pins with their plant or a new plant, and the `base_version` read when the session started. The `409` and `400 unknown_plant` paths are in [flows.md](flows.md#11-garden-mapping-session). An edit is offered only when the module's saved `ARWorldMap` is tagged with the current version.

---

## 12. State ownership

```
┌──────────────────────── server is the source of truth ─────────────────────────┐
│ TanStack Query cache                                                            │
│ gardens · members · plants · timeline · measurements · scans · tasks ·          │
│ conversations · memories · map · me                                             │
└────────────────────────────────────────────────────────────────────────────────┘
┌──────────── screen-local, rebuilt on open ────────────┐ ┌──── device-local ─────┐
│ scan reducer state (useScanEvents)                    │ │ zustand (persisted)   │
│ streaming chat message (messageReducer)               │ │ activeGardenId        │
│ form state (react-hook-form)                          │ │ capture drafts        │
│ map draft (zustand, in memory)                        │ │ preferences           │
└───────────────────────────────────────────────────────┘ │                       │
                                                          │ Clerk session         │
                                                          │ (expo-secure-store)   │
                                                          └───────────────────────┘
```

| State | Where | Why |
|---|---|---|
| Records from the API | TanStack Query cache | The server is the source of truth; refetched and invalidated |
| A scan's live progress | Scan reducer state in `useScanEvents` | Built from the snapshot plus the event history; thrown away when the screen closes |
| A streaming chat message | `messageReducer` state in the conversation screen | Not resumable, so not cached |
| The active garden | zustand, persisted to AsyncStorage | Survives restarts; checked against `GET /gardens` on start |
| Capture drafts | zustand, persisted | A capture survives the app closing |
| Map draft | zustand, in memory | Lost with the AR session anyway |
| UI preferences | zustand, persisted | Theme, dismissed tips |
| Session | Clerk, in `expo-secure-store` on phones | The only secret |

Nothing else is persisted, and signing out clears all of it.

---

## 13. Platform adapters and the LiDAR module

### Capabilities by target

| | iOS dev build, LiDAR device | iOS dev build | Android dev build | Expo Go | Phone browser | Desktop browser |
|---|---|---|---|---|---|---|
| Sign-in: email and password | yes | yes | yes | yes | yes | yes |
| Sign-in: Google, Apple | yes | yes | yes | yes | yes, except in embedded web views (`in_app`) | yes |
| Capture | LiDAR module | `expo-camera` | `expo-camera` | `expo-camera` | System camera (`launchCameraAsync`), or the library | File picker |
| Depth and distance hints | yes | | | | | |
| Garden mapping | yes | | | | | |
| Map view, growth curve, measurements | yes | yes | yes | yes | yes | yes |
| Push | yes | yes | yes | | | |
| Alert inbox | yes | yes | yes | yes | yes | yes |

`platform/capabilities.ts` answers these once at start-up, through `useCapabilities()`. Features ask it; they never check the platform.

### Phone browsers and in-app browsers

The live link is often opened on a phone, in Safari or Chrome, or from inside another app. Two kinds of in-app browser behave differently:

- **The system's in-app view.** Slack, Gmail and many other apps open links in [`SFSafariViewController`](https://developer.apple.com/documentation/safariservices/sfsafariviewcontroller) on iOS or [Custom Tabs](https://developer.chrome.com/docs/android/custom-tabs) on Android. These are the system browser's engine, with its user agent and camera access, and Google allows OAuth in them. Custom Tabs shares the browser's cookies; `SFSafariViewController` keeps its own since iOS 11, so a user signed in to Safari signs in again there. The app treats them as `mobile`.
- **An embedded web view.** Instagram, Facebook, LinkedIn and TikTok render links in their own web view. Google refuses OAuth there, and camera and file access can be limited. The app treats these as `in_app`.

| Concern | What the web build does |
|---|---|
| Telling them apart | `platform/browser.ts` reports `native`, `desktop`, `mobile` or `in_app` (`native` outside the web build). `mobile` comes from the pointer and screen media queries. `in_app` comes from the user-agent markers that embedded web views add (for example `Instagram`, `FBAN`/`FBAV`, `LinkedInApp`); the system's in-app views add none, so they read as `mobile`, which is right (*unverified*: the marker list lives in one file and is checked on the demo phones) |
| Capture on a phone | **Take photo** calls `expo-image-picker`'s `launchCameraAsync` directly inside the button's press handler, which mobile browsers require ([expo-image-picker](https://docs.expo.dev/versions/v57.0.0/sdk/imagepicker/)). It opens the phone's own camera at full resolution, and its result carries a web `File` that is uploaded as it is. The capture screen shows the slot's instructions before the camera opens, since there is no overlay on the system camera. **Choose from library** sits beside it. The [`capture`](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Attributes/capture) attribute of the [file input](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/input/file) is what the browser uses underneath |
| Capture on a desktop | A file picker, and drag and drop onto the slot |
| Sign-in in an embedded web view (`in_app`) | Email and password only, with a banner: "For Google or Apple sign-in, open this page in Safari or Chrome", and a **Copy link** button. Google blocks OAuth in embedded browsers ([policy](https://developers.google.com/identity/protocols/oauth2/policies)) |
| Camera in an embedded web view | Tried as above; if the picker returns nothing, the screen suggests opening the page in Safari or Chrome |
| HTTPS | The web build is served only over HTTPS, which Caddy provides (architecture 12). The system-camera file input works without it, but the [Geolocation API](https://developer.mozilla.org/en-US/docs/Web/API/Geolocation_API) used for the garden's location and the [Clipboard API](https://developer.mozilla.org/en-US/docs/Web/API/Clipboard_API) behind **Copy link** need a [secure context](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts) |
| Background tabs | A hidden tab is treated like a backgrounded app: the scan stream closes and resumes from the last applied `seq` when the tab is visible again (section 10) |
| Home screen | A web manifest (`display: standalone`, icons, theme colour) lets a judge add the app to the home screen and open it full screen ([Expo PWA guide](https://docs.expo.dev/guides/progressive-web-apps/), [MDN manifest](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest)) |
| Layout | Phone-width layouts are the same as native. Safe areas and the browser's toolbars are respected with `react-native-safe-area-context`, and the composer stays above the on-screen keyboard |

### Adapters

```
 features/capture ──▶ CaptureSource ──┬─▶ LidarCaptureSource    (modules/lidar)               iOS, LiDAR
                                      ├─▶ CameraCaptureSource   (expo-camera)                 iOS, Android, Expo Go
                                      ├─▶ SystemCameraSource    (expo-image-picker, camera)   phone browsers
                                      └─▶ PickerCaptureSource   (expo-image-picker, library)  desktop browsers, "choose from library"
 features/map     ──▶ lidar.GardenMappingView                                                  iOS, LiDAR
 features/auth    ──▶ browser   ──▶ native · desktop · mobile · in_app
 features/gardens ──▶ location  ──▶ expo-location (native and browser)
 features/account ──▶ push      ──▶ expo-notifications │ web: null
 features/capture ──▶ files     ──▶ expo-file-system File │ web: File from the picker
```

| Adapter | Interface | Implementations |
|---|---|---|
| `platform/capture` | `CaptureSource`: a view and `capture(): Promise<CapturedShot>` | LiDAR module, `expo-camera`, the system camera, the library picker |
| `platform/lidar` | The module's JS surface (below) | Native module, or a fallback with no capabilities |
| `platform/browser` | `kind(): "native" \| "desktop" \| "mobile" \| "in_app"` | Media queries and user agent on the web; `"native"` elsewhere |
| `platform/location` | `requestGardenLocation(): Promise<GardenLocationInput \| null>` | `expo-location` |
| `platform/push` | `register(): Promise<{ token, platform } \| null>`; `onOpen(handler)` for a tapped notification; `onReceive(handler)` for one that arrives while the app is open; `setForegroundFilter(fn: (payload) => boolean)`, which decides whether an arriving notification is shown | `expo-notifications` (`addNotificationResponseReceivedListener`, `addNotificationReceivedListener`, `setNotificationHandler`); no-ops on the web. Handlers receive contracts 9's `PushPayload`; `features/account` maps it to a route and hides a `scan` notification for the scan on screen ([flows 12](flows.md#12-push-notifications-and-deep-links)) |
| `platform/files` | `toUploadPart(shot)` | `File` from `expo-file-system` on native, the picker's `File` on the web |
| `platform/storage` | `get`, `set`, `remove` | AsyncStorage |
| `platform/haptics` | `success()`, `warning()` | `expo-haptics`; no-op on the web |
| `platform/linking` | `parse(url)` for invite and notification links | `expo-linking` |

```ts
// src/platform/capture/types.ts
interface CapturedShot {
  file: {
    uri: string;
    mimeType: "image/jpeg" | "image/png" | "image/webp" | "image/heic";
    width: number;
    height: number;
    webFile?: File;                 // browsers: the picker's File, uploaded as it is
  };
  depth?: {                         // LiDAR shots only
    depthUri: string;               // 16-bit PNG in millimetres, sensor frame (contracts 3)
    confidenceUri?: string;         // ARKit path only
    meta: DepthMeta;                // contracts 3
  };
}
```

### The LiDAR module's JavaScript surface

The native side is designed in architecture 6.7 and 6.8. This is the interface the app codes against, in `app/modules/lidar/src/index.ts`, so the app and the Swift work proceed in parallel:

```ts
export function getCapabilities(): { depth: boolean; mapping: boolean };

// Camera view with depth. Replaces expo-camera on LiDAR devices: one capture session at a time.
export const LidarCameraView: React.ComponentType<{
  active: boolean;
  onDistance?: (e: { distance_m: number | null }) => void;   // median depth at the centre of the frame
}>;
export function capturePhoto(view: LidarCameraViewRef): Promise<CapturedShot>;

// AR mapping session.
export const GardenMappingView: React.ComponentType<{
  onTrackingState?: (e: { state: "normal" | "limited" | "lost" }) => void;
  onGroundTap?: (e: { x_m: number; y_m: number }) => void;            // bed corners
  onPinPlaced?: (e: { x_m: number; y_m: number; canopy_radius_m?: number }) => void;
  onHeading?: (e: { north_deg: number; accuracy_deg: number }) => void;
}>;
export function saveWorldMap(version: number): Promise<void>;          // stays on the phone
export function savedWorldMapVersion(): Promise<number | null>;
export function relocalise(): Promise<"ok" | "failed">;
```

Where the native module is absent (Android, the web, Expo Go), `requireOptionalNativeModule` returns `null` and the fallback reports `{ depth: false, mapping: false }`.

Docs: [Expo Modules API](https://docs.expo.dev/modules/module-api/) · [`AVDepthData`](https://developer.apple.com/documentation/avfoundation/avdepthdata) · [`ARWorldTrackingConfiguration`](https://developer.apple.com/documentation/arkit/arworldtrackingconfiguration) · [`ARWorldMap`](https://developer.apple.com/documentation/arkit/arworldmap) · [Core Motion](https://developer.apple.com/documentation/coremotion).

---

## 14. UI system

```
 tokens (colour roles, spacing, radii, type) ──▶ theme (light, dark) ──▶ useTheme()
                                                                            │
 primitives: Text · Button · IconButton · Card · Chip · Badge · Banner ·    │
             Sheet · TextField · ListItem · Skeleton · EmptyState · ErrorState ◀┘
                                                                            │
 composites: ConfidenceBadge · StatusChip · StageProgress · PhotoWithRegions ·
             LineChart · CitationList · QuestionCard shell · ConfirmCard shell
                                                                            │
 feature components (props only) ◀──────────────────────────────────────────┘
```

- **Tokens:** colour roles (surface, text, accent, and status colours for healthy, watch, sick and dead plants), spacing on a 4-point scale, radii and type sizes. Light and dark share role names. Components read roles through `useTheme()`, never raw colours.
- **`PhotoWithRegions`** draws normalised boxes (contracts 1, regions) and mask PNGs over an `expo-image`, converting with `domain/regions.ts` from the image's stored size to the view's size.
- **`ConfidenceBadge`** is used only where a record has a `confidence_band`: findings and measurements. The diagnosis uses its status wording instead (section 11).
- **Accessibility:** every control has a label, touch targets are at least 44 points, confidence and status are written as words as well as colours, and text scales with the system setting ([WCAG 2.2 quick reference](https://www.w3.org/WAI/WCAG22/quickref/)).
- **Copy:** English only for now, kept with each feature's components. Server text (`ErrorBody.message`, card text, plan text) is shown as sent.

The screens that use them are in [screens.md](screens.md).

---

## 15. Mock mode

`EXPO_PUBLIC_API_MODE=mock` swaps the live transport for `api/mock`, so the app runs with no backend and no network:

```
 features ─▶ api client ─▶ Transport
                              ├── live: expo/fetch ─▶ API
                              └── mock: router ─┬─▶ fixture store (in memory, typed fixtures)
                                                ├─▶ scripted scan runs ─▶ SSE byte stream with delays
                                                ├─▶ scripted chat turn ─▶ SSE byte stream
                                                └─▶ fault injection (debug menu)
```

- **The fixture store** answers the REST endpoints: a garden with members of each role and its weather, plants with thumbnails, timelines and growth points, species with reference photos, tasks and alerts, a garden map, conversations and memories.
- **Scripted scan runs:** the early-blight run of contracts 4; a run that asks a question and continues without an answer; a healthy plant; a `low_evidence` result; `no_plant_found`; a species change that reruns; a LiDAR scan with measurements and measured indicators; a guided jar test that finishes after the plan; a beneficial organism with its prey.
- **Push payloads:** the debug menu can fire each `PushPayload` kind at the app, to exercise the routing without a push service.
- **A scripted chat turn** streams text, tool chips, a citation, a confirmation card and a remembered fact, and can answer a retried send with the JSON replay.
- **Fault injection:** a debug menu makes the next upload fail with `image_quality` or `unsupported_media_type`, or the next call with `budget_exceeded`, `invalid_state` or a dropped stream, to exercise each row of the error table.

The fixtures use the same types as the live client, so a contract change that breaks them fails type checking. Mock mode is also how component tests get their data and how the UI is demoed when the backend is down.

---

## 16. Testing and tooling

| Check | What | Where |
|---|---|---|
| Types | `tsc --noEmit`, strict | CI, pre-push |
| Lint | `eslint-config-expo`, `import/no-restricted-paths` for section 5, no deep imports into features | CI |
| Generated types | `gen:api` output matches the committed `schema.ts` | CI |
| Expo versions | `npx expo install --check` | CI |
| Model tests | Jest on every `model/`: the scan reducer against contract examples and a full replay, the resting rule, the capture machine, the message reducer, the map draft, the SSE parser on split chunks, `permissions.ts` against the role matrix, `diagnosis.ts` wording | CI |
| API tests | Middleware against the mock transport: token refresh on `401`, key reuse with identical bytes on retry and a new key when the body changes, the retry policy (`budget_exceeded` never retried), `ErrorBody` parsing, the chat JSON replay | CI |
| Component tests | React Native Testing Library on the cards with fixtures: question, diagnosis, plan checklist, confirmation card, capture slots | CI |
| Device checks | A short manual script per build-plan day on a LiDAR iPhone, an Android phone, Safari and Chrome on phones, a link opened from Slack (system in-app view) and from Instagram (embedded web view), and a desktop browser | Before each demo build |

---

## 17. Builds, environments and delivery

| Variable | Example | Notes |
|---|---|---|
| `EXPO_PUBLIC_CLERK_PUBLISHABLE_KEY` | `pk_test_…` | Public by design; still kept in `.env`, out of git |
| `EXPO_PUBLIC_API_URL` | `https://api.example.org` | No trailing slash |
| `EXPO_PUBLIC_API_MODE` | `live` or `mock` | Section 15 |

`EXPO_PUBLIC_*` values are compiled into the app and readable by anyone who has it, so nothing secret goes in one ([environment variables](https://docs.expo.dev/guides/environment-variables/)). `app/.env.example` lists them with placeholders.

```
                        ┌─▶ Expo Go ─────────────────── UI work, mock mode (no LiDAR, no push)
 app/ ── npx expo ──────┤
                        └─▶ web export (static files) ─▶ hosted on the control plane ─▶ live link
 app/ ── EAS build ─────┬─▶ development (expo-dev-client) ─▶ LiDAR iPhone, Android: module, push
                        └─▶ preview (internal distribution) ─▶ teammates' phones
```

| Build | Used for | How |
|---|---|---|
| Expo Go | Fast UI work early on, mock mode | `npx expo start` |
| Development build | LiDAR module, push, device testing | EAS `development` profile with `expo-dev-client`; the LiDAR iPhone registered for ad-hoc installs ([development builds](https://docs.expo.dev/develop/development-builds/introduction/)) |
| Web build | The live link (architecture 12) | `npx expo export -p web` produces static files. Where they are served from is open decision 5 ([Expo for web](https://docs.expo.dev/workflow/web/)) |
| Preview build | Teammates' phones | EAS `preview` profile ([EAS Build](https://docs.expo.dev/build/introduction/), [internal distribution](https://docs.expo.dev/build/internal-distribution/)) |

If the web build has its own origin, the API must answer CORS for it, allowing the request headers `Authorization`, `Content-Type`, `Idempotency-Key` and `Last-Event-ID`, since every API call sends `Authorization` and JSON bodies use a non-simple `Content-Type`, so every call is [preflighted](https://developer.mozilla.org/en-US/docs/Glossary/Preflight_request) ([MDN CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CORS); open decision 5).

---

## 18. Build plan

The frontend track of [architecture 14](../architecture/README.md#14-build-plan). The [roadmap](roadmap.md) holds each slice's work, dependencies and exit checks; this is its calendar. The app is built for the iPhone first, in the [Conservatory](../design/design.md#31-conservatory) direction, and the web build comes last.

| Dates | Frontend goal |
|---|---|
| Oct 20 | F0: the project, the layers, the api layer, mock mode with the demo story, Clerk, Conservatory's design layer, the app's CI job, the day-one spikes. F5 starts: the LiDAR module, the LiDAR day-1 check and the `GardenView` spike on the iPhone |
| Oct 20–21 | F1: the garden home and the new-scan sheet. F2: capture after the Camera app. F5: depth capture and the evaluation captures for vision by Oct 21 |
| Oct 21–22 | F3: the live analysis. F4: the tinted diagnosis, the plan with its mini player, step by step, plant detail and the Tasks tab |
| Oct 22–23 | F6: chat with answer blocks. F7: the 3D garden and the 2D map, Insights, members and invites, farm connectors, Google and Apple sign-in, account deletion. F5: `GardenView`, the mapping session, the Live Activity |
| Oct 24 | F8: feature freeze, design and guideline review, accessibility, the web origin settled, the demo rehearsed and recorded |
| Oct 25 | F9: the web build deployed as the live link |

---

## 19. Open decisions

1. **Expo SDK 58.** Move once `@clerk/expo`'s `peerDependencies` include it. Until then stay on 57.
2. **Styling.** Tokens with `StyleSheet` are proposed. NativeWind or Unistyles would add a build-time or native dependency; choose one only if the team prefers utility classes.
3. **Clerk screens.** Custom screens with Clerk's hooks are proposed, because they match the app's design and work on every target. Clerk's hosted pages (`useHostedAuth`) or its native components (`AuthView`, development builds only) are the alternatives.
4. **Sign-in methods.** Email and password, Google and Apple are proposed. Apple sign-in is only required for an App Store release, which build week doesn't include.
5. **Web build origin.** A subdomain beside the API (needs CORS) or the same origin under a path (doesn't). Decide with open decision 2 of the architecture document.
6. **Upload progress.** `expo/fetch` has no upload progress events. For shots without depth, `expo-file-system`'s `File.createUploadTask` sends one file with string fields as multipart and reports progress (*unverified* beyond the package's types). It doesn't fit LiDAR shots, which have three file parts. Use it only if the day-1 check shows uploads are slow enough to need a percentage.
7. **Display name.** Sent by the app from Clerk's profile (proposed), or added to Clerk's session token as a custom claim so the API reads it directly.
8. **Abandoned scans.** Decided: **Discard** deletes the pending scan (`DELETE /scans/{id}`). It still counts toward the daily scan limit, and a plant it created stays (contracts 3). Whether discarding should also offer to delete that new plant is open; only owners can delete plants.
9. **Browser capture.** The system camera (`launchCameraAsync`) is proposed for phone browsers because it takes a full-resolution photo. `expo-camera`'s web view would allow the same framing overlay as native, but its photo is a frame from the video stream. Compare the two on the demo phones on day 1.
10. **Web push.** The contracts accept `platform: "web"` push tokens and store them unused (contracts 9), because Expo Push serves only iOS and Android. Browser push would need a separate Web Push sender in the API, and on iOS works only for a page added to the home screen ([WebKit](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/)). Until then the alert inbox covers browsers.

---

## 20. Contract changes the screens need

Writing the screens showed data they needed that the contracts didn't carry. [Contracts](../architecture/contracts.md) v0.8 added all of it, and v0.8.1 and v0.8.2 tightened it, with the stored shapes in [`schema.surql`](../../backend/migrations/schema.surql). The schema changes and the new storage policies were run on the pinned SurrealDB 3.3.4 and RustFS 1.0.1 ([database 3, 4](../database/README.md#3-tables), [storage 2](../storage/README.md#2-bucket-keys-and-access)).

| # | What was missing | Contract change (v0.8 / v0.8.1 / v0.8.2) | Used in |
|---|---|---|---|
| 1 | **Thumbnails.** `TimelineEntry` had only `thumbnail_image_id`, `Plant` had no image, and URLs are server-signed | `TimelineEntry.thumbnail_url`; `Plant.latest_scan` with `scan_id`, `created_at`, `status`, `thumbnail_image_id` and `thumbnail_url` (contracts 3, 9) | Garden home plant cards, plant timeline ([screens 3.1](screens.md#31-garden-home), [3.5](screens.md#35-plant-detail-and-timeline)) |
| 2 | **Garden weather.** `WeatherSummary` existed only on `ScanDetail` | `GET /gardens/{id}/weather`, from the scheduler's cell cache (contracts 9) | Garden home weather strip |
| 3 | **A jar-test task's scan.** Contracts 3 had the jar photo uploaded "from the task the app created", but `Task` had no scan link | `Task.scan_id`, set by `POST /tasks`, which refuses a missing or deleting scan (contracts 7, 9; `task.scan` in the schema) | Guided soil checks (section 11, capture) |
| 4 | **Push payload** | `PushPayload`, its recipients and when each is sent (contracts 9, push notifications) | [Flows 12](flows.md#12-push-notifications-and-deep-links) |
| 5 | **Species list and reference photos.** Architecture 6.2 promised reference photos, and `species_id` inputs need slugs | `GET /species`, `GET /species/{id}`, `SpeciesSummary.reference_images`, `SpeciesCandidate.reference_image` with its credit (contracts 3, 9); reference images in the seed format and under `knowledge/` in RustFS, keyed and routed by a content hash (`GET /species/{id}/images/{hash}`; contracts 10, storage 2) | Species card and picker ([flows 7](flows.md#7-species-confirmation-and-reruns)) |
| 6 | **What a beneficial organism eats.** Architecture 6.5 says "they eat aphids" from `preys_on` | `Diagnosis.organisms[].preys_on` (contracts 7) | Scan results, organisms |
| 7 | **Low confidence overall.** Architecture 6.1 asks for a better photo when confidence is low, but the diagnosis had no confidence | `Diagnosis.low_evidence` (contracts 7; schema) | Diagnosis wording ([screens 6](screens.md#6-wording-for-uncertain-results)) |
| 8 | **Stream close while a guided check runs**, and what opening a resting scan's stream sends | The stream stays open until a running check finishes; a resting scan replays and closes (contracts 4) | The resting rule (section 10) |
| 9 | **`image_quality` hints.** Only `tap_to_focus` appeared, in an example | `QualityHint`: `tap_to_focus`, `more_light`, `avoid_glare` (contracts 2) | Capture retake banner |
| 10 | **City as a location.** Architecture 8.5 says "postal code or city" | `GardenLocationInput.city` and the precision `city` (contracts 9; schema) | Onboarding, garden edit |
| 11 | **`retryable` on `429`.** Not stated | `rate_limited` is retryable, `budget_exceeded` isn't (contracts 2) | Retry policy (section 9) |
| 12 | **Pending scans couldn't be deleted** | `DELETE /scans/{id}` for `pending` scans, with `scan.deleting_at` (contracts 3, 9; database 4) | Capture **Discard** |
| 13 | **Clerk as the provider** | Clerk and Svix in contracts 1 and 9; architecture open decision 1 decided | Section 8 |
| 14 | **Why a scan failed, on the timeline** | `TimelineEntry.failure_code` (contracts 9, v0.8.1) | Plant timeline |

The audit of v0.8 then tightened it (v0.8.1):
- URLs in events are signed each time the event is sent;
- the stream stays open for any running guided check, also after a failure;
- `low_evidence` covers only results where nothing points to a problem;
- feedback waits until a scan is submitted;
- deleting a pending scan also deletes its jar-test task;
- the knowledge photo upload runs on the control-plane host.

The third audit added (v0.8.2):
- failing a scan supersedes its open question, and a guided check that finishes on a failed scan only stores its result, for a resubmit;
- `POST /tasks` refuses a `scan_id` that is missing or being deleted;
- species photos are computed from `species_id` when read or sent, and keyed by a content hash, so a reseed never shows an old photo with a new credit.

## 21. Sources

Every link was opened on 2026-10-15. The sections above link the page they rely on; this list groups them.

**Expo**
- [Expo documentation](https://docs.expo.dev/) · [SDK 57 reference](https://docs.expo.dev/versions/v57.0.0/) · [SDK 57 changelog](https://expo.dev/changelog/sdk-57) · [SDK 57 bundled versions](https://github.com/expo/expo/blob/sdk-57/packages/expo/bundledNativeModules.json)
- [Expo CLI, `expo install`](https://docs.expo.dev/more/expo-cli/#install) · [environment setup](https://docs.expo.dev/get-started/set-up-your-environment/) · [environment variables](https://docs.expo.dev/guides/environment-variables/) · [EAS environment variables](https://docs.expo.dev/eas/environment-variables/)
- Router: [introduction](https://docs.expo.dev/router/introduction/) · [protected routes](https://docs.expo.dev/router/advanced/protected/) · [authentication](https://docs.expo.dev/router/advanced/authentication/) · [typed routes](https://docs.expo.dev/router/reference/typed-routes/) · [src directory](https://docs.expo.dev/router/reference/src-directory/) · [modals](https://docs.expo.dev/router/advanced/modals/) · [SDK reference](https://docs.expo.dev/versions/v57.0.0/sdk/router/)
- [`expo/fetch`](https://docs.expo.dev/versions/latest/sdk/expo/#expofetch-api) and [its source in SDK 57](https://github.com/expo/expo/tree/sdk-57/packages/expo/src/winter/fetch)
- Packages: [camera](https://docs.expo.dev/versions/v57.0.0/sdk/camera/) · [image picker](https://docs.expo.dev/versions/v57.0.0/sdk/imagepicker/) · [image manipulator](https://docs.expo.dev/versions/v57.0.0/sdk/imagemanipulator/) · [file system](https://docs.expo.dev/versions/v57.0.0/sdk/filesystem/) · [image](https://docs.expo.dev/versions/v57.0.0/sdk/image/) · [location](https://docs.expo.dev/versions/v57.0.0/sdk/location/) · [notifications](https://docs.expo.dev/versions/v57.0.0/sdk/notifications/) · [secure store](https://docs.expo.dev/versions/v57.0.0/sdk/securestore/) · [haptics](https://docs.expo.dev/versions/v57.0.0/sdk/haptics/) · [crypto](https://docs.expo.dev/versions/v57.0.0/sdk/crypto/) · [linking](https://docs.expo.dev/versions/v57.0.0/sdk/linking/) · [web browser](https://docs.expo.dev/versions/v57.0.0/sdk/webbrowser/) · [auth session](https://docs.expo.dev/versions/v57.0.0/sdk/auth-session/) · [AsyncStorage](https://docs.expo.dev/versions/v57.0.0/sdk/async-storage/) · [SVG](https://docs.expo.dev/versions/v57.0.0/sdk/svg/) · [Reanimated](https://docs.expo.dev/versions/v57.0.0/sdk/reanimated/) · [Gesture Handler](https://docs.expo.dev/versions/v57.0.0/sdk/gesture-handler/) · [FlashList](https://docs.expo.dev/versions/v57.0.0/sdk/flash-list/) · [Keyboard Controller](https://docs.expo.dev/versions/v57.0.0/sdk/keyboard-controller/) · [dev client](https://docs.expo.dev/versions/v57.0.0/sdk/dev-client/) · [icons](https://docs.expo.dev/guides/icons/)
- Native modules: [overview](https://docs.expo.dev/modules/overview/) · [Module API](https://docs.expo.dev/modules/module-api/) · [native view tutorial](https://docs.expo.dev/modules/native-view-tutorial/)
- Builds and delivery: [development builds](https://docs.expo.dev/develop/development-builds/introduction/) · [EAS Build](https://docs.expo.dev/build/introduction/) · [internal distribution](https://docs.expo.dev/build/internal-distribution/) · [Expo for web](https://docs.expo.dev/workflow/web/) · [progressive web apps](https://docs.expo.dev/guides/progressive-web-apps/) · [linking](https://docs.expo.dev/linking/overview/)
- Push: [overview](https://docs.expo.dev/push-notifications/overview/) · [sending notifications](https://docs.expo.dev/push-notifications/sending-notifications/)
- Quality: [unit testing](https://docs.expo.dev/develop/unit-testing/) · [ESLint](https://docs.expo.dev/guides/using-eslint/)

**Clerk**
- [Expo quickstart](https://clerk.com/docs/expo/getting-started/quickstart) · [Expo SDK reference](https://clerk.com/docs/reference/expo/overview) · [Expo web support](https://clerk.com/docs/guides/development/web-support/overview) · [package source](https://github.com/clerk/javascript/tree/main/packages/expo) · [`@clerk/expo` 4.12.0 manifest](https://registry.npmjs.org/@clerk/expo/4.12.0)
- Hooks: [`useAuth`](https://clerk.com/docs/reference/hooks/use-auth) · [`useSignIn`](https://clerk.com/docs/reference/hooks/use-sign-in) · [`useSignUp`](https://clerk.com/docs/reference/hooks/use-sign-up) · [`useUser`](https://clerk.com/docs/reference/hooks/use-user) · [`useSSO`](https://clerk.com/docs/reference/expo/native-hooks/use-sso) · [the `Clerk` object](https://clerk.com/docs/reference/objects/clerk)
- Custom flows: [email and password](https://clerk.com/docs/guides/development/custom-flows/authentication/email-password) · [multi-factor sign-in](https://clerk.com/docs/guides/development/custom-flows/authentication/multi-factor-authentication) · [managing MFA](https://clerk.com/docs/guides/development/custom-flows/account-updates/manage-mfa)
- Backend: [session tokens](https://clerk.com/docs/guides/sessions/session-tokens) · [manual JWT verification](https://clerk.com/docs/guides/sessions/manual-jwt-verification) · [customising session tokens](https://clerk.com/docs/guides/sessions/customize-session-tokens) · [`verifyToken`](https://clerk.com/docs/reference/backend/verify-token) · [webhooks](https://clerk.com/docs/guides/development/webhooks/overview) · [Svix signature verification](https://docs.svix.com/receiving/verifying-payloads/how)

**Libraries**
- [TanStack Query overview](https://tanstack.com/query/latest/docs/framework/react/overview) · [React Native](https://tanstack.com/query/latest/docs/framework/react/react-native) · [query keys](https://tanstack.com/query/latest/docs/framework/react/guides/query-keys) · [invalidation](https://tanstack.com/query/latest/docs/framework/react/guides/query-invalidation) · [retries](https://tanstack.com/query/latest/docs/framework/react/guides/query-retries) · [important defaults](https://tanstack.com/query/latest/docs/framework/react/guides/important-defaults)
- [openapi-typescript](https://openapi-ts.dev/introduction) · [openapi-fetch](https://openapi-ts.dev/openapi-fetch/) · [its API](https://openapi-ts.dev/openapi-fetch/api) · [auth middleware](https://openapi-ts.dev/openapi-fetch/middleware-auth)
- [Zustand](https://zustand.docs.pmnd.rs/) · [persist middleware](https://zustand.docs.pmnd.rs/reference/middlewares/persist) · [React Hook Form](https://react-hook-form.com/) · [Zod](https://zod.dev/) · [date-fns](https://date-fns.org/) · [@date-fns/tz](https://github.com/date-fns/tz)
- [react-native-svg](https://github.com/software-mansion/react-native-svg) · [Reanimated](https://docs.swmansion.com/react-native-reanimated/) · [Gesture Handler](https://docs.swmansion.com/react-native-gesture-handler/) · [FlashList](https://shopify.github.io/flash-list/) · [Keyboard Controller](https://kirillzyusko.github.io/react-native-keyboard-controller/) · [React Native Web](https://necolas.github.io/react-native-web/) · [React Native Testing Library](https://callstack.github.io/react-native-testing-library/)

**Web platform**
- [Server-sent events (WHATWG)](https://html.spec.whatwg.org/multipage/server-sent-events.html) · [MDN, using SSE](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events) · [MDN `EventSource`](https://developer.mozilla.org/en-US/docs/Web/API/EventSource) · [MDN Streams API](https://developer.mozilla.org/en-US/docs/Web/API/Streams_API)
- [MDN secure contexts](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts) · [MDN Geolocation API](https://developer.mozilla.org/en-US/docs/Web/API/Geolocation_API) · [MDN Clipboard API](https://developer.mozilla.org/en-US/docs/Web/API/Clipboard_API) · [MDN preflight requests](https://developer.mozilla.org/en-US/docs/Glossary/Preflight_request)
- [MDN file input](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/input/file) · [MDN `capture` attribute](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Attributes/capture) · [MDN `getUserMedia`](https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia) · [MDN CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CORS) · [MDN web app manifest](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest)
- [Google: OAuth in embedded browsers](https://developers.googleblog.com/upcoming-security-changes-to-googles-oauth-20-authorization-endpoint-in-embedded-webviews/) · [Google OAuth 2.0 policies](https://developers.google.com/identity/protocols/oauth2/policies) · [WebKit: Web Push on iOS and iPadOS](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/) · [Android Custom Tabs](https://developer.chrome.com/docs/android/custom-tabs) · [`SFSafariViewController`](https://developer.apple.com/documentation/safariservices/sfsafariviewcontroller)
- [WCAG 2.2 quick reference](https://www.w3.org/WAI/WCAG22/quickref/)

**Apple (LiDAR module)**
- [`AVDepthData`](https://developer.apple.com/documentation/avfoundation/avdepthdata) · [`ARWorldTrackingConfiguration`](https://developer.apple.com/documentation/arkit/arworldtrackingconfiguration) · [`ARWorldMap`](https://developer.apple.com/documentation/arkit/arworldmap) · [Core Motion](https://developer.apple.com/documentation/coremotion)

**Project documents**
- [System architecture](../architecture/README.md) · [data contracts](../architecture/contracts.md) · [database](../database/README.md) and [schema](../../backend/migrations/schema.surql) · [storage](../storage/README.md)
