# PDM 3D Garden: Rendering Notes

> **Status:** Draft v0.3.3, 2026-10-25. v0.3.3: the web build, dropped on Oct 25, no longer appears among the targets that show the 2D map; the notes on fiber's web entry and the Apple account say it is dropped. v0.3.2: after the second run on the iPhone, labels rank the plants that need attention before the nearest, a dot tries the crown and either side before it is hidden (never with VoiceOver on), a dot taps as 44 pt, the plants' shared parts draw through `MeshInstancesComponent` on iOS 26, and the view freezes through the turntable's 3 s wait (sections 1 and 3). v0.3.1: after the first run on an iPhone, labels that would overlap shrink to their status dot, and the UIKit labels, not `AccessibilityComponent`s, are VoiceOver's elements, since an idle garden takes the `ARView` out of the window (section 1). v0.3: the app is iPhone only, so the 3D garden moves to RealityKit on Metal, as a native view in the LiDAR module, and react-three-fiber on expo-gl (OpenGL ES, deprecated since iOS 12) becomes the fallback; adds the RealityKit APIs with their iOS versions and a new recommendation. v0.2: folds in the PR #43 audit: fiber's optional peers, and status shapes and words as in design §6. v0.1: how to render the stylised 3D garden on iOS, Android and web in Expo SDK 57 (react-three-fiber on expo-gl), the on-device ARKit mesh view, labels, interaction and accessibility, with one open expo-gl bug to patch.

This note covers how the app can draw the 3D garden view and the optional on-device LiDAR mesh view. It is a companion to [`design.md`](design.md) (same folder), [frontend README](../frontend/README.md) §13 and [architecture 6.8](../architecture/README.md#68-garden-map-lidar). The server holds only bed outlines, pin positions and canopy radii, so the stylised scene is built from that data on the phone. The ARKit mesh never leaves the phone.

The app targets the iPhone only (decided 2026-10-18; Android is out of scope). So the garden no longer needs one scene for iOS, Android and web: it renders on Metal through RealityKit, in the same Swift module as the LiDAR mapping (section 1). Sections 2 to 6 keep the cross-platform plan as the fallback, and add the native details where they differ.

Marks: ✓ means read in a primary source (named in the line). *unverified* means from memory, a secondary source, or not run.

## Contents

1. [The garden on Metal: RealityKit in the LiDAR module](#1-the-garden-on-metal-realitykit-in-the-lidar-module)
2. [The fallback: react-three-fiber on expo-gl](#2-the-fallback-react-three-fiber-on-expo-gl)
3. [Performance budget](#3-performance-budget)
4. [On-device mesh view (LiDAR iPhone)](#4-on-device-mesh-view-lidar-iphone)
5. [Labels and interaction](#5-labels-and-interaction)
6. [Reduced motion and accessibility](#6-reduced-motion-and-accessibility)
7. [Recommendation](#7-recommendation)
8. [Sources](#8-sources)

## 1. The garden on Metal: RealityKit in the LiDAR module

### Why Metal

- **expo-gl is OpenGL ES on iOS.** ✓ `EXGLContext.mm` creates an `EAGLContext` with `kEAGLRenderingAPIOpenGLES3`, and `GLView.swift` draws into a `CAEAGLLayer` (expo, branch `sdk-57`). ✓ Apple marks `EAGLContext` deprecated since iOS 12 (Apple documentation). RealityKit renders with Metal, Apple's current GPU API.
- **One engine for the garden, the mapping and the mesh.** The mapping session ([architecture 6.8](../architecture/README.md#68-garden-map-lidar)) and the mesh view (section 4) already use RealityKit's `ARView`. The garden uses the same class with the camera off. The Swift module then owns one renderer, and the app adds no `expo-gl`, `three` or `@react-three/fiber`.
- **Apple's lighting and shadows.** ✓ `GroundingShadowComponent` (iOS 18) gives each plant a contact shadow. In three, that takes a shadow map or a fake disc (section 3).
- **The expo-gl bug goes away.** Issue #51543 (section 2) concerns expo-gl and three. Neither is used.
- **SceneKit is not the alternative.** ✓ Apple marks SceneKit deprecated in iOS 26 (Apple documentation), so RealityKit is the one to build on.

The costs:
- The garden needs a development build. The LiDAR module needs one anyway (section 4).
- The scene, labels and gestures are written in Swift.
- Expo Go shows the 2D map and the plant list instead of the 3D view. The 2D map needs no GPU; it is the same map as in the mockups' Map view (design §3.1).
- The mockups stay on three.js in the browser.

### The RealityKit APIs it needs

✓ Read from Apple's documentation JSON (`developer.apple.com/tutorials/data/documentation/…`) on 2026-10-18. The app targets iOS 26 for Liquid Glass, so every API here is available.

| API | iOS | Use |
|---|---|---|
| `ARView` `init(frame:cameraMode:automaticallyConfigureSession:)` with `ARView.CameraMode.nonAR` | 13.0 | The garden view, with no camera feed and no AR session |
| `PerspectiveCamera` | 13.0 | The orbit camera, moved by the gestures |
| `MeshResource.generateBox(size:cornerRadius:)`, `generateSphere(radius:)` | 13.0 | Beds with soft corners, canopy blobs |
| `MeshResource.generateCylinder(height:radius:)`, `generateCone(height:radius:)` | 18.0 | Stems, pots, status rings, frost covers |
| `LowLevelMesh` | 18.0 | Bed outlines and canopy rings built from the map, updated in place |
| `MeshInstancesComponent` | 26.0 | One draw for many plants of the same shape |
| `SimpleMaterial`, `UnlitMaterial` | 13.0 | Flat status colours; unlit for rings and covers |
| `DirectionalLightComponent` | 13.0 | One key light |
| `GroundingShadowComponent` | 18.0 | A contact shadow per plant |
| `CollisionComponent` | 13.0 | Hit shapes for tap-to-select, larger than the plant |
| `ARView.entity(at:)` | 13.0 | The entity under a tap |
| `ARView.project(_:)` | 13.0 | A plant's label position on screen |
| `SceneEvents.Update` | 13.0 | A per-frame callback to move the labels and run the scrubber's tween |
| `AccessibilityComponent` | 17.0 | Not used: the UIKit labels are the VoiceOver elements, since the `ARView` leaves the window while idle |
| `RealityView`, `realityViewCameraControls(_:)` | 18.0 | The SwiftUI alternative, with a built-in orbit control |
| `ViewAttachmentComponent` | visionOS only | Not available on iPhone, so labels are UIKit views (below) |

### Hosting it in React Native

1. **`ARView` in an `ExpoView` (recommended).** `ARView` is a UIKit view, so it fits the Expo Modules API directly: ✓ a native view subclasses `ExpoView` and is registered with `View(...)` in the module's `definition()` (docs.expo.dev module API). Proposed `GardenView`:
   - props: `map` (beds, pins and measurements, as the API returns them), `dateIndex`, `mode` (`health`, `growth` or `frost`), `selectedPlantId`, `scheme` and `reduceMotion`;
   - one event, `onSelectPlant`;
   - the app's store keeps the date, the mode and the selection, as with the 2D map. The scrubber sends an index, never geometry, and Swift tweens scale and colour.
   - Orbit: a `UIPanGestureRecognizer` changes azimuth and elevation, and a `UIPinchGestureRecognizer` changes distance, both applied to the `PerspectiveCamera`. Clamp elevation to about 10 to 85 degrees. *design proposal.*
2. **SwiftUI `RealityView` in a `UIHostingController`.** ✓ iOS 18 gives `RealityView` an orbit control, `realityViewCameraControls(.orbit)`. ✓ Expo's module API does not render SwiftUI views yet; its docs say to add a `UIHostingController`'s view to the UIKit view (docs.expo.dev module API). ✓ `RealityViewCameraContent` lists no projection method on iOS (its topics: `camera`, `cameraTarget`, `entities`, `environment`, `renderingEffects`), so placing UIKit labels is harder. Use it only if the built-in orbit saves real work.

### Labels, selection and accessibility on Metal

- **Labels are UIKit views** over the `ARView`: a capsule with the status shape, the plant name and, in Growth, the height. On each `SceneEvents.Update`, place each one at `project(_:)` of a point above its plant, and hide it when that returns `nil` or the plant is behind the camera. They get SF Pro, Dynamic Type and VoiceOver as ordinary views. The selected label is outlined, not filled: nothing tinted sits on the 3D view (design §2). They are placed the selected plant's first, then the plants that need attention (sick, then watch), then from the nearest; a label that would overlap one already placed shrinks to its status dot, placed above the plant, on its crown or beside the spot above it, whichever is free first, and hidden only when none is, its plant's ring still showing the status (✓ ran on the iOS 27 simulator: all 6 plants labelled or dotted, none overlapping at 40). With VoiceOver on, nothing is hidden, since each label is a plant's element. A dot is drawn small but taps as 44 pt. (✓ ran on an iPhone 13 Pro Max: 40 full labels piled up, two overlapped at 6 plants, and the first fix left dots over names and the sick plants as dots.)
- **Tap-to-select** uses `entity(at:)`. Give each plant an invisible `CollisionComponent` shape with a radius of at least 0.25 m, so the target is about 44 pt at the default zoom.
- **Accessibility:** each plant's UIKit label is its VoiceOver element, with the button trait and its words ("Tomato 1, sick, 62 cm tall"), also when shrunk to a dot; double tap selects the plant. Not an `AccessibilityComponent`: that lives in the `ARView`, which is out of the window while an idle garden shows its snapshot. The plant list in section 6 stays the screen reader's main view of the garden.
- **Reduce motion:** ✓ `UIAccessibility.isReduceMotionEnabled` (iOS 8). When it is on, skip the grow-in and the turntable, and jump the scrubber with no tween.
- **One AR session:** the garden's `ARView` is created with `.nonAR` and `automaticallyConfigureSession: false`, so it should run no `ARSession`. The mapping view stays the only camera owner. *unverified: confirm on the device that a non-AR `ARView` starts no session.*

## 2. The fallback: react-three-fiber on expo-gl

This is the cross-platform plan from v0.2. It stays as the fallback if the native view slips, because it runs in Expo Go with no Swift.

### Versions

| Package | Version | Peer range or pin | Source |
|---|---|---|---|
| `expo` | 57.0.30 | bundles `expo-gl ~57.0.2`, `react-native 0.86.3`, `react 19.2.3` | ✓ `bundledNativeModules.json` on branch `sdk-57` |
| `expo-gl` | 57.0.2 for SDK 57 (npm `latest` is 58.0.3, for SDK 58) | none that matter | ✓ npm registry |
| `@react-three/fiber` | 9.8.2 | `react >=19 <19.4`, `three >=0.156`; optional: `expo-gl >=11`, `react-native >=0.78`, `expo >=43`, `expo-asset >=8.4`, `expo-file-system >=11`, `react-dom >=19 <19.4` | ✓ npm `peerDependencies` and `peerDependenciesMeta` |
| `three` | 0.186.1 is latest; the expo-gl bug report used r181 | fiber wants `>=0.156` | ✓ npm; ✓ expo issue #51543 |
| `@react-three/drei` | 10.7.9 | `react ^19`, `three >=0.159`, `fiber ^9` | ✓ npm |
| `expo-three` | 8.0.0 | `three ^0.166.0` (older than fiber needs) | ✓ npm |
| `react-native-wgpu` | 0.5.17 | `react-native >=0.81` | ✓ npm |
| `react-native-filament` | 1.11.0 | needs `react-native-worklets-core` | ✓ npm |

React 19.2.3 sits inside fiber's `>=19 <19.4`. ✓ (bundledNativeModules, npm).

### Does fiber work on native, and in Expo Go?

- ✓ The R3F install page documents `expo install expo-gl`, then `npm install three @react-three/fiber`, and imports from `@react-three/fiber/native`. It says fiber 9 pairs with React 19. It says iOS simulators often have incomplete OpenGL ES support and can crash, so test on a physical iPhone. (r3f.docs.pmnd.rs, installation)
- ✓ The `@react-three/fiber` package.json has a `react-native` field pointing at its native build. Metro therefore picks the native renderer on iOS and Android with a plain `import { Canvas } from '@react-three/fiber'`, and a web build (dropped on Oct 25) would use the normal DOM entry. One scene component can serve all three platforms. *Not run in this repo yet.*
- ✓ `expo-gl` is listed as supported in Expo Go, on Android, iOS and web (docs.expo.dev GLView page). It needs no custom native code, so fiber, three and expo-gl run in Expo Go. *Not run on SDK 57 Expo Go in this repo yet.*
- ✓ On web, `expo-gl` is not needed: fiber's web entry uses the browser's WebGL canvas.

### Known issues

1. ✓ **Open bug, SDK 57 specific.** expo issue [#51543](https://github.com/expo/expo/issues/51543) (opened 2026-10-14, label `sdk-57`, open, fix PR #51572 unreviewed): since `expo-gl` 56.0.5, a WebGL 2 context also answers `true` to `instanceof WebGLRenderingContext`. three.js r163 and later then throws "WebGL 1 is not supported since r163" and nothing renders. Reproduced on Android and iOS simulators with expo 57.0.28, expo-gl 57.0.2, RN 0.86.3, three r181. It was not tested on physical devices.
   - Workaround from the report (untested by the bot): in `onContextCreate`, override `WebGLRenderingContext[Symbol.hasInstance]` so it returns `false` for WebGL 2 instances. It patches a global.
   - Fiber's native `Canvas` creates the renderer itself, so the patch must run before the first render. Put it in a module imported before the `Canvas` mounts, and re-test once expo-gl ships the fix. *unverified whether fiber exposes a hook early enough; check on first run.*
   - Fallback if the patch fails: pin `three` below r163 (fiber allows `>=0.156`). This loses newer three features. *unverified.*
2. ✓ expo-gl lacks several WebGL 2 methods (`getUniform`, `getVertexAttrib`, `getBufferSubData`, `renderbufferStorageMultisample`, sync objects). `texImage2D` pixels must be `null`, an `ArrayBuffer` or `{ localUri }`. Bad arguments can crash natively. Some old Android phones lack WebGL 2. (docs.expo.dev GLView page)
3. ✓ History on the New Architecture: fiber issue #3399 (nothing rendered on iOS with the New Architecture) was an expo-gl fault, fixed and closed 2025-06-29 via PR #3539. expo issue #42520 (iOS crash in `GLView.layoutSubviews`, SDK 55 bare) was closed as incomplete with no confirmed fix. Both are old and not SDK 57, but show the iOS GL view is the fragile part.
4. ✓ Loaders: fiber issues #3583, #3558, #3550, #3496, #3254 report GLTF texture and `Base64`/`url` errors on native. This plan avoids them: the scene uses procedural geometry and flat colours, no GLB and no image textures. Do not add `useGLTF` or `useTexture` to the garden.
5. ✓ Worklets: three.js does not run inside Reanimated worklets with expo-gl. Keep all three code on the JS thread. (GLView page)
6. ✓ `@react-three/drei/native` re-exports `../core` (read in the 10.7.9 tarball); the drei README says the native build omits `Html` and `Loader`. Core components that need the DOM or web workers (for example troika `Text`) *unverified* on native. Avoid drei beyond simple helpers.

### Alternatives

- **`expo-three` 8.0.0** is a thin helper (texture and loader glue) and its peer pin `three ^0.166.0` is old. It adds nothing fiber does not cover. Skip.
- **`react-native-wgpu` 0.5.17** (Dawn-based WebGPU; iOS, Android, macOS, visionOS and web per its site) ✓ README. Whether it runs in Expo Go and how well three's `WebGPURenderer` works under fiber are *unverified*; it is a native module, so it needs a development build (it is not in Expo Go's fixed set, *unverified*). A user report on its tracker ([#214](https://github.com/wcandillon/react-native-webgpu/issues/214)) says fiber on iOS never got past `await state.gl.init()` ✓ (seen via search summary only). Too risky for a 10-day build; revisit later.
- **`react-native-filament`** is a glTF/PBR engine for pre-authored assets, needs a development build, and has no web target. It fits models, not procedural scenes. Skip.

## 3. Performance budget

On Metal, the same scene limits apply: about 10 beds and 40 plants, under 50,000 triangles, flat materials, one light, and no frames drawn while idle. With `MeshInstancesComponent`, each plant shape is one draw: the app batches each shared mesh by material (the stems, the pots, the pots' soil, a crown per green and the dead canopy, a ring per status), so 40 plants come to about a dozen draws instead of 160 to 240. Instances share their entity's material (✓ Apple's `LowLevelInstanceData` has transforms only; a per-instance colour needs a `CustomMaterial` shader), so a ring that is changing colour draws on its own until the tween ends. Before iOS 26 each part is its own entity. The view also freezes through the turntable's 3 s wait and wakes on a timer when the turn starts (✓ measured on the iPhone: 280 ms/s of main thread while drawing). Measure on the device with Xcode's GPU tools. The bullets below are written for three on expo-gl.

All figures are guidance. *unverified:* none was measured on a device for this app; measure on the iPhone 17 Pro Max in the first build day.

Scene: about 10 beds and 40 plants.

- **Draw calls.** Aim for under 100. Ten beds as one `InstancedMesh` (a unit box, scaled per instance) is 1 call. Plants: each plant is a few primitives (trunk, 1 to 3 canopy blobs). Use one `InstancedMesh` per primitive type, with per-instance matrix and per-instance colour. That is about 4 to 6 calls for all 40 plants.
- **Triangles.** Keep the whole scene under 50,000. Low-poly spheres at 8 by 6 segments (about 80 triangles) and cylinders at 6 sides are enough. A mid-range Android phone should still hold 60 frames per second well below that.
- **Materials.** Use `MeshLambertMaterial` or `MeshBasicMaterial` with flat colours. `MeshStandardMaterial` costs more per pixel and adds nothing for a stylised look.
- **Lights.** One ambient plus one directional light. No point lights.
- **Shadows.** Avoid real-time shadow maps on Android. If wanted at all, use one 1024 by 1024 directional shadow map on iOS only, or fake it with a blurred dark disc (a flat circle mesh with a radial alpha gradient made in code) under each plant.
- **Pixel ratio.** Cap `dpr` at 2 (`dpr={[1, 2]}`). A 3x phone at full ratio triples fill cost for little gain.
- **Render loop.** Use `frameloop="demand"` and call `invalidate()` on interaction and during animations. An idle garden should draw no frames. The grow-in animation and the scrubber then cost power only while moving.
- **Antialiasing.** Leave MSAA off on Android; on iOS expo-gl has an `msaaSamples` prop ✓ (GLView page). Try 4 on iOS.
- **Updates.** Update instance matrices in place and set `instanceMatrix.needsUpdate`. Never rebuild geometry on a scrubber move; change scale and colour only.
- **Avoid:** image textures and GLB loading (see §2 issue 4), post-processing, transparent overlapping meshes in bulk, `Html` or per-frame React state, dozens of separate `<mesh>` elements with unique geometries, and `console.log` in `useFrame`.
- **Web.** Same code runs on plain WebGL at higher cost headroom, so the native budget covers it. Test on a judge's laptop browser with hardware acceleration on.

## 4. On-device mesh view (LiDAR iPhone)

Facts below come from memory of Apple's API; Apple's documentation pages render with JavaScript and could not be read by the fetch tool used for this note. Every claim in this section is *unverified* against the live page unless marked.

### Two ways to draw the mesh

1. **RealityKit `ARView` with debug options (cheapest).** Create an `ARView`, run an `ARWorldTrackingConfiguration` with `sceneReconstruction = .mesh` (check `ARWorldTrackingConfiguration.supportsSceneReconstruction(.mesh)` first), and set `arView.debugOptions = [.showSceneUnderstanding]`. This draws the reconstructed mesh as a wireframe-style overlay on the camera feed. It needs about ten lines. It is a debug visual, so its look is not controllable. *unverified.*
2. **Own rendering from `ARMeshAnchor`.** In the `ARSessionDelegate`, read `anchor.geometry` (`ARMeshGeometry`: vertices, faces, normals, optional classification) from each `ARMeshAnchor` and build a SceneKit or RealityKit `MeshResource` per anchor, replacing it on `didUpdate`. This lets the app colour by classification and hide the camera feed (after mapping). It is about 150 to 250 lines of Swift. *unverified.*

Recommendation: ship option 1 first, since it reuses the same `ARView` that the mapping session needs for tap-to-place (raycast to the ground plane). Move to option 2 only if the product wants a "scan replay" look.

### Exposing it to React Native

- ✓ The Expo Modules API defines a native view by subclassing `ExpoView` and registering it with `View(MyView.self) { Events(...); Prop(...) }` in the module's `definition()`; the JS side calls `requireNativeViewManager('Name')` (docs.expo.dev native-view tutorial).
- Plan: `LidarMapView` (an `ExpoView` that owns the `ARView`), props `mode` (`mapping` or `meshPreview`) and `showMesh`, events `onBedCorner`, `onPlantPin`, `onTrackingState`. The same view serves the mapping flow of architecture §6.8 and the optional mesh preview, so no second AR session runs. *design proposal.*
- Keep the mapping `ARView` as the single camera owner. Only one `ARSession` may run at a time. The garden's own `ARView` runs without a camera (section 1), so the two never share a session.

### iOS 26

*unverified:* I found no source stating that iOS 26 removes or changes `ARView`, `ARMeshAnchor` or `.showSceneUnderstanding`. Apple's newer RealityKit direction is `RealityView` plus SwiftUI and `SpatialTrackingSession`, but `ARView` on iPhone is still the documented path for LiDAR scene reconstruction as far as known. Build and run on the iPhone 17 Pro Max with iOS 26 and Xcode 26 before committing to either option.

### Development build and Apple account

- ✓ Custom native code needs a development build; Expo Go is a fixed app that cannot include `app/modules/lidar` (docs.expo.dev development builds). So the mesh view, the mapping session, the depth capture and the 3D garden (section 1) all require one. Only the fallback (section 2) runs in Expo Go.
- ✓ "Local builds are the only way to install a development build on an iPhone without a paid Apple Developer account"; EAS iPhone builds need a paid account. Local flow: `npx expo prebuild`, then `npx expo run:ios --device`, with a unique `ios.bundleIdentifier` and Developer Mode on (same page).
- Free Apple ID personal team: *unverified* for this app, but ARKit needs only the camera permission string (`NSCameraUsageDescription`) and no entitlement, so a free team should be enough. Known free-team limits (apps expire after 7 days, a small cap on App IDs per week, no push, no TestFlight) are from memory, *unverified*. A web build (dropped on Oct 25) would not need any Apple account.

## 5. Labels and interaction

This section is written for the fallback (section 2). On Metal, see section 1.

### Labels

- ✓ `Html` is not exported by `@react-three/drei/native` (drei README). So there are no DOM overlays on native.
- Best practice, and it also works on web: render labels as ordinary React Native `<Text>` views absolutely positioned over the `Canvas`, not inside the GL scene.
  - Each frame (in `useFrame`), project every plant's world position with `vector.project(camera)`, convert to pixels using the canvas size, and write the position to the label. Do this without React state: keep a ref per label and update `transform` through Reanimated shared values or `setNativeProps`. *unverified performance; 40 labels should be fine.*
  - Hide labels whose projected `z` is outside the clip range or that are behind the camera; fade labels by distance; show only the selected plant's label plus a status dot for the others when zoomed out.
  - Native `<Text>` gives crisp type, Dynamic Type, VoiceOver and the app's font for free. The alternative, GL text (troika or SDF text in three), is blurry at some scales, heavier, and *unverified* on expo-gl.
- A fallback is a canvas-drawn `CanvasTexture` sprite per label, but ✓ expo-gl restricts `texImage2D` inputs (see §2), so this is risky. Not recommended.

### Tap-to-select

- ✓ Fiber's native build supports pointer events on meshes (`onClick`, `onPointerDown`) with built-in raycasting, as the R3F install page and issues #3399 and #3583 show events firing on native. Put `onClick` on the instanced meshes; fiber reports `instanceId` on the event for `InstancedMesh`. *unverified on native for instances; test first.*
- Make hit areas generous: add an invisible, larger cylinder per plant (radius at least 0.25 m, or about 44 pt on screen at the default zoom).
- Selection state lives in the app's store, not in three. The selected plant gets a ring mesh and a bottom sheet (`design.md`).
- Also support selecting from the accessible list (§6), which sets the same state.

### Orbit controls

- `drei`'s `OrbitControls` wraps three's DOM-based controls and listens to pointer events on `gl.domElement`. On native there is no DOM element, so it is *unverified* and likely unreliable. Do not depend on it for native.
- Recommended: write a small `useOrbit` hook using `react-native-gesture-handler` (SDK 57 pins `~2.32.0` ✓ bundledNativeModules): a `Pan` gesture changes azimuth and elevation, a `Pinch` changes distance, a two-finger pan moves the target. Store the state in refs, apply it to the camera in `useFrame`, clamp elevation (about 10 to 85 degrees) and distance, and call `invalidate()`. The gestures wrap the `Canvas` view and work the same on web through gesture-handler's web support (*unverified for the web build, dropped on Oct 25; fall back to `OrbitControls` on web only if needed*).
- A "reset view" button and a top-down toggle help phone users and are also the accessible way to change the view.

## 6. Reduced motion and accessibility

The rules apply on both paths. On Metal the reduce-motion check is `UIAccessibility.isReduceMotionEnabled` and the per-plant labels are the UIKit labels (section 1); the plant list is a React Native view either way.

### Reduce motion

- ✓ `AccessibilityInfo.isReduceMotionEnabled()` returns a promise of a boolean, and the `'reduceMotionChanged'` event fires on change. On Android it is also true when "Transition Animation Scale" is "Animation off". (reactnative.dev AccessibilityInfo)
- On web, use `matchMedia('(prefers-reduced-motion: reduce)')` (*unverified in this app*; Reanimated and RN Web may expose it too).
- Behaviour when on: skip the grow-in (draw plants at final size), switch camera moves to instant jumps, and let the growth scrubber step directly to a value with no tween. Keep `frameloop="demand"` so nothing animates on its own. No auto-rotate at any time.

### Screen readers

- A GL canvas is one opaque element to VoiceOver and TalkBack. Fiber's native canvas exposes nothing per object. *unverified, but expected.*
- Provide an equivalent accessible view, not a hidden one:
  - Mark the canvas `accessible={false}` and `importantForAccessibility="no-hide-descendants"` on Android, or give it a single `accessibilityLabel` such as "3D garden view, 10 beds and 40 plants. A list of plants follows."
  - Next to it, render a plain list of beds and plants (React Native `FlatList`) sorted by bed, then by distance, each row with plant name, status in words ("Tomato 1, sick, 62 cm tall"), and `accessibilityRole="button"`. Selecting a row selects the plant (same store as tap-to-select) and moves the camera to it.
  - When `isScreenReaderEnabled()` is true (✓ same page, event `'screenReaderChanged'`), show the list by default and the canvas second.
- Do not carry status by colour alone: add a shape or icon per status (healthy ● circle, watch ▲ triangle, sick ◆ diamond, dead □ hollow square, as in [design §6](design.md#6-rules-every-direction-follows)) on labels and in the list, and keep contrast on labels against both light and dark ground colours. See the accessibility notes in `design.md`.
- The scrubber (growth over time) is a standard `Slider` with an `accessibilityValue` text such as "12 March, 3 plants measured", and a stepper (previous and next scan date) as an alternative.
- Provide a text summary line above the canvas ("Garden map version 4, 10 beds, 40 plants, 3 need attention").

## 7. Recommendation

Draw the 3D garden with **RealityKit on Metal, as a native `GardenView` in `app/modules/lidar`**, with procedural meshes, UIKit labels and the app's store driving it through props.

1. **Stack.** RealityKit is part of iOS; nothing is installed. The view is an `ARView` in `.nonAR` mode inside an `ExpoView` (section 1). Skip `expo-gl`, `three`, `@react-three/fiber`, `expo-three`, `react-native-wgpu` and `react-native-filament`. No USDZ or GLB files, no image textures.
2. **Where it runs.** The iPhone development build, the same build as the LiDAR features. Expo Go shows the 2D map and the plant list instead.
3. **First task.** A day-one spike in a local development build on the iPhone 17 Pro Max:
   - the mock garden: 2 beds and 6 plants, tap to select, orbit, labels, the three modes and the date scrubber;
   - then 10 beds and 40 plants, with the frame time measured in Xcode;
   - confirm that the non-AR view starts no AR session while the mapping view is closed.
4. **LiDAR mesh view.** A `LidarMapView` in the same module that owns the mapping `ARView`, started with the `.showSceneUnderstanding` debug option; own-geometry rendering from `ARMeshAnchor` later if wanted (section 4). A free personal team should be enough to install it, *unverified*.
5. **Labels and list.** UIKit labels placed from `project(_:)` each frame, shrunk to a status dot where they would overlap, and VoiceOver's element per plant; the plant list shown first for screen-reader users; reduce motion removes the grow-in and the turntable.
6. **Fallback.** If the native view slips, use fiber on expo-gl (section 2) with the #51543 workaround applied. It runs in Expo Go and needs no Swift.

Biggest risk: the Swift work lands on whoever builds the LiDAR module, so the garden and the mapping compete for the same person. Start the garden spike on day one, beside the LiDAR day-1 check (architecture §10), and agree the `GardenView` props first so the screen can be built against the 2D map meanwhile. A second risk is that nothing here has run on the device yet.

## 8. Sources

All read on 2026-10-17 unless noted.

- npm registry (`npm view`): `@react-three/fiber` 9.8.2, `expo-gl` 58.0.3 latest, `three` 0.186.1, `@react-three/drei` 10.7.9, `expo-three` 8.0.0, `react-native-wgpu` 0.5.17, `react-native-filament` 1.11.0, `react-native-gesture-handler` 3.4.0 latest (SDK 57 pins ~2.32.0).
- Expo SDK 57 pins: <https://github.com/expo/expo/blob/sdk-57/packages/expo/bundledNativeModules.json>
- Expo GLView (expo-gl) reference: <https://docs.expo.dev/versions/latest/sdk/gl-view/> (page labelled v58.0.0)
- expo-gl WebGL 2 `instanceof` bug: <https://github.com/expo/expo/issues/51543>; fix PR <https://github.com/expo/expo/pull/51572>
- expo-gl iOS crash with fiber: <https://github.com/expo/expo/issues/42520>
- R3F installation (native): <https://r3f.docs.pmnd.rs/getting-started/installation>
- R3F native and New Architecture: <https://github.com/pmndrs/react-three-fiber/issues/3399>; loader issues <https://github.com/pmndrs/react-three-fiber/issues?q=is%3Aissue+expo-gl>
- `@react-three/fiber@9.8.2` and `@react-three/drei@10.7.9` package tarballs (package.json `react-native` field; `native/index.d.ts`)
- drei README (native omits `Html` and `Loader`): <https://github.com/pmndrs/drei>
- react-native-webgpu: <https://github.com/wcandillon/react-native-webgpu>, <https://wcandillon.github.io/react-native-webgpu/>, issue <https://github.com/wcandillon/react-native-webgpu/issues/214> (search summary only)
- Expo native view tutorial: <https://docs.expo.dev/modules/native-view-tutorial/>
- Expo development builds: <https://docs.expo.dev/develop/development-builds/introduction/>
- React Native AccessibilityInfo: <https://reactnative.dev/docs/accessibilityinfo>
- Apple documentation JSON, read on 2026-10-18 for section 1 (iOS availability and deprecation): `ARView.CameraMode.nonAR`, `ARView.init(frame:cameraMode:automaticallyConfigureSession:)`, `ARView.project(_:)`, `ARView.entity(at:)`, `PerspectiveCamera`, `MeshResource` generators, `LowLevelMesh`, `MeshInstancesComponent`, `SimpleMaterial`, `UnlitMaterial`, `DirectionalLightComponent`, `GroundingShadowComponent`, `CollisionComponent`, `SceneEvents.Update`, `AccessibilityComponent`, `RealityView`, `RealityViewCameraContent`, `realityViewCameraControls(_:)`, `ViewAttachmentComponent`, SceneKit, `EAGLContext` and `UIAccessibility.isReduceMotionEnabled`, at `https://developer.apple.com/tutorials/data/documentation/<framework>/<symbol>.json`
- expo-gl's iOS sources on branch `sdk-57`: <https://github.com/expo/expo/tree/sdk-57/packages/expo-gl/ios> (`EXGLContext.mm`, `GLView.swift`)
- Expo Modules API (native views, SwiftUI note): <https://docs.expo.dev/modules/module-api/>
- Apple, not readable by the fetch tool, so *unverified*: [`ARMeshAnchor`](https://developer.apple.com/documentation/arkit/armeshanchor), [`ARView.DebugOptions`](https://developer.apple.com/documentation/realitykit/arview/debugoptions-swift.struct), [`ARConfiguration.SceneReconstruction`](https://developer.apple.com/documentation/arkit/arconfiguration/scenereconstruction-swift.enum)
