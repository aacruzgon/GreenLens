import { forwardRef } from "react";
import { TextInput, View, type TextInputProps } from "react-native";

import { useTheme } from "../theme";
import { MIN_TOUCH } from "../tokens";
import { Text } from "./Text";

export interface TextFieldProps extends TextInputProps {
  label: string;
  error?: string;
  hint?: string;
}

export const TextField = forwardRef<TextInput, TextFieldProps>(function TextField({ label, error, hint, style, ...rest }, ref) {
  const t = useTheme();
  return (
    <View style={{ gap: t.space.xs }}>
      <Text variant="subhead" weight="600">
        {label}
      </Text>
      <TextInput
        ref={ref}
        accessibilityLabel={label}
        accessibilityHint={hint}
        placeholderTextColor={t.colors.textMuted}
        style={[
          t.type.body,
          {
            minHeight: MIN_TOUCH + 4,
            color: t.colors.text,
            backgroundColor: t.colors.surface,
            borderColor: error ? t.colors.danger : t.colors.border,
            borderWidth: 1,
            borderRadius: t.radius.md,
            paddingHorizontal: t.space.md,
          },
          style,
        ]}
        {...rest}
      />
      {error ? (
        <Text variant="footnote" color="danger" accessibilityLiveRegion="polite">
          {error}
        </Text>
      ) : hint ? (
        <Text variant="footnote" color="textMuted">
          {hint}
        </Text>
      ) : null}
    </View>
  );
});
