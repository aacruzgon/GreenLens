// Apple's own "Continue with Apple" button (HIG Sign in with Apple: the system button meets Apple's branding rules,
// which a hand-drawn one has to copy). `ASAuthorizationAppleIDButton` through expo-apple-authentication; null where
// iOS doesn't draw it (Android, the web, Jest), where the sign-in screen draws its own with Apple's logo artwork.
// It only draws the button: the sign-in itself stays Clerk's browser flow until the app is signed with the Sign in
// with Apple entitlement, which a free Apple ID can't sign (roadmap F5).
import * as AppleAuthentication from "expo-apple-authentication";
import { useEffect, useState } from "react";
import { Platform, type StyleProp, type ViewStyle } from "react-native";

export interface AppleButtonProps {
  onPress: () => void;
  scheme: "light" | "dark";
  cornerRadius: number;
  style: StyleProp<ViewStyle>;
}

/** Black in light mode and white in dark (HIG Sign in with Apple, Using the system-provided buttons). */
export function AppleButton({ onPress, scheme, cornerRadius, style }: AppleButtonProps) {
  return (
    <AppleAuthentication.AppleAuthenticationButton
      buttonType={AppleAuthentication.AppleAuthenticationButtonType.CONTINUE}
      buttonStyle={scheme === "dark" ? AppleAuthentication.AppleAuthenticationButtonStyle.WHITE : AppleAuthentication.AppleAuthenticationButtonStyle.BLACK}
      cornerRadius={cornerRadius}
      onPress={onPress}
      style={style}
    />
  );
}

/** Whether iOS draws the system button here; false until it has answered. */
export function useAppleButtonAvailable(): boolean {
  const [available, setAvailable] = useState(false);
  useEffect(() => {
    if (Platform.OS !== "ios") return;
    let alive = true;
    AppleAuthentication.isAvailableAsync()
      .then((ok) => {
        if (alive) setAvailable(ok);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  return available;
}
