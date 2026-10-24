// Google and Apple sign-in (roadmap F7 item 4; README 8, sign-in; closes README open decision 4): `useSSO` opens
// the provider in a browser session and returns to the `pdm` scheme, and Clerk's Core 3 hook activates the session
// itself. Both go and "Open in Safari or Chrome" shows instead in an embedded web view, where Google refuses OAuth
// (README 13) — that is the web build's (F9). Apple's button follows its guidelines: black, or white in dark mode,
// with the Apple logo and "Continue with Apple" (HIG Sign in with Apple).
import { useSSO } from "@clerk/expo/experimental";
import { SymbolView } from "expo-symbols";
import { useState } from "react";
import { Pressable, View } from "react-native";
import Svg, { Path } from "react-native-svg";

import { Banner, Text, hiddenSvg, minTouch, radii, space, useTheme } from "@/ui";

import { clerkErrorMessage, isStrategyOff } from "../model/clerkErrors";
import { ssoOutcome, type SsoStrategy } from "../model/sso";

export function SsoButtons() {
  const { startSSOFlow } = useSSO();
  const { colors, scheme } = useTheme();
  const [busy, setBusy] = useState<SsoStrategy | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function start(strategy: SsoStrategy) {
    if (busy) return;
    setBusy(strategy);
    setError(null);
    try {
      const result = await startSSOFlow({ strategy });
      setError(ssoOutcome(result));
    } catch (e) {
      setError(isStrategyOff(e) ? `Sign-in with ${strategy === "oauth_apple" ? "Apple" : "Google"} isn't turned on for PDM yet. Use your email.` : clerkErrorMessage(e));
    } finally {
      setBusy(null);
    }
  }

  const apple = scheme === "dark" ? { bg: "#FFFFFF", ink: "#000000" } : { bg: "#000000", ink: "#FFFFFF" };
  const pill = { minHeight: 52, borderRadius: radii.pill, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: space[2] } as const;

  return (
    <View style={{ gap: space[3] }}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Continue with Apple"
        accessibilityState={{ busy: busy === "oauth_apple", disabled: busy !== null }}
        disabled={busy !== null}
        onPress={() => void start("oauth_apple")}
        testID="sso-apple"
        style={({ pressed }) => [pill, { backgroundColor: apple.bg, opacity: pressed || busy === "oauth_apple" ? 0.7 : 1 }]}
      >
        <SymbolView name="apple.logo" tintColor={apple.ink} size={19} />
        <Text variant="headline" style={{ color: apple.ink }}>
          Continue with Apple
        </Text>
      </Pressable>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Continue with Google"
        accessibilityState={{ busy: busy === "oauth_google", disabled: busy !== null }}
        disabled={busy !== null}
        onPress={() => void start("oauth_google")}
        testID="sso-google"
        style={({ pressed }) => [pill, { minHeight: Math.max(minTouch, 52), backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.border, opacity: pressed || busy === "oauth_google" ? 0.7 : 1 }]}
      >
        <GoogleMark />
        <Text variant="headline">Continue with Google</Text>
      </Pressable>
      {error ? <Banner tone="danger" title="That didn't sign you in" message={error} /> : null}
      <View style={{ flexDirection: "row", alignItems: "center", gap: space[3] }}>
        <View style={{ flex: 1, height: 1, backgroundColor: colors.border }} />
        <Text variant="footnote" color="textMuted">
          or with email
        </Text>
        <View style={{ flex: 1, height: 1, backgroundColor: colors.border }} />
      </View>
    </View>
  );
}

/** Google's "G" in its four colours, as Google's sign-in branding asks. */
function GoogleMark() {
  return (
    <Svg width={18} height={18} viewBox="0 0 48 48" {...hiddenSvg}>
      <Path fill="#EA4335" d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z" />
      <Path fill="#4285F4" d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z" />
      <Path fill="#FBBC05" d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z" />
      <Path fill="#34A853" d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z" />
    </Svg>
  );
}
