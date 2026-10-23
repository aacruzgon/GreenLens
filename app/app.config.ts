import type { ExpoConfig } from "expo/config";

// docs/frontend/README.md §6 and §17. The app is built for the iPhone; the web build is the last slice (roadmap F9).
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
      NSLocationWhenInUseUsageDescription: "PDM uses your location once, to find your garden's weather and soil.",
      // The LiDAR day-1 check's evaluation captures leave the phone through the Files app (roadmap F5 item 2).
      UIFileSharingEnabled: true,
      LSSupportsOpeningDocumentsInPlace: true,
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
