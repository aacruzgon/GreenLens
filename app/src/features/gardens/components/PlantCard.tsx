// A plant in the home's carousel (styles.css .pcard under data-melt and data-plants="carousel"): the plant's
// photo fills the card and melts into it, with the name in Urbanist at 22 pt in ink and the StatusChip below.
// Without a photo, a leaf stands in (screens 3.1). The photo keeps its height; the card grows with the text at
// large Dynamic Type, and the name wraps.
import { View } from "react-native";
import type { ImageSource } from "expo-image";

import { Card, MeltPhoto, StatusChip, Text, space, statusWords, useTheme, type PlantStatus } from "@/ui";

import { Icon } from "./Icons";

export const PLANT_CARD_WIDTH = 170;
/** The photo's height, and the card's least height. */
export const PLANT_CARD_HEIGHT = 232;

export interface PlantCardProps {
  name: string;
  status: PlantStatus;
  photo: ImageSource | number | null;
  /** "Scan not submitted". */
  note?: string;
  onPress?: () => void;
  testID?: string;
}

function Body({ name, status, note }: Pick<PlantCardProps, "name" | "status" | "note">) {
  return (
    <View style={{ paddingHorizontal: 14, paddingBottom: 14, gap: space[2], alignItems: "flex-start" }}>
      <Text variant="section" accessibilityRole="text">
        {name}
      </Text>
      <StatusChip status={status} />
      {note ? (
        <Text variant="footnote" color="textMuted">
          {note}
        </Text>
      ) : null}
    </View>
  );
}

export function PlantCard({ name, status, photo, note, onPress, testID }: PlantCardProps) {
  const { colors } = useTheme();
  const label = [name, statusWords[status], note].filter(Boolean).join(", ");
  return (
    <Card
      padded={false}
      onPress={onPress}
      accessibilityLabel={label}
      // One element for VoiceOver, so the label above is what it reads (the children are decorative to it).
      accessible
      testID={testID}
      style={{ width: PLANT_CARD_WIDTH, minHeight: PLANT_CARD_HEIGHT }}
    >
      {photo ? (
        <MeltPhoto source={photo} accessibilityLabel="" ground="surface" melt="card" height={PLANT_CARD_HEIGHT}>
          <Body name={name} status={status} note={note} />
        </MeltPhoto>
      ) : (
        <View style={{ flex: 1, minHeight: PLANT_CARD_HEIGHT, justifyContent: "space-between" }}>
          <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
            <Icon name="leaf" color={colors.textMuted} size={44} />
          </View>
          <Body name={name} status={status} note={note} />
        </View>
      )}
    </Card>
  );
}
