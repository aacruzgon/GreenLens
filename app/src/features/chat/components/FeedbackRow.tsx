// Thumbs up and down on a finished assistant message (screens 3.6; flows 9): a tap sends
// `POST /messages/{id}/feedback`. A thumbs-down opens an optional note before sending.
import { useState } from "react";
import { Pressable, View } from "react-native";

import { useMessageFeedback } from "@/api";
import type { Rating } from "@/domain/types";
import { Button, LineIcon, Text, TextField, minTouch, space, useTheme } from "@/ui";

export function FeedbackRow({ messageId }: { messageId: string }) {
  const { colors } = useTheme();
  const feedback = useMessageFeedback();
  const [sent, setSent] = useState<Rating>();
  const [noting, setNoting] = useState(false);
  const [notes, setNotes] = useState("");

  const send = (rating: Rating, note?: string) =>
    feedback.mutate(
      { messageId, body: { rating, ...(note?.trim() ? { notes: note.trim() } : {}) } },
      {
        onSuccess: () => {
          setSent(rating);
          setNoting(false);
        },
      },
    );

  const thumb = (rating: Rating) => {
    const chosen = sent === rating;
    return (
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={rating === "up" ? "Good answer" : "Bad answer"}
        accessibilityState={{ selected: chosen, disabled: sent !== undefined }}
        disabled={sent !== undefined || feedback.isPending}
        onPress={() => (rating === "up" ? send("up") : setNoting(true))}
        testID={`feedback-${rating}`}
        style={({ pressed }) => ({ width: minTouch, height: minTouch, alignItems: "center", justifyContent: "center", opacity: pressed ? 0.6 : sent && !chosen ? 0.35 : 1 })}
      >
        <LineIcon name={rating === "up" ? "thumbUp" : "thumbDown"} color={chosen ? colors.text : colors.textMuted} size={18} />
      </Pressable>
    );
  };

  return (
    <View style={{ gap: space[2] }}>
      <View style={{ flexDirection: "row", justifyContent: "flex-end", alignItems: "center" }}>
        {sent ? (
          <Text variant="caption1" color="textMuted" accessibilityLiveRegion="polite" style={{ marginRight: space[1] }}>
            Thanks for the feedback
          </Text>
        ) : null}
        {thumb("up")}
        {thumb("down")}
      </View>
      {noting && !sent ? (
        <View style={{ gap: space[2] }}>
          <TextField label="What was wrong? (optional)" value={notes} onChangeText={setNotes} multiline maxLength={1000} />
          <View style={{ flexDirection: "row", justifyContent: "flex-end", gap: space[2] }}>
            <Button label="Cancel" variant="secondary" size="small" onPress={() => setNoting(false)} />
            <Button label="Send" size="small" onPress={() => send("down", notes)} busy={feedback.isPending} />
          </View>
        </View>
      ) : null}
      {feedback.isError ? (
        <Text variant="footnote" color="danger">
          The feedback wasn&apos;t sent. Try again.
        </Text>
      ) : null}
    </View>
  );
}
