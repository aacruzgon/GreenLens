// Controls shared by the capture screens (screens 3.2): a shot's problem line and the white action at the right of
// the shutter (Analyse in capture, Send in question and guided mode). Neutral on the photo: white on black (design 2).
import { ActivityIndicator, Pressable, View } from "react-native";

import type { QualityHint } from "@/domain/types";
import { Text, camera, minTouch, space } from "@/ui";

import { CaptureIcon } from "./CaptureIcons";

const HINT_WORDS: Record<QualityHint, string> = { tap_to_focus: "Tap to focus", more_light: "More light", avoid_glare: "Avoid glare" };

/** A refused or failed shot: the server's message, with its hint's icon, and an action such as Try again. */
export function ProblemLine({ message, hint, action }: { message: string; hint?: QualityHint; action?: { label: string; onPress: () => void } }) {
  return (
    <View accessibilityRole="alert" accessibilityLiveRegion="polite" testID="shot-problem" style={{ flexDirection: "row", gap: space[3], alignItems: "flex-start" }}>
      {hint ? (
        <View accessibilityLabel={HINT_WORDS[hint]} style={{ paddingTop: 1 }}>
          <CaptureIcon name={hint} color={camera.ink} />
        </View>
      ) : null}
      <View style={{ flex: 1, gap: space[1] }}>
        <Text variant="subheadline" style={{ color: camera.ink }}>
          {message}
        </Text>
        {action ? (
          <Pressable accessibilityRole="button" onPress={action.onPress} hitSlop={8} style={{ minHeight: 32, justifyContent: "center", alignSelf: "flex-start" }}>
            <Text variant="subheadline" style={{ color: camera.ink, fontWeight: "600", textDecorationLine: "underline" }}>
              {action.label}
            </Text>
          </Pressable>
        ) : null}
      </View>
    </View>
  );
}

/** The white action on the black control area: Analyse or Send. The hint says why it's disabled (HIG Feedback). */
export function CameraAction({
  label,
  enabled,
  busy,
  onPress,
  disabledHint,
  testID,
}: {
  label: string;
  enabled: boolean;
  busy: boolean;
  onPress: () => void;
  disabledHint: string;
  testID: string;
}) {
  const disabled = !enabled || busy;
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled, busy }}
      accessibilityHint={enabled ? undefined : disabledHint}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({
        minHeight: minTouch,
        paddingHorizontal: 18,
        borderRadius: 999,
        justifyContent: "center",
        backgroundColor: camera.ink,
        opacity: disabled ? 0.4 : pressed ? 0.8 : 1,
      })}
    >
      {busy ? (
        <ActivityIndicator color={camera.onInk} />
      ) : (
        <Text variant="headline" style={{ color: camera.onInk }}>
          {label}
        </Text>
      )}
    </Pressable>
  );
}
