// The question on the live analysis (roadmap F3 item 6; design 3.1; flows 6): low and compact, so the spots it asks
// about stay in view between the cards. An indicator question has picture answers for Yes and No and a plain
// Not sure; a photo request opens question mode. Answers are for editors and owners only (contracts 1).
import { View } from "react-native";

import type { Answer, Question } from "@/domain/types";
import { Button, PictureAnswer, Text, camera, radii, space, useTheme } from "@/ui";

import { indicatorArt } from "./IndicatorArt";

export interface QuestionCardProps {
  question: Question;
  canAnswer: boolean;
  /** The option being sent, while its request is in flight. */
  sending?: Answer;
  onAnswer: (answer: Answer) => void;
  onTakePhoto: () => void;
  /** Why the last answer didn't arrive, with Try again inside onAnswer. */
  problem?: string;
}

export function QuestionCard({ question, canAnswer, sending, onAnswer, onTakePhoto, problem }: QuestionCardProps) {
  const { colors } = useTheme();
  const answering = question.status === "answering";
  const busy = sending !== undefined || answering;
  return (
    <View
      testID="question-card"
      accessibilityLiveRegion="polite"
      style={{ paddingTop: 14, paddingHorizontal: 16, paddingBottom: 10, gap: 6, borderRadius: radii.sheet, borderCurve: "continuous", backgroundColor: camera.card, borderWidth: 1, borderColor: camera.hairline }}
    >
      <Text variant="footnote" style={{ color: colors.accent, fontWeight: "600", letterSpacing: 0.8 }}>
        ONE QUESTION
      </Text>
      <Text variant="headline" style={{ fontSize: 19, lineHeight: 24 }} accessibilityRole="header">
        {question.text}
      </Text>
      {question.kind === "photo_request" && question.photo_hint ? (
        <Text variant="subheadline" color="textMuted">
          {question.photo_hint}
        </Text>
      ) : null}

      {answering ? (
        <Text variant="subheadline" color="textMuted" testID="question-answering">
          {question.kind === "photo_request" ? "Checking your photo" : "Checking your answer"}
        </Text>
      ) : !canAnswer ? (
        <Text variant="subheadline" color="textMuted">
          {"An editor or the garden's owner can answer this."}
        </Text>
      ) : question.kind === "photo_request" ? (
        <Button label="Take photo" size="small" onPress={onTakePhoto} style={{ alignSelf: "flex-start", marginTop: space[1] }} testID="take-photo" />
      ) : (
        <IndicatorAnswers question={question} busy={busy} sending={sending} onAnswer={onAnswer} />
      )}

      {problem ? (
        <Text variant="footnote" style={{ color: colors.danger }} accessibilityRole="alert">
          {problem}
        </Text>
      ) : null}
    </View>
  );
}

function IndicatorAnswers({ question, busy, sending, onAnswer }: { question: Question; busy: boolean; sending?: Answer; onAnswer: (a: Answer) => void }) {
  const art = indicatorArt(question.indicator_id);
  const label = (id: Answer, fallback: string) => question.options?.find((o) => o.id === id)?.label ?? fallback;
  return (
    <View style={{ gap: 6, marginTop: 4 }}>
      <View style={{ flexDirection: "row", gap: 10 }}>
        <PictureAnswer
          testID="answer-yes"
          label={art?.yes.label ?? label("yes", "Yes")}
          picture={art?.yes.picture()}
          disabled={busy}
          selected={sending === "yes"}
          onPress={() => onAnswer("yes")}
        />
        <PictureAnswer
          testID="answer-no"
          label={art?.no.label ?? label("no", "No")}
          picture={art?.no.picture()}
          disabled={busy}
          selected={sending === "no"}
          onPress={() => onAnswer("no")}
        />
      </View>
      <Button
        testID="answer-unsure"
        label={label("unsure", "Not sure")}
        variant="plain"
        size="small"
        disabled={busy}
        busy={sending === "unsure"}
        onPress={() => onAnswer("unsure")}
      />
    </View>
  );
}
