// The species check's sheet (flows 7; screens 3.11): "Is this a tomato?" with Yes when the models agree, the top
// three with their photos and credits when they don't, and a search over the knowledge base's species. A species
// the knowledge base doesn't have is typed as a name (`custom_name`, at most 80 characters). Opened from the live
// analysis's "Tomato · Change" and, in F4, from the plant page. A changed species reruns the plant's latest scan.
import { Image } from "expo-image";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useDeferredValue, useRef, useState } from "react";
import { ActivityIndicator, View } from "react-native";

import { flattenPages, imageSource, useConfirmSpecies, usePlant, useSpecies } from "@/api";
import type { ConfirmSpeciesRequest, SpeciesCandidate } from "@/domain/types";
import { Button, ListItem, Sheet, Text, TextField, radii, space } from "@/ui";

const MAX_CUSTOM = 80;
const TOP = 3;

export function SpeciesSheet() {
  const { plantId } = useLocalSearchParams<{ plantId: string }>();
  const router = useRouter();
  const plant = usePlant(plantId);
  const confirm = useConfirmSpecies(plantId);
  const [query, setQuery] = useState("");
  const q = useDeferredValue(query.trim());
  const results = useSpecies(q || undefined);
  const [problem, setProblem] = useState<string>();
  // One change at a time: each one reruns the plant's latest scan, and isPending lags a quick second tap.
  const sending = useRef(false);

  const choose = (body: ConfirmSpeciesRequest) => {
    if (sending.current) return;
    sending.current = true;
    setProblem(undefined);
    confirm.mutate(body, {
      onSuccess: () => router.back(),
      onError: () => setProblem("The species wasn't saved. Try again."),
      onSettled: () => {
        sending.current = false;
      },
    });
  };

  const p = plant.data;
  const busy = confirm.isPending;
  const candidates: SpeciesCandidate[] = p?.species_candidates.slice(0, TOP) ?? [];
  const current = p?.species;
  const askYes = current !== undefined && candidates.length === 0;
  // What the sheet already offers above the search isn't listed again.
  const offered = new Set([...candidates.map((c) => c.species_id), askYes ? current.id : undefined]);
  const list = flattenPages(results.data).filter((s) => !offered.has(s.id));

  return (
    <Sheet title={askYes ? `Is this a ${current.common_name.toLowerCase()}?` : "Which plant is this?"}>
      {!p ? <ActivityIndicator /> : null}
      {problem ? (
        <Text variant="footnote" color="danger" accessibilityRole="alert" testID="species-problem">
          {problem}
        </Text>
      ) : null}
      {askYes ? (
        <Button label={`Yes, ${current.common_name.toLowerCase()}`} busy={busy} onPress={() => choose({ species_id: current.id })} testID="species-yes" />
      ) : null}
      {candidates.map((c) => (
        <Candidate key={c.species_id ?? c.name} candidate={c} disabled={busy} onPress={() => choose(c.species_id ? { species_id: c.species_id } : { custom_name: c.name })} />
      ))}

      <TextField
        label="Something else"
        placeholder="Search, or type the plant's name"
        value={query}
        onChangeText={setQuery}
        maxLength={MAX_CUSTOM}
        autoCorrect={false}
        returnKeyType="done"
        testID="species-search"
      />
      {list.slice(0, 20).map((s) => (
        <ListItem
          key={s.id}
          title={s.common_name}
          subtitle={s.scientific_name}
          onPress={busy ? undefined : () => choose({ species_id: s.id })}
          leading={<Thumb url={s.reference_images[0]?.url} />}
        />
      ))}
      {q && !results.isFetching && list.length === 0 ? (
        <Button label={`Use "${q}"`} variant="secondary" busy={busy} onPress={() => choose({ custom_name: q })} testID="species-custom" />
      ) : null}
    </Sheet>
  );
}

/** A candidate with its reference photo and the photo's credit under it, always together (contracts 3). */
function Candidate({ candidate, disabled, onPress }: { candidate: SpeciesCandidate; disabled: boolean; onPress: () => void }) {
  const photo = candidate.reference_image;
  return (
    <View style={{ gap: space[1] }}>
      <ListItem title={candidate.name} onPress={disabled ? undefined : onPress} leading={<Thumb url={photo?.url} />} />
      {photo ? (
        <Text variant="caption1" color="textMuted" style={{ marginLeft: space[4] }}>
          Photo: {photo.credit}
        </Text>
      ) : null}
    </View>
  );
}

function Thumb({ url }: { url?: string }) {
  if (!url) return null;
  return <Image source={imageSource(url)} contentFit="cover" style={{ width: 44, height: 44, borderRadius: radii.small }} accessibilityIgnoresInvertColors />;
}
