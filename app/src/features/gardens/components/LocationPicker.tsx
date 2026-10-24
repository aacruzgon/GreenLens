// "Where is it?" (screens 3.10): the phone's location, exact or about 1 km, or a postal code or city.
import { View } from "react-native";

import { Button, Chip, Text, TextField, space } from "@/ui";

import type { LocationInputState } from "../hooks/useLocationInput";

export function LocationPicker({ input, title = "Where is it?" }: { input: LocationInputState; title?: string }) {
  return (
    <View style={{ gap: space[3] }}>
      <Text variant="headline">{title}</Text>
      {input.usingDevice ? (
        <View style={{ gap: space[3] }}>
          <Text variant="body">Using your location</Text>
          <View style={{ flexDirection: "row", gap: space[2] }}>
            <Chip label="About 1 km" selected={input.precision === "about_1_km"} onPress={() => input.setPrecision("about_1_km")} />
            <Chip label="Exact" selected={input.precision === "exact"} onPress={() => input.setPrecision("exact")} />
          </View>
          <Button label="Type a postal code instead" variant="plain" onPress={input.typeInstead} />
        </View>
      ) : (
        <View style={{ gap: space[3] }}>
          <Button label="Use my location" variant="secondary" onPress={input.useMyLocation} busy={input.locating} />
          {input.refused ? (
            <Text variant="footnote" color="textMuted">
              Location is off for PDM. Type a postal code or city instead.
            </Text>
          ) : null}
          <TextField
            label="or postal code or city"
            placeholder="M5V 2T6 or Guelph, ON"
            value={input.place}
            onChangeText={input.setPlace}
            autoCapitalize="words"
          />
        </View>
      )}
    </View>
  );
}
