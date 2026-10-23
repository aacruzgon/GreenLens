import type { ExpoConfig } from "expo/config";

// docs/frontend/README.md §6 and §17. The app is built for the iPhone; the web build is the last slice (roadmap F9).

// A development or evaluation build (EXPO_PUBLIC_DEV_TOOLS=1): the LiDAR tools in Settings, and Documents in the
// Files app so the evaluation captures can leave the phone (roadmap F5 item 2). A build for users has neither.
const devTools = process.env.EXPO_PUBLIC_DEV_TOOLS === "1";

const config: ExpoConfig = {
  name: "PDM",
  slug: "pdm",
  version: "1.0.0",
  orientation: "portrait",
  icon: "./assets/images/icon.png",
  scheme: "pdm",
  userInterfaceStyle: "automatic",
  ios: {
    bundleIdentifier: "org.buildnorth.pdm",
    supportsTablet: false,
    icon: "./assets/expo.icon",
    infoPlist: {
      NSCameraUsageDescription: "PDM uses the camera to photograph your plants for a diagnosis.",
      NSLocationWhenInUseUsageDescription:
        "PDM uses your location to find your garden's weather and soil, and while you map a garden, to point its map north.",
      // The LiDAR day-1 check's evaluation captures leave the phone through the Files app (roadmap F5 item 2).
      // Only Documents/evaluation is there; capture drafts are kept in Application Support (platform/files.ts).
      ...(devTools ? { UIFileSharingEnabled: true, LSSupportsOpeningDocumentsInPlace: true } : {}),
      // The scan's Live Activity (targets/scan-activity, modules/live-activity), updated by the app itself.
      NSSupportsLiveActivities: true,
    },
  },
  web: {
    output: "single",
    favicon: "./assets/images/favicon.png",
  },
  plugins: [
    "expo-router",
    "expo-secure-store",
    "@clerk/expo",
    "expo-asset",
    // Adds the widget extensions in targets/ to the Xcode project on prebuild (design open decision 4).
    "@bacons/apple-targets",
    // The scene life cycle, without which iOS 27 stops the app at launch (plugins/withSceneLifecycle.js).
    "./plugins/withSceneLifecycle",
    [
      "expo-font",
      {
        fonts: ["./node_modules/@expo-google-fonts/urbanist/600SemiBold/Urbanist_600SemiBold.ttf"],
      },
    ],
    [
      "expo-splash-screen",
      {
        backgroundColor: "#F6F7F2",
        dark: { backgroundColor: "#0F1412" },
        image: "./assets/images/splash-icon.png",
        imageWidth: 76,
      },
    ],
  ],
  experiments: {
    typedRoutes: true,
    reactCompiler: true,
  },
};

export default config;
