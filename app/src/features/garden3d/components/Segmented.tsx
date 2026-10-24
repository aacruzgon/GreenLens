// The garden's two switches (design 3.1, the 3D garden): the mode bar HEALTH · GROWTH · FROST, and 3D / Map at the
// top right. Apple's segmented control, from SwiftUI, with the chosen one in the text colour on the system's own
// selection; the mode bar sits on Liquid Glass like capture's, which iOS draws (design 6). Where SwiftUI isn't
// there (the web), a solid pill drawn here, never an imitation of glass.
import { Host, Picker, Text as SwiftText } from "@expo/ui/swift-ui";
import { accessibilityLabel, fixedSize, glassEffect, padding, pickerStyle, tag } from "@expo/ui/swift-ui/modifiers";
import { Platform, Pressable, View } from "react-native";

import { Text, radii, useGlass, useTheme } from "@/ui";

export interface SegmentedProps<T extends string> {
  items: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
  /** What the control chooses, for VoiceOver: "Garden mode". */
  label: string;
  /** The mode bar sits on glass; the view switch is a plain control. */
  glass?: boolean;
  testID?: string;
}

export function Segmented<T extends string>({ items, value, onChange, label, glass = false, testID }: SegmentedProps<T>) {
  const { scheme } = useTheme();
  const onGlass = useGlass() && glass;
  if (Platform.OS === "ios" && process.env.NODE_ENV !== "test") {
    return (
      <Host matchContents colorScheme={scheme} testID={testID}>
        <Picker
          selection={value}
          onSelectionChange={(v: T) => onChange(v)}
          modifiers={[
            pickerStyle("segmented"),
            fixedSize(),
            accessibilityLabel(label),
            // Not `interactive`: interactive glass around the picker took its taps on the iOS 27 simulator.
            ...(onGlass ? [padding({ horizontal: 4, vertical: 4 }), glassEffect({ glass: { variant: "regular" }, shape: "capsule" })] : []),
          ]}
        >
          {items.map((m) => (
            <SwiftText key={m.value} modifiers={[tag(m.value)]}>
              {m.label}
            </SwiftText>
          ))}
        </Picker>
      </Host>
    );
  }
  return <DrawnSegmented items={items} value={value} onChange={onChange} label={label} testID={testID} />;
}

function DrawnSegmented<T extends string>({ items, value, onChange, label, testID }: SegmentedProps<T>) {
  const { colors } = useTheme();
  return (
    <View
      accessibilityRole="tablist"
      accessibilityLabel={label}
      testID={testID}
      style={{ flexDirection: "row", alignSelf: "center", padding: 3, gap: 2, borderRadius: radii.pill, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.border }}
    >
      {items.map((m) => {
        const on = m.value === value;
        return (
          <Pressable
            key={m.value}
            accessibilityRole="tab"
            accessibilityState={{ selected: on }}
            accessibilityLabel={m.label}
            onPress={() => onChange(m.value)}
            testID={testID ? `${testID}-${m.value}` : undefined}
            style={{ minHeight: 32, paddingHorizontal: 14, justifyContent: "center", borderRadius: radii.pill, backgroundColor: on ? colors.surfaceMuted : "transparent" }}
          >
            <Text variant="footnote" emphasis color={on ? "text" : "textMuted"}>
              {m.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}
