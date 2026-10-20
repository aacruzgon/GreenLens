// Global test setup. Keep it small: tests that need more mock it themselves.

jest.mock("@react-native-async-storage/async-storage", () =>
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  require("@react-native-async-storage/async-storage/jest/async-storage-mock"),
);

jest.mock("expo-haptics", () => ({
  impactAsync: jest.fn(async () => {}),
  notificationAsync: jest.fn(async () => {}),
  selectionAsync: jest.fn(async () => {}),
  ImpactFeedbackStyle: { Light: "light", Medium: "medium", Heavy: "heavy", Soft: "soft", Rigid: "rigid" },
  NotificationFeedbackType: { Success: "success", Warning: "warning", Error: "error" },
}));

// Reanimated 4 and its worklets runtime have no native side under Jest; both ship a mock.
// eslint-disable-next-line @typescript-eslint/no-require-imports
jest.mock("react-native-worklets", () => require("react-native-worklets/src/mock"));
// eslint-disable-next-line @typescript-eslint/no-require-imports
jest.mock("react-native-reanimated", () => require("react-native-reanimated/mock"));

// Safe-area insets come from the native side; the package's mock gives a fixed frame.
// eslint-disable-next-line @typescript-eslint/no-require-imports
jest.mock("react-native-safe-area-context", () => require("react-native-safe-area-context/jest/mock").default);

// Importing @clerk/expo starts clerk-js, which opens a MessagePort that keeps Jest from exiting.
// Tests run the demo session, so Clerk is a stub here.
jest.mock("@clerk/expo", () => {
  const passthrough = ({ children }: { children: unknown }) => children;
  return {
    ClerkProvider: passthrough,
    ClerkLoaded: passthrough,
    useAuth: () => ({ isLoaded: true, isSignedIn: false, getToken: async () => null, signOut: async () => {} }),
    useUser: () => ({ user: null }),
    useSignIn: () => ({ signIn: {}, fetchStatus: "idle", errors: {} }),
    useSignUp: () => ({ signUp: {}, fetchStatus: "idle", errors: {} }),
  };
});
jest.mock("@clerk/expo/token-cache", () => ({ tokenCache: undefined }));
