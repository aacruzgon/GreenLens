// A forecast alert (screens 3.1, alert inbox; styles.css .banner): an open forecast_alert task on the watch
// fill with the snowflake at 22 pt. **Done** is a small ghost pill (.btn-ghost.btn-sm: the surface at 80%, 10 pt
// under the text), shown to editors and owners only; the home's one green pill is the scan button.
// The mockup's plain **View** opens the Tasks list; Tasks joins in F4 and no route exists yet, so View is omitted.
import { View } from "react-native";

import { Button, Text, radii, space, useTheme } from "@/ui";

import { Icon } from "./Icons";

export interface AlertCardProps {
  title: string;
  detail?: string;
  /** Completes the alert's task; absent for viewers (screens 4). */
  onDone?: () => void;
  testID?: string;
}

export function AlertCard({ title, detail, onDone, testID }: AlertCardProps) {
  const { colors } = useTheme();
  return (
    <View
      testID={testID}
      accessibilityRole="summary"
      style={{
        flexDirection: "row",
        gap: space[3],
        paddingVertical: 14,
        paddingHorizontal: space[4],
        borderRadius: radii.card,
        borderCurve: "continuous",
        backgroundColor: colors.watchSoft,
      }}
    >
      <View style={{ marginTop: 1 }}>
        <Icon name="snow" color={colors.watch} size={22} />
      </View>
      <View style={{ flex: 1 }}>
        <Text variant="headline">{title}</Text>
        {detail ? <Text variant="subheadline">{detail}</Text> : null}
        {onDone ? (
          <View style={{ flexDirection: "row", gap: space[2], marginTop: 10 }}>
            <Button label="Done" variant="ghost" size="small" onPress={onDone} accessibilityLabel={`Done: ${title}`} />
          </View>
        ) : null}
      </View>
    </View>
  );
}
