# PDM 3D Garden: Rendering Notes

> **Status:** Draft v0.1, 2026-10-17. v0.1: how to render the stylised 3D garden on iOS, Android and web in Expo SDK 57 (react-three-fiber on expo-gl), the on-device ARKit mesh view, labels, interaction and accessibility, with one open expo-gl bug to patch.

This note covers how the app can draw the 3D garden view and the optional on-device LiDAR mesh view. It is a companion to [`design.md`](design.md) (same folder), [frontend README](../frontend/README.md) §13 and [architecture 6.8](../architecture/README.md#68-garden-map-lidar). The server holds only bed outlines, pin positions and canopy radii, so the stylised scene is built from that data on every platform. The ARKit mesh never leaves the phone.

Marks: ✓ means read in a primary source (named in the line). *unverified* means from memory, a secondary source, or not run.

## Contents

1. [Cross-platform 3D in Expo SDK 57](#1-cross-platform-3d-in-expo-sdk-57)
2. [Performance budget](#2-performance-budget)
3. [On-device mesh view (LiDAR iPhone)](#3-on-device-mesh-view-lidar-iphone)
4. [Labels and interaction](#4-labels-and-interaction)
5. [Reduced motion and accessibility](#5-reduced-motion-and-accessibility)
6. [Recommendation](#6-recommendation)
7. [Sources](#7-sources)

## 1. Cross-platform 3D in Expo SDK 57

### Versions

| Package | Version | Peer range or pin | Source |
|---|---|---|---|
| `expo` | 57.0.30 | bundles `expo-gl ~57.0.2`, `react-native 0.86.3`, `react 19.2.3` | ✓ `bundledNativeModules.json` on branch `sdk-57` |
| `expo-gl` | 57.0.2 for SDK 57 (npm `latest` is 58.0.3, for SDK 58) | none that matter | ✓ npm registry |
| `@react-three/fiber` | 9.8.2 | `react >=19 <19.4`, `three >=0.156`, `expo-gl >=11`, `react-native >=0.78`, `expo-asset`, `expo-file-system` | ✓ npm `peerDependencies` |
| `three` | 0.186.1 is latest; the expo-gl bug report used r181 | fiber wants `>=0.156` | ✓ npm; ✓ expo issue #51543 |
| `@react-three/drei` | 10.7.9 | `react ^19`, `three >=0.159`, `fiber ^9` | ✓ npm |
| `expo-three` | 8.0.0 | `three ^0.166.0` (older than fiber needs) | ✓ npm |
| `react-native-wgpu` | 0.5.17 | `react-native >=0.81` | ✓ npm |
| `react-native-filament` | 1.11.0 | needs `react-native-worklets-core` | ✓ npm |

React 19.2.3 sits inside fiber's `>=19 <19.4`. ✓ (bundledNativeModules, npm).

### Does fiber work on native, and in Expo Go?

- ✓ The R3F install page documents `expo install expo-gl`, then `npm install three @react-three/fiber`, and imports from `@react-three/fiber/native`. It says fiber 9 pairs with React 19. It says iOS simulators often have incomplete OpenGL ES support and can crash, so test on a physical iPhone. (r3f.docs.pmnd.rs, installation)
- ✓ The `@react-three/fiber` package.json has a `react-native` field pointing at its native build. Metro therefore picks the native renderer on iOS and Android with a plain `import { Canvas } from '@react-three/fiber'`, and the web build uses the normal DOM entry. One scene component can serve all three platforms. *Not run in this repo yet.*
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

## 2. Performance budget

All figures are guidance. *unverified:* none was measured on a device for this app; measure on the iPhone 17 Pro Max and one mid-range Android phone in the first build day.

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
- **Avoid:** image textures and GLB loading (see §1 issue 4), post-processing, transparent overlapping meshes in bulk, `Html` or per-frame React state, dozens of separate `<mesh>` elements with unique geometries, and `console.log` in `useFrame`.
- **Web.** Same code runs on plain WebGL at higher cost headroom, so the native budget covers it. Test on a judge's laptop browser with hardware acceleration on.

## 3. On-device mesh view (LiDAR iPhone)

Facts below come from memory of Apple's API; Apple's documentation pages render with JavaScript and could not be read by the fetch tool used for this note. Every claim in this section is *unverified* against the live page unless marked.

### Two ways to draw the mesh

1. **RealityKit `ARView` with debug options (cheapest).** Create an `ARView`, run an `ARWorldTrackingConfiguration` with `sceneReconstruction = .mesh` (check `ARWorldTrackingConfiguration.supportsSceneReconstruction(.mesh)` first), and set `arView.debugOptions = [.showSceneUnderstanding]`. This draws the reconstructed mesh as a wireframe-style overlay on the camera feed. It needs about ten lines. It is a debug visual, so its look is not controllable. *unverified.*
2. **Own rendering from `ARMeshAnchor`.** In the `ARSessionDelegate`, read `anchor.geometry` (`ARMeshGeometry`: vertices, faces, normals, optional classification) from each `ARMeshAnchor` and build a SceneKit or RealityKit `MeshResource` per anchor, replacing it on `didUpdate`. This lets the app colour by classification and hide the camera feed (after mapping). It is about 150 to 250 lines of Swift. *unverified.*

Recommendation: ship option 1 first, since it reuses the same `ARView` that the mapping session needs for tap-to-place (raycast to the ground plane). Move to option 2 only if the product wants a "scan replay" look.

### Exposing it to React Native

- ✓ The Expo Modules API defines a native view by subclassing `ExpoView` and registering it with `View(MyView.self) { Events(...); Prop(...) }` in the module's `definition()`; the JS side calls `requireNativeViewManager('Name')` (docs.expo.dev native-view tutorial).
- Plan: `LidarMapView` (an `ExpoView` that owns the `ARView`), props `mode` (`mapping` or `meshPreview`) and `showMesh`, events `onBedCorner`, `onPlantPin`, `onTrackingState`. The same view serves the mapping flow of architecture §6.8 and the optional mesh preview, so no second AR session runs. *design proposal.*
- Keep `ARView` as the single camera owner. Only one `ARSession` may run at a time, so the 3D garden (GL) view and the AR view must not be on screen together.

### iOS 26

*unverified:* I found no source stating that iOS 26 removes or changes `ARView`, `ARMeshAnchor` or `.showSceneUnderstanding`. Apple's newer RealityKit direction is `RealityView` plus SwiftUI and `SpatialTrackingSession`, but `ARView` on iPhone is still the documented path for LiDAR scene reconstruction as far as known. Build and run on the iPhone 17 Pro Max with iOS 26 and Xcode 26 before committing to either option.

### Development build and Apple account

- ✓ Custom native code needs a development build; Expo Go is a fixed app that cannot include `app/modules/lidar` (docs.expo.dev development builds). So the mesh view, the mapping session and the depth capture all require one. The 3D garden (§1) does not.
- ✓ "Local builds are the only way to install a development build on an iPhone without a paid Apple Developer account"; EAS iPhone builds need a paid account. Local flow: `npx expo prebuild`, then `npx expo run:ios --device`, with a unique `ios.bundleIdentifier` and Developer Mode on (same page).
- Free Apple ID personal team: *unverified* for this app, but ARKit needs only the camera permission string (`NSCameraUsageDescription`) and no entitlement, so a free team should be enough. Known free-team limits (apps expire after 7 days, a small cap on App IDs per week, no push, no TestFlight) are from memory, *unverified*. The web build and Android do not need any Apple account.

## 4. Labels and interaction

### Labels

- ✓ `Html` is not exported by `@react-three/drei/native` (drei README). So there are no DOM overlays on native.
- Best practice, and it also works on web: render labels as ordinary React Native `<Text>` views absolutely positioned over the `Canvas`, not inside the GL scene.
  - Each frame (in `useFrame`), project every plant's world position with `vector.project(camera)`, convert to pixels using the canvas size, and write the position to the label. Do this without React state: keep a ref per label and update `transform` through Reanimated shared values or `setNativeProps`. *unverified performance; 40 labels should be fine.*
  - Hide labels whose projected `z` is outside the clip range or that are behind the camera; fade labels by distance; show only the selected plant's label plus a status dot for the others when zoomed out.
  - Native `<Text>` gives crisp type, Dynamic Type, VoiceOver and the app's font for free. The alternative, GL text (troika or SDF text in three), is blurry at some scales, heavier, and *unverified* on expo-gl.
- A fallback is a canvas-drawn `CanvasTexture` sprite per label, but ✓ expo-gl restricts `texImage2D` inputs (see §1), so this is risky. Not recommended.

### Tap-to-select

- ✓ Fiber's native build supports pointer events on meshes (`onClick`, `onPointerDown`) with built-in raycasting, as the R3F install page and issues #3399 and #3583 show events firing on native. Put `onClick` on the instanced meshes; fiber reports `instanceId` on the event for `InstancedMesh`. *unverified on native for instances; test first.*
- Make hit areas generous: add an invisible, larger cylinder per plant (radius at least 0.25 m, or about 44 pt on screen at the default zoom).
- Selection state lives in the app's store, not in three. The selected plant gets a ring mesh and a bottom sheet (`design.md`).
- Also support selecting from the accessible list (§5), which sets the same state.

### Orbit controls

- `drei`'s `OrbitControls` wraps three's DOM-based controls and listens to pointer events on `gl.domElement`. On native there is no DOM element, so it is *unverified* and likely unreliable. Do not depend on it for native.
- Recommended: write a small `useOrbit` hook using `react-native-gesture-handler` (SDK 57 pins `~2.32.0` ✓ bundledNativeModules): a `Pan` gesture changes azimuth and elevation, a `Pinch` changes distance, a two-finger pan moves the target. Store the state in refs, apply it to the camera in `useFrame`, clamp elevation (about 10 to 85 degrees) and distance, and call `invalidate()`. The gestures wrap the `Canvas` view and work the same on web through gesture-handler's web support (*unverified for the web build; fall back to `OrbitControls` on web only if needed*).
- A "reset view" button and a top-down toggle help phone users and are also the accessible way to change the view.

## 5. Reduced motion and accessibility

### Reduce motion

- ✓ `AccessibilityInfo.isReduceMotionEnabled()` returns a promise of a boolean, and the `'reduceMotionChanged'` event fires on change. On Android it is also true when "Transition Animation Scale" is "Animation off". (reactnative.dev AccessibilityInfo)
- On web, use `matchMedia('(prefers-reduced-motion: reduce)')` (*unverified in this app*; Reanimated and RN Web may expose it too).
- Behaviour when on: skip the grow-in (draw plants at final size), switch camera moves to instant jumps, and let the growth scrubber step directly to a value with no tween. Keep `frameloop="demand"` so nothing animates on its own. No auto-rotate at any time.

### Screen readers

- A GL canvas is one opaque element to VoiceOver and TalkBack. Fiber's native canvas exposes nothing per object. *unverified, but expected.*
- Provide an equivalent accessible view, not a hidden one:
  - Mark the canvas `accessible={false}` and `importantForAccessibility="no-hide-descendants"` on Android, or give it a single `accessibilityLabel` such as "3D garden view, 10 beds and 40 plants. A list of plants follows."
  - Next to it, render a plain list of beds and plants (React Native `FlatList`) sorted by bed, then by distance, each row with plant name, status in words ("Tomato 1, needs attention, 45 cm tall"), and `accessibilityRole="button"`. Selecting a row selects the plant (same store as tap-to-select) and moves the camera to it.
  - When `isScreenReaderEnabled()` is true (✓ same page, event `'screenReaderChanged'`), show the list by default and the canvas second.
- Do not carry status by colour alone: add a shape or icon per status (dot, triangle, cross) on labels and in the list, and keep contrast on labels against both light and dark ground colours. See the accessibility notes in `design.md`.
- The scrubber (growth over time) is a standard `Slider` with an `accessibilityValue` text such as "12 March, 3 plants measured", and a stepper (previous and next scan date) as an alternative.
- Provide a text summary line above the canvas ("Garden map version 4, 10 beds, 40 plants, 3 need attention").

## 6. Recommendation

Use **`@react-three/fiber` 9.8.2 on `expo-gl ~57.0.2` with `three`, `drei` only for helpers, procedural instanced geometry, native `<Text>` labels, and gesture-handler orbit**.

1. **Stack.** `expo-gl ~57.0.2`, `@react-three/fiber 9.8.2`, `three` r181 (the version the SDK 57 bug report ran) to start, `react-native-gesture-handler ~2.32.0`, `react-native-reanimated 4.5.1`. Skip `expo-three`, `react-native-wgpu` and `react-native-filament`. No GLB files, no image textures.
2. **Where it runs.** iOS, Android and web from one scene component. It runs in Expo Go (expo-gl is in Expo Go ✓), so the 3D garden does not wait for a development build or a paid Apple account.
3. **First task.** Build a spike: 10 beds, 40 plants, instanced, with tap and orbit, on a physical iPhone in Expo Go and on a mid-range Android phone. The spike must apply the issue #51543 workaround first, and record whether it works. If it fails, try `three` below r163.
4. **LiDAR mesh view.** A `LidarMapView` in `app/modules/lidar` that owns one `ARView`, started with the `.showSceneUnderstanding` debug option; own-geometry rendering from `ARMeshAnchor` later if wanted. It needs a development build (local `npx expo run:ios --device`); a free personal team should be enough, *unverified*.
5. **Labels and list.** RN `<Text>` overlays projected from 3D each frame; an accessible plant list shown first for screen-reader users; reduce motion removes the grow-in.

Biggest risk: expo-gl 57.0.2 breaks three r163+ (issue #51543), and the iOS GL view has had crashes in recent SDKs. Mitigate with the patch, a pin, and the spike on day one. A second risk is that none of the gesture, instancing and Expo Go claims here was run on SDK 57 yet.

## 7. Sources

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
- Apple, not readable by the fetch tool, so *unverified*: [`ARMeshAnchor`](https://developer.apple.com/documentation/arkit/armeshanchor), [`ARView.DebugOptions`](https://developer.apple.com/documentation/realitykit/arview/debugoptions-swift.struct), [`ARConfiguration.SceneReconstruction`](https://developer.apple.com/documentation/arkit/arconfiguration/scenereconstruction-swift.enum)
