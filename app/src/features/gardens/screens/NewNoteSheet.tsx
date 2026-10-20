import { useRouter } from "expo-router";
import { useEffect, useState } from "react";
import { ScrollView } from "react-native";

import { isApiError, useActionKey, useCreateMemory, useGarden } from "@/api";
import { can } from "@/domain/permissions";
import { Button, Text, TextField, space, useTheme } from "@/ui";

import { useActiveGardenId } from "../hooks/useActiveGarden";

// + Note on the garden home (screens 3.1): a note about the whole garden, shared with its members.
export function NewNoteSheet() {
  const { colors } = useTheme();
  const gardenId = useActiveGardenId();
  const garden = useGarden(gardenId);
  const createMemory = useCreateMemory();
  const keyFor = useActionKey();
  const router = useRouter();
  const [text, setText] = useState("");

  // Notes are for editors and owners; anyone else is sent back (screens 4).
  const allowed = garden.data ? can(garden.data.my_role, "memory.note") : undefined;
  useEffect(() => {
    if (allowed === false) router.back();
  }, [allowed, router]);

  async function save() {
    const body = { garden_id: gardenId, text: text.trim() };
    const saved = await createMemory.mutateAsync(body, keyFor(body)).then(() => true, () => false);
    if (saved) router.back();
  }

  return (
    <ScrollView
      style={{ backgroundColor: colors.background }}
      contentContainerStyle={{ padding: space[4], gap: space[4] }}
      keyboardShouldPersistTaps="handled"
    >
      <Text variant="section" accessibilityRole="header">
        A garden note
      </Text>
      <TextField
        label="Note"
        placeholder="The east bed gets shade after 3 pm"
        value={text}
        onChangeText={setText}
        multiline
        autoFocus
        error={createMemory.error ? (isApiError(createMemory.error) ? createMemory.error.message : "The note wasn't saved. Try again.") : undefined}
      />
      <Button label="Save note" size="large" onPress={save} busy={createMemory.isPending} disabled={!text.trim() || allowed !== true} />
    </ScrollView>
  );
}
