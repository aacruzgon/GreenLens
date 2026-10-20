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
