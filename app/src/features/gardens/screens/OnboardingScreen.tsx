import { useState } from "react";
import { KeyboardAvoidingView, Platform, View } from "react-native";
import { useQueryClient } from "@tanstack/react-query";

import { describeError, gardens, me, qk, useIdempotentMutation, useMe } from "@/api";
import { useSession } from "@/features/auth";
import { currentLocation } from "@/platform";
import { Banner, Button, Card, Screen, Text, TextField, useTheme } from "@/ui";

import { useActiveGardenStore } from "../hooks/activeGardenStore";
import { parsePlace, toLocationInput, type LocationChoice } from "../model/locationInput";

/** Onboarding (screens.md 3.10): a display name, the first garden, and its location with consent. */
export function OnboardingScreen() {
  const t = useTheme();
  const queryClient = useQueryClient();
  const session = useSession();
  const profile = useMe();
  const setActive = useActiveGardenStore((s) => s.setGardenId);
  const [name, setName] = useState(profile.data?.display_name || session.suggestedName || "");
  const [gardenName, setGardenName] = useState("My garden");
  const [place, setPlace] = useState("");
  const [device, setDevice] = useState<LocationChoice | null>(null);
  const [locating, setLocating] = useState(false);
  const [locationNote, setLocationNote] = useState<string>();

  const location = device ?? parsePlace(place);

  const create = useIdempotentMutation(
    async (_: void, key) => {
      if (name.trim() && name.trim() !== profile.data?.display_name) {
        await me.update({ display_name: name.trim(), ...(device ? { location_consent: true } : {}) });
      } else if (device) {
        await me.update({ location_consent: true });
      }
      return gardens.create({ name: gardenName.trim(), location: toLocationInput(location!) }, key);
    },
    {
      onSuccess: async (g) => {
        setActive(g.id);
        await queryClient.invalidateQueries({ queryKey: qk.me() });
        await queryClient.invalidateQueries({ queryKey: qk.gardens() });
      },
    },
  );

  async function useMyLocation() {
    setLocating(true);
    setLocationNote(undefined);
    const r = await currentLocation();
    setLocating(false);
    if (r.kind === "ok") {
      setDevice({ kind: "device", lat: r.lat, lon: r.lon });
      setLocationNote("Using your location, rounded to about 1 km.");
    } else {
      setLocationNote(r.kind === "denied" ? "No problem. Enter a postal code or city instead." : "We couldn't get your location. Enter a postal code or city.");
    }
  }

  const error = create.error ? describeError(create.error) : undefined;

  return (
    <KeyboardAvoidingView style={{ flex: 1 }} behavior={Platform.OS === "ios" ? "padding" : undefined}>
      <Screen
        padTop
        footer={
          <Button
            label="Create garden"
            size="large"
            onPress={() => create.mutate()}
            loading={create.isPending}
            disabled={!gardenName.trim() || !location}
          />
        }
      >
        <View style={{ gap: t.space.sm, paddingTop: t.space.xl }}>
          <Text variant="display" accessibilityRole="header">
            Set up your garden
          </Text>
          <Text color="textMuted">We use its location for the weather and soil, which shape every diagnosis.</Text>
        </View>

        <TextField label="Your name" value={name} onChangeText={setName} autoComplete="name" hint="Shown to people you share a garden with." />
        <TextField label="Garden name" value={gardenName} onChangeText={setGardenName} />

        <Card style={{ gap: t.space.md }}>
          <Text variant="headline">Where is it?</Text>
          {device ? (
            <Banner tone="accent" icon="location" title="Location set" body={locationNote} action={{ label: "Enter a place instead", onPress: () => setDevice(null) }} />
          ) : (
            <>
              <Button label="Use my location" icon="location" variant="secondary" onPress={useMyLocation} loading={locating} />
              <Text variant="footnote" color="textMuted" align="center">
                or
              </Text>
              <TextField
                label="Postal code or city"
                value={place}
                onChangeText={setPlace}
                autoCapitalize="characters"
                placeholder="M5V 3L9, or Guelph, ON"
                hint={locationNote ?? "Only the garden stores a location, never your account."}
              />
            </>
          )}
        </Card>

        {error ? <Banner tone="danger" title={error.title} body={error.message} /> : null}
      </Screen>
    </KeyboardAvoidingView>
  );
}
