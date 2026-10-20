import { useState } from "react";
import { View } from "react-native";

import { isApiError, useCreateGarden, useMe, useUpdateMe } from "@/api";
import type { GardenLocationInput } from "@/domain/types";
import { requestGardenLocation } from "@/platform";
import { Button, Chip, Screen, Text, TextField, space } from "@/ui";

import { locationFromDevice, locationFromText, type ConsentPrecision } from "../model/locationInput";
import { useActiveGardenStore } from "../hooks/activeGardenStore";

// The first garden (screens 3.10; flows 2): a name and where it is, by consent or by postal code or city.
export function OnboardingScreen() {
  const me = useMe();
  const updateMe = useUpdateMe();
  const createGarden = useCreateGarden();
  const setGardenId = useActiveGardenStore((s) => s.setGardenId);

  const needsName = Boolean(me.data && !me.data.display_name);
  const [displayName, setDisplayName] = useState("");
  const [name, setName] = useState("Back garden");
  const [place, setPlace] = useState("");
  const [precision, setPrecision] = useState<ConsentPrecision>("about_1_km");
  const [device, setDevice] = useState<GardenLocationInput | null>(null);
  const [locating, setLocating] = useState(false);
  const [locationRefused, setLocationRefused] = useState(false);

  const location = device ? { ...device, precision } : locationFromText(place);
  const failed = updateMe.error ?? createGarden.error;
  const fieldError = failed ? (isApiError(failed) ? failed.message : "Something went wrong. Try again.") : undefined;

  async function useMyLocation() {
    setLocating(true);
    const position = await requestGardenLocation();
    setLocating(false);
    if (!position) return setLocationRefused(true);
    setDevice(locationFromDevice(position, precision));
  }

  async function submit() {
    if (!location) return;
    if (needsName && displayName.trim()) {
      const named = await updateMe.mutateAsync({ display_name: displayName.trim() }).then(() => true, () => false);
      if (!named) return;
    }
    const garden = await createGarden.mutateAsync({ name: name.trim(), location }).catch(() => null);
    if (garden) setGardenId(garden.id);
  }

  return (
    <Screen>
      <View style={{ paddingTop: space[12], gap: space[2] }}>
        <Text variant="display" accessibilityRole="header">
          Your garden
        </Text>
        <Text variant="body" color="textMuted">
          We use its location for weather and soil. Only the garden has a location.
        </Text>
      </View>

      <View style={{ marginTop: space[8], gap: space[4] }}>
        {needsName ? <TextField label="Your name" value={displayName} onChangeText={setDisplayName} autoComplete="name" /> : null}
        <TextField label="Garden name" value={name} onChangeText={setName} />

        <Text variant="headline">Where is it?</Text>
        {device ? (
          <View style={{ gap: space[3] }}>
            <Text variant="body">Using your location</Text>
            <View style={{ flexDirection: "row", gap: space[2] }}>
              <Chip label="About 1 km" selected={precision === "about_1_km"} onPress={() => setPrecision("about_1_km")} />
              <Chip label="Exact" selected={precision === "exact"} onPress={() => setPrecision("exact")} />
            </View>
            <Button label="Type a postal code instead" variant="plain" onPress={() => setDevice(null)} />
          </View>
        ) : (
          <View style={{ gap: space[3] }}>
            <Button label="Use my location" variant="secondary" onPress={useMyLocation} busy={locating} />
            {locationRefused ? (
              <Text variant="footnote" color="textMuted">
                Location is off for PDM. Type a postal code or city instead.
              </Text>
            ) : null}
            <TextField label="or postal code or city" placeholder="M5V 2T6 or Guelph, ON" value={place} onChangeText={setPlace} autoCapitalize="words" />
          </View>
        )}

        {fieldError ? (
          <Text variant="footnote" color="danger">
            {fieldError}
          </Text>
        ) : null}
        <Button
          label="Create garden"
          size="large"
          onPress={submit}
          busy={createGarden.isPending || updateMe.isPending}
          disabled={!name.trim() || !location || (needsName && !displayName.trim())}
        />
      </View>
    </Screen>
  );
}
