// Garden notes (screens 3.1): memories with no plant, shared ones plus the caller's own, with **+ Note**
// for editors and owners. The note's own text is shown as stored.
import { View } from "react-native";

import { Button, Card, ErrorState, Section, Skeleton, Text, space, useTheme } from "@/ui";

import type { Loadable } from "../model/views";

export interface GardenNotesProps {
  /** Loading shows a skeleton, an error a retry; the notes are shown once ready. */
  state: Loadable<{ items: { id: string; text: string }[] }>;
  /** Adds a note; absent for viewers (screens 4). */
  onAddNote?: () => void;
}

export function GardenNotes({ state, onAddNote }: GardenNotesProps) {
  const { colors } = useTheme();
  if (state.status === "loading") {
    return (
      <Section title="Garden notes">
        <Card testID="notes-loading" accessibilityLabel="Loading the garden notes" style={{ gap: space[2] }}>
          <Skeleton height={16} width="85%" />
          <Skeleton height={16} width="60%" />
        </Card>
      </Section>
    );
  }
  if (state.status === "error") {
    return (
      <Section title="Garden notes">
        <ErrorState title="Couldn't load the notes" message={state.message} onRetry={state.retry} />
      </Section>
    );
  }
  const notes = state.items;
  return (
    <Section title="Garden notes">
      <Card style={{ gap: space[2] }}>
        {notes.length === 0 ? (
          <Text variant="subheadline" color="textMuted">
            Notes about the whole garden go here, such as where the shade falls.
          </Text>
        ) : (
          notes.map((n, i) => (
            <View
              key={n.id}
              style={{
                paddingTop: i === 0 ? 0 : space[2],
                borderTopWidth: i === 0 ? 0 : 1,
                borderTopColor: colors.border,
              }}
            >
              <Text variant="subheadline">{n.text}</Text>
            </View>
          ))
        )}
        {onAddNote ? <Button variant="secondary" label="+ Note" accessibilityLabel="Add a note" onPress={onAddNote} /> : null}
      </Card>
    </Section>
  );
}
