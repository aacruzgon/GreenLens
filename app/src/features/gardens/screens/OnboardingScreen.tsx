import { useState } from "react";
import { View } from "react-native";

import { isApiError, useCreateGarden, useMe, useUpdateMe } from "@/api";
import { Button, Screen, Text, TextField, space } from "@/ui";

import { LocationPicker } from "../components/LocationPicker";
import { useActiveGardenStore } from "../hooks/activeGardenStore";
import { useLocationInput } from "../hooks/useLocationInput";

// The first garden (screens 3.10; flows 2): a name and where it is, by consent or by postal code or city.
export function OnboardingScreen() {
  const me = useMe();
  const updateMe = useUpdateMe();
  const createGarden = useCreateGarden();
  const setGardenId = useActiveGardenStore((s) => s.setGardenId);

  const needsName = Boolean(me.data && !me.data.display_name);
  const [displayName, setDisplayName] = useState("");
  const [name, setName] = useState("Back garden");
  const where = useLocationInput();
  const location = where.location;
  const failed = updateMe.error ?? createGarden.error;
  const fieldError = failed ? (isApiError(failed) ? failed.message : "Something went wrong. Try again.") : undefined;

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

        <LocationPicker input={where} />

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
