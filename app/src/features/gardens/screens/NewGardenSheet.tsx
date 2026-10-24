import { useRouter } from "expo-router";
import { useState } from "react";

import { isApiError, useActionKey, useCreateGarden } from "@/api";
import { Button, Sheet, Text, TextField } from "@/ui";

import { LocationPicker } from "../components/LocationPicker";
import { useActiveGardenStore } from "../hooks/activeGardenStore";
import { useLocationInput } from "../hooks/useLocationInput";

// gardens/new from Settings (screens 3.9): onboarding's name and location, as a sheet. The new garden becomes
// the active one and the garden home opens on it.
export function NewGardenSheet() {
  const router = useRouter();
  const create = useCreateGarden();
  const keyFor = useActionKey();
  const setGardenId = useActiveGardenStore((s) => s.setGardenId);
  const where = useLocationInput();
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<string>();

  const submit = () => {
    if (!where.location || !name.trim()) return;
    const body = { name: name.trim(), location: where.location };
    setProblem(undefined);
    create.mutateAsync(body, keyFor(body)).then(
      (garden) => {
        setGardenId(garden.id);
        router.dismissTo("/");
      },
      (e: unknown) => setProblem(isApiError(e) ? e.message : "The garden wasn't made. Try again."),
    );
  };

  return (
    <Sheet
      title="New garden"
      footer={<Button label="Create garden" onPress={submit} busy={create.isPending} disabled={!name.trim() || !where.location} testID="garden-create" />}
    >
      <TextField label="Garden name" placeholder="Allotment" value={name} onChangeText={setName} autoFocus testID="new-garden-name" />
      <LocationPicker input={where} />
      <Text variant="footnote" color="textMuted">
        We use its location for weather and soil. Only the garden has a location.
      </Text>
      {problem ? (
        <Text variant="footnote" color="danger">
          {problem}
        </Text>
      ) : null}
    </Sheet>
  );
}
