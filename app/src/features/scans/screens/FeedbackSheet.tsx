// "Was this result right?" (roadmap 10 item 10; screens 3.11): reviewers, editors and owners, once the scan is
// submitted. The verdict, and for a wrong result what it was (a problem's name or free text), and notes.
import { useLocalSearchParams, useRouter } from "expo-router";
import { useState } from "react";
import { View } from "react-native";

import { useActionKey, useGarden, useScan, useScanFeedback } from "@/api";
import { can } from "@/domain/permissions";
import type { FeedbackRequest, Verdict } from "@/domain/types";
import { haptics } from "@/platform";
import { Button, Chip, Sheet, Text, TextField, space } from "@/ui";

const VERDICTS: { value: Verdict; label: string }[] = [
  { value: "correct", label: "Correct" },
  { value: "wrong", label: "Wrong" },
  { value: "unsure", label: "Unsure" },
];

const MAX_LABEL = 120;
const MAX_NOTES = 2000;

export function FeedbackSheet() {
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  const router = useRouter();
  const scan = useScan(scanId);
  const garden = useGarden(scan.data?.garden_id ?? "");
  const send = useScanFeedback(scanId);
  const keyFor = useActionKey();
  const [verdict, setVerdict] = useState<Verdict>();
  const [label, setLabel] = useState("");
  const [notes, setNotes] = useState("");
  const [problem, setProblem] = useState<string>();

  const allowed = can(garden.data?.my_role, "scan.feedback") && scan.data?.status !== "pending";

  const submit = () => {
    if (!verdict) return;
    const body: FeedbackRequest = { verdict };
    if (verdict === "wrong" && label.trim()) body.corrected_label = label.trim();
    if (notes.trim()) body.notes = notes.trim();
    setProblem(undefined);
    send
      .mutateAsync(body, keyFor(body))
      .then(() => {
        void haptics.success();
        router.back();
      })
      .catch(() => setProblem("Your feedback didn't send. Try again."));
  };

  return (
    <Sheet
      title="Was this result right?"
      footer={allowed ? <Button label="Send" disabled={!verdict} busy={send.isPending} onPress={submit} testID="feedback-send" /> : undefined}
    >
      {!allowed && garden.data ? (
        <Text variant="body" color="textMuted">
          Feedback is for reviewers, editors and owners, once the scan is submitted.
        </Text>
      ) : (
        <>
          <View accessibilityRole="radiogroup" style={{ flexDirection: "row", gap: space[2] }}>
            {VERDICTS.map((v) => (
              <Chip key={v.value} label={v.label} selected={verdict === v.value} onPress={() => setVerdict(v.value)} />
            ))}
          </View>
          {verdict === "wrong" ? (
            <TextField label="What was it?" placeholder="The problem's name, if you know it" value={label} onChangeText={setLabel} maxLength={MAX_LABEL} testID="feedback-label" />
          ) : null}
          <TextField label="Notes" placeholder="Anything else we should know" value={notes} onChangeText={setNotes} maxLength={MAX_NOTES} multiline testID="feedback-notes" />
          {problem ? (
            <Text variant="footnote" color="danger" accessibilityRole="alert">
              {problem}
            </Text>
          ) : null}
        </>
      )}
    </Sheet>
  );
}
