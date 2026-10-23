// The scan's Live Activity: a WidgetKit extension that @bacons/apple-targets adds to the Xcode project on
// `npx expo prebuild` (roadmap F5 item 6; design open decision 4). Its colours are Conservatory's, from
// src/ui/palette.json. No entitlements: a Live Activity updated by the app itself needs none, and an App Group
// or push updates would need capabilities a free Apple ID's personal team can't sign.
const palette = require("../../src/ui/palette.json");

const pair = (key) => ({ light: palette.light[key], dark: palette.dark[key] });

/** @type {import('@bacons/apple-targets/app.plugin').ConfigFunction} */
module.exports = () => ({
  type: "widget",
  name: "ScanActivity",
  displayName: "PDM scan",
  bundleIdentifier: ".scanactivity",
  // The app's own minimum (Expo SDK 57's pods are iOS 16.4).
  deploymentTarget: "16.4",
  colors: {
    $accent: pair("accent"),
    $widgetBackground: pair("background"),
    pdmBackground: pair("background"),
    pdmText: pair("text"),
    pdmMuted: pair("textMuted"),
    pdmAccent: pair("accent"),
    pdmTrack: pair("border"),
    pdmWatch: pair("watch"),
    pdmDanger: pair("danger"),
    // The Dynamic Island is always black: the dark palette's values in both appearances.
    islandAccent: palette.dark.accent,
    islandMuted: palette.dark.textMuted,
    islandTrack: palette.dark.border,
    islandWatch: palette.dark.watch,
    islandDanger: palette.dark.danger,
  },
});
