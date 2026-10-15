# Applying the guidelines in Expo SDK 57

The guidelines name Apple's UIKit and SwiftUI APIs. This table gives the PDM app's equivalent for each component it uses, in Expo SDK 57 and React Native 0.86. The app's libraries are listed in [docs/frontend section 4](../../../docs/frontend/README.md#4-libraries); a library named here that isn't in that list (`expo-symbols`, `expo-glass-effect`, `@expo/ui`, DateTimePicker, the segmented control) is added there first. Prefer the native component, which follows the guidelines by itself, over a look-alike drawn in JavaScript.

## Contents

- [Navigation and presentation](#navigation-and-presentation)
- [Controls and input](#controls-and-input)
- [Content and status](#content-and-status)
- [Foundations](#foundations)
- [System features](#system-features)

## Navigation and presentation

| Guideline page | In the PDM app |
|---|---|
| [Tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars) | Expo Router [native tabs](https://docs.expo.dev/router/advanced/native-tabs/) draw the system tab bar. In SDK 57 they are imported from `expo-router/unstable-native-tabs`; `expo-router/native-tabs` starts with SDK 58. The JavaScript [Tabs](https://docs.expo.dev/router/advanced/tabs/) layout is the alternative |
| [Sheets](https://developer.apple.com/design/human-interface-guidelines/sheets) | A `Stack.Screen` with `presentation: 'formSheet'` and `sheetAllowedDetents` ([modals](https://docs.expo.dev/router/advanced/modals/)). One sheet at a time |
| [Modality](https://developer.apple.com/design/human-interface-guidelines/modality) | `presentation: 'modal'` or `'fullScreenModal'` on the stack screen. Full screen suits the camera ([modals](https://docs.expo.dev/router/advanced/modals/)) |
| Navigation stacks ([Layout](https://developer.apple.com/design/human-interface-guidelines/layout), [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars)) | Expo Router's [Stack](https://docs.expo.dev/router/advanced/stack/) is the native stack, with the back swipe, large titles (`headerLargeTitleEnabled`) and the header toolbar (`Stack.Toolbar`) |
| [Alerts](https://developer.apple.com/design/human-interface-guidelines/alerts), [Action sheets](https://developer.apple.com/design/human-interface-guidelines/action-sheets) | React Native [`Alert`](https://reactnative.dev/docs/alert) for alerts; [`ActionSheetIOS`](https://reactnative.dev/docs/actionsheetios) on iOS, with a sheet on Android and the web |
| [Menus](https://developer.apple.com/design/human-interface-guidelines/menus), [Context menus](https://developer.apple.com/design/human-interface-guidelines/context-menus), [Popovers](https://developer.apple.com/design/human-interface-guidelines/popovers) | [`@expo/ui`](https://docs.expo.dev/versions/v57.0.0/sdk/ui/) offers SwiftUI's `Menu`, `Popover` and `Picker` on iOS ([SwiftUI components](https://docs.expo.dev/versions/v57.0.0/sdk/ui/swift-ui/)). Check its status in SDK 57 before relying on it, and give Android and the web an equivalent |
| [Searching](https://developer.apple.com/design/human-interface-guidelines/searching), [Search fields](https://developer.apple.com/design/human-interface-guidelines/search-fields) | The stack header's search bar, or a `TextInput` with `returnKeyType="search"` for the species picker |

## Controls and input

| Guideline page | In the PDM app |
|---|---|
| [Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons) | The UI system's `Button` and `IconButton` on `Pressable`, at least 44 by 44 points, with `accessibilityRole="button"` and a label ([docs/frontend 14](../../../docs/frontend/README.md#14-ui-system)) |
| [Toggles](https://developer.apple.com/design/human-interface-guidelines/toggles) | React Native [`Switch`](https://reactnative.dev/docs/switch) |
| [Text fields](https://developer.apple.com/design/human-interface-guidelines/text-fields), [Entering data](https://developer.apple.com/design/human-interface-guidelines/entering-data), [Virtual keyboards](https://developer.apple.com/design/human-interface-guidelines/virtual-keyboards) | [`TextInput`](https://reactnative.dev/docs/textinput) with `textContentType`, `autoComplete`, `keyboardType` and `returnKeyType` set. [react-native-keyboard-controller](https://docs.expo.dev/versions/v57.0.0/sdk/keyboard-controller/) keeps fields above the keyboard |
| [Pickers](https://developer.apple.com/design/human-interface-guidelines/pickers) | [DateTimePicker](https://docs.expo.dev/versions/v57.0.0/sdk/date-time-picker/) for dates; lists in a sheet for longer choices |
| [Segmented controls](https://developer.apple.com/design/human-interface-guidelines/segmented-controls) | [Segmented control](https://docs.expo.dev/versions/v57.0.0/sdk/segmented-control/) |
| [Gestures](https://developer.apple.com/design/human-interface-guidelines/gestures) | [Gesture Handler](https://docs.expo.dev/versions/v57.0.0/sdk/gesture-handler/) with Reanimated; keep the system's own gestures (back swipe, sheet drag) free |

## Content and status

| Guideline page | In the PDM app |
|---|---|
| [Lists and tables](https://developer.apple.com/design/human-interface-guidelines/lists-and-tables) | [FlashList](https://docs.expo.dev/versions/v57.0.0/sdk/flash-list/) with the UI system's `ListItem` |
| [Image views](https://developer.apple.com/design/human-interface-guidelines/image-views), [Images](https://developer.apple.com/design/human-interface-guidelines/images) | [`expo-image`](https://docs.expo.dev/versions/v57.0.0/sdk/image/), with `accessibilityLabel` on meaningful photos |
| [Charts](https://developer.apple.com/design/human-interface-guidelines/charts), [Charting data](https://developer.apple.com/design/human-interface-guidelines/charting-data) | The UI system's `LineChart` on react-native-svg, with an accessible summary of the data |
| [Progress indicators](https://developer.apple.com/design/human-interface-guidelines/progress-indicators), [Loading](https://developer.apple.com/design/human-interface-guidelines/loading) | [`ActivityIndicator`](https://reactnative.dev/docs/activityindicator) for unknown durations; the scan's `StageProgress` for staged work; `Skeleton` while a screen loads |
| [Feedback](https://developer.apple.com/design/human-interface-guidelines/feedback) | `Banner`, `ErrorState` and `EmptyState` from the UI system, worded as in [screens.md](../../../docs/frontend/screens.md) |

## Foundations

| Guideline page | In the PDM app |
|---|---|
| [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility), [VoiceOver](https://developer.apple.com/design/human-interface-guidelines/voiceover) | React Native's [accessibility props](https://reactnative.dev/docs/accessibility): `accessibilityLabel`, `accessibilityRole`, `accessibilityState` and `accessibilityHint`. Test with VoiceOver on iPhone and TalkBack on Android |
| [Typography](https://developer.apple.com/design/human-interface-guidelines/typography) | Text scales with the system text size by default (`allowFontScaling`); cap it only where a layout breaks, with `maxFontSizeMultiplier` |
| [Color](https://developer.apple.com/design/human-interface-guidelines/color), [Dark Mode](https://developer.apple.com/design/human-interface-guidelines/dark-mode) | The UI system's colour roles through `useTheme()`, for light and dark ([color schemes](https://docs.expo.dev/guides/color-schemes/)). Status is never shown by colour alone |
| [Layout](https://developer.apple.com/design/human-interface-guidelines/layout) | [react-native-safe-area-context](https://docs.expo.dev/versions/v57.0.0/sdk/safe-area-context/) for safe areas; the 4-point spacing scale |
| [SF Symbols](https://developer.apple.com/design/human-interface-guidelines/sf-symbols), [Icons](https://developer.apple.com/design/human-interface-guidelines/icons) | [`expo-symbols`](https://docs.expo.dev/versions/v57.0.0/sdk/symbols/): SF Symbols on iOS, Material Symbols on Android and the web |
| [Materials](https://developer.apple.com/design/human-interface-guidelines/materials) | [`expo-glass-effect`](https://docs.expo.dev/versions/v57.0.0/sdk/glass-effect/) (`GlassView`, with `isLiquidGlassAvailable()` to check support) where the system material is wanted |
| [Motion](https://developer.apple.com/design/human-interface-guidelines/motion) | Reanimated, honouring the system's Reduce Motion setting |
| [Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics) | [`expo-haptics`](https://docs.expo.dev/versions/v57.0.0/sdk/haptics/): selection, impact and notification feedback, used sparingly |
| [Writing](https://developer.apple.com/design/human-interface-guidelines/writing) | The wording rules in [screens.md section 6](../../../docs/frontend/screens.md#6-wording-for-uncertain-results) |
| [Privacy](https://developer.apple.com/design/human-interface-guidelines/privacy) | Ask for camera, location and notifications at the moment they're needed, with purpose strings that say why ([docs/frontend 13](../../../docs/frontend/README.md#13-platform-adapters-and-the-lidar-module)) |

## System features

| Guideline page | In the PDM app |
|---|---|
| [Managing notifications](https://developer.apple.com/design/human-interface-guidelines/managing-notifications), [Notifications](https://developer.apple.com/design/human-interface-guidelines/notifications) | [`expo-notifications`](https://docs.expo.dev/push-notifications/overview/). The app asks for permission after sign-in ([flows 12](../../../docs/frontend/flows.md#12-push-notifications-and-deep-links)); check that timing against these pages |
| [Camera Control](https://developer.apple.com/design/human-interface-guidelines/camera-control), [Augmented reality](https://developer.apple.com/design/human-interface-guidelines/augmented-reality) | [`expo-camera`](https://docs.expo.dev/versions/v57.0.0/sdk/camera/) for capture; the LiDAR module for depth |
| [Maps](https://developer.apple.com/design/human-interface-guidelines/maps) | The garden map on react-native-svg (architecture 6.8) |
| [Managing accounts](https://developer.apple.com/design/human-interface-guidelines/managing-accounts), [Sign in with Apple](https://developer.apple.com/design/human-interface-guidelines/sign-in-with-apple) | Clerk's sign-in flows ([docs/frontend 8](../../../docs/frontend/README.md#8-authentication-clerk)). Account deletion is in the app, as App Store review requires |
| [Onboarding](https://developer.apple.com/design/human-interface-guidelines/onboarding), [Launching](https://developer.apple.com/design/human-interface-guidelines/launching) | The onboarding flow in [flows 2](../../../docs/frontend/flows.md#2-onboarding-and-the-first-garden); the splash screen stays up until Clerk has loaded ([flows 1](../../../docs/frontend/flows.md#1-app-start-and-sign-in)) |
