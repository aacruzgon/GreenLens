import type { ExpoConfig } from "expo/config";

// docs/frontend 6 and 17. EXPO_PUBLIC_* values are read at runtime from src/config/env.ts.
const config: ExpoConfig = {
  name: "PDM",
  slug: "pdm",
  scheme: "pdm",
  version: "0.1.0",
  orientation: "portrait",
  icon: "./assets/icon.png",
  userInterfaceStyle: "automatic",
  ios: {
    supportsTablet: true,
    bundleIdentifier: "org.buildnorth.pdm",
    infoPlist: {
      NSCameraUsageDescription: "PDM uses the camera to photograph your plants for a health check.",
      NSPhotoLibraryUsageDescription: "PDM lets you choose plant photos from your library.",
      NSLocationWhenInUseUsageDescription:
        "PDM uses your location once to find your garden's weather. You can enter a postal code instead.",
    },
  },
  android: {
    package: "org.buildnorth.pdm",
    adaptiveIcon: {
      backgroundColor: "#E1EEE4",
      foregroundImage: "./assets/android-icon-foreground.png",
      backgroundImage: "./assets/android-icon-background.png",
      monochromeImage: "./assets/android-icon-monochrome.png",
    },
    predictiveBackGestureEnabled: false,
  },
  web: {
    favicon: "./assets/favicon.png",
    output: "single",
    name: "PDM",
    shortName: "PDM",
    themeColor: "#2F6B47",
    backgroundColor: "#F6F7F2",
  },
  plugins: [
    "expo-router",
    "expo-status-bar",
    [
      "expo-splash-screen",
      {
        image: "./assets/splash-icon.png",
        imageWidth: 160,
        backgroundColor: "#F6F7F2",
        dark: { backgroundColor: "#0F1412" },
      },
    ],
    "expo-font",
    "expo-secure-store",
    "expo-web-browser",
    "@clerk/expo",
    "expo-image",
    [
      "expo-camera",
      { cameraPermission: "PDM uses the camera to photograph your plants for a health check." },
    ],
    [
      "expo-image-picker",
      { photosPermission: "PDM lets you choose plant photos from your library." },
    ],
    [
      "expo-location",
      {
        locationWhenInUsePermission:
          "PDM uses your location once to find your garden's weather. You can enter a postal code instead.",
      },
    ],
  ],
  experiments: {
    typedRoutes: true,
  },
};

export default config;
