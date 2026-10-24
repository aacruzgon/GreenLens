// Suggested follow-ups under the conversation (screens 3.6; the design's mockup): a tap sends the question. Each
// chip is a full 44 pt touch target.
import { Pressable, View } from "react-native";

import { Text, minTouch, radii, space, useTheme } from "@/ui";

export function SuggestionChips({ questions, disabled, onAsk }: { questions: readonly string[]; disabled: boolean; onAsk: (q: string) => void }) {
  const { colors } = useTheme();
  if (questions.length === 0) return null;
  return (
    <View style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }} testID="suggestions">
      {questions.map((q) => (
        <Pressable
          key={q}
          accessibilityRole="button"
          accessibilityHint="Sends this question"
          accessibilityState={{ disabled }}
          disabled={disabled}
          onPress={() => onAsk(q)}
          style={({ pressed }) => ({
            minHeight: minTouch,
            justifyContent: "center",
            paddingHorizontal: space[3],
            borderRadius: radii.pill,
            borderWidth: 1,
            borderColor: colors.border,
            opacity: disabled ? 0.4 : pressed ? 0.6 : 1,
          })}
        >
          <Text variant="subheadline">{q}</Text>
        </Pressable>
      ))}
    </View>
  );
}
