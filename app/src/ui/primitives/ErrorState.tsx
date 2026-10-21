// A failed load in place of the content: what went wrong in plain words and a retry. The retry is a
// secondary pill, since it is not the screen's main action. Server text is shown as sent (frontend README 14).
import { View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Circle, Path } from "react-native-svg";

import { hiddenSvg } from "../svgId";
import { useTheme } from "../theme";
import { space } from "../tokens";
import { Button } from "./Button";
import { Text } from "./Text";

export interface ErrorStateProps {
  title?: string;
  message?: string;
  onRetry?: () => void;
  retryLabel?: string;
  busy?: boolean;
  style?: StyleProp<ViewStyle>;
}

export function ErrorState({ title = "Something went wrong", message, onRetry, retryLabel = "Try again", busy, style }: ErrorStateProps) {
  const { colors } = useTheme();
  return (
    <View
      accessibilityRole="alert"
      style={[{ alignItems: "center", gap: space[3], paddingVertical: space[10], paddingHorizontal: space[6] }, style]}
    >
      <Svg width={32} height={32} viewBox="0 0 32 32" {...hiddenSvg}>
        <Circle cx={16} cy={16} r={14} stroke={colors.danger} strokeWidth={2} fill="none" />
        <Path d="M16 9v9M16 22.5v.5" stroke={colors.danger} strokeWidth={2.5} strokeLinecap="round" />
      </Svg>
      <Text variant="title3" emphasis align="center">
        {title}
      </Text>
      {message ? (
        <Text variant="body" color="textMuted" align="center">
          {message}
        </Text>
      ) : null}
      {onRetry ? (
        <Button variant="secondary" label={retryLabel} onPress={onRetry} busy={busy} style={{ alignSelf: "center", marginTop: space[2] }} />
      ) : null}
    </View>
  );
}
