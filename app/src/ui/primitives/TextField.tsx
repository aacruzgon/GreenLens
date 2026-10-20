// A labelled text field: the label above, the field on the surface, the error below in the danger colour.
import { useState, type Ref } from "react";
import { TextInput, View, type StyleProp, type TextInputProps, type ViewStyle } from "react-native";

import { useTheme } from "../theme";
import { minTouch, radii, space, textStyles } from "../tokens";
import { Text } from "./Text";

export interface TextFieldProps extends Omit<TextInputProps, "style"> {
  label: string;
  hint?: string;
  error?: string;
  ref?: Ref<TextInput>;
  style?: StyleProp<ViewStyle>;
}

export function TextField({ label, hint, error, ref, style, onFocus, onBlur, editable = true, ...rest }: TextFieldProps) {
  const { colors } = useTheme();
  const [focused, setFocused] = useState(false);
  return (
    <View style={[{ gap: space[1] + 2 }, style]}>
      <Text variant="subheadline" color="textMuted" emphasis>
        {label}
      </Text>
      <TextInput
        {...rest}
        ref={ref}
        editable={editable}
        accessibilityLabel={label}
        accessibilityHint={error ?? hint}
        accessibilityState={{ disabled: !editable }}
        placeholderTextColor={colors.textMuted}
        selectionColor={colors.accent}
        onFocus={(e) => {
          setFocused(true);
          onFocus?.(e);
        }}
        onBlur={(e) => {
          setFocused(false);
          onBlur?.(e);
        }}
        style={{
          ...textStyles.body,
          lineHeight: undefined,
          color: colors.text,
          minHeight: minTouch + 6,
          paddingHorizontal: space[4],
          paddingVertical: space[3],
          borderRadius: radii.field,
          borderCurve: "continuous",
          backgroundColor: colors.surface,
          borderWidth: focused || error ? 2 : 1,
          borderColor: error ? colors.danger : focused ? colors.text : colors.border,
          opacity: editable ? 1 : 0.6,
        }}
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
}
