// "Remembered: …" after `memory_saved` (screens 3.6; flows 10): Undo deletes it, and Share with garden, for
// editors and owners, makes it a shared memory the plan writer reads. A chat memory is private until shared.
import { useState } from "react";
import { View } from "react-native";

import { useDeleteMemory, useShareMemory } from "@/api";
import type { Memory } from "@/domain/types";
import { Button, Card, Text, space } from "@/ui";

export interface MemoryCardProps {
  memory: Memory;
  canShare: boolean;
}

type State = "saved" | "undone" | "shared";

export function MemoryCard({ memory, canShare }: MemoryCardProps) {
  const undo = useDeleteMemory();
  const share = useShareMemory();
  const [state, setState] = useState<State>(memory.shared ? "shared" : "saved");
  const failed = undo.isError || share.isError;

  if (state === "undone") {
    return (
      <Text variant="footnote" color="textMuted" accessibilityLiveRegion="polite">
        Forgotten: {memory.text}
      </Text>
    );
  }
  return (
    <Card style={{ gap: space[2] }} testID={`memory-${memory.id}`}>
      <Text variant="subheadline">
        <Text variant="subheadline" emphasis>
          Remembered:
        </Text>{" "}
        {memory.text}
      </Text>
      {state === "shared" ? (
        <Text variant="footnote" color="textMuted" accessibilityLiveRegion="polite">
          Shared with the garden
        </Text>
      ) : (
        <Text variant="footnote" color="textMuted">
          Only you see this until you share it.
        </Text>
      )}
      {failed ? (
        <Text variant="footnote" color="danger">
          That didn&apos;t go through. Try again.
        </Text>
      ) : null}
      {state === "saved" ? (
        <View style={{ flexDirection: "row", justifyContent: "flex-end", gap: space[2] }}>
          <Button label="Undo" variant="secondary" size="small" busy={undo.isPending} onPress={() => undo.mutate(memory, { onSuccess: () => setState("undone") })} />
          {canShare ? (
            <Button label="Share with garden" variant="secondary" size="small" busy={share.isPending} onPress={() => share.mutate(memory.id, { onSuccess: () => setState("shared") })} />
          ) : null}
        </View>
      ) : null}
    </Card>
  );
}
