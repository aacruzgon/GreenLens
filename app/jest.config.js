// The mock's demo garden is in Toronto, and its fixtures are dated by the device's day. Tests run in the
// garden's zone, so a machine elsewhere, or a runner on UTC, sees the same days. Workers inherit it.
process.env.TZ = "America/Toronto";

/** @type {import('jest').Config} */
module.exports = {
  preset: "jest-expo",
  setupFiles: ["<rootDir>/jest.setup.ts"],
  moduleNameMapper: {
    "^@/assets/(.*)$": "<rootDir>/assets/$1",
    "^@/(.*)$": "<rootDir>/src/$1",
    "^@modules/(.*)$": "<rootDir>/modules/$1",
  },
  testPathIgnorePatterns: ["/node_modules/", "/\\.expo/", "/dist/"],
  // ESM-only or untranspiled packages that must go through babel.
  transformIgnorePatterns: [
    "node_modules/(?!((jest-)?react-native|@react-native(-community)?|expo(nent)?|@expo(nent)?/.*|@expo-google-fonts/.*|react-navigation|@react-navigation/.*|@clerk/.*|openapi-fetch|date-fns|@date-fns/.*|@shopify/flash-list|react-native-svg|zustand|@tanstack/.*|standard-navigation))",
  ],
};
