// A provisional plan's open question, at the top of the plan (screens 3.4). On the diagnosis it is a card on the
// tint with text answers; the picture answers belong to the live analysis's camera layout. Answers are for editors
// and owners; a viewer reads the question only.
import { View } from "react-native";

import type { Answer, Question } from "@/domain/types";
import { Button, Text, space } from "@/ui";

import { TintCard } from "./TintCard";

export interface PlanQuestionProps {
  question: Question;
  canAnswer: boolean;
  sending?: Answer;
  problem?: string;
  fill: string;
  onAnswer: (answer: Answer) => void;
}

export function PlanQuestion({ question, canAnswer, sending, problem, fill, onAnswer }: PlanQuestionProps) {
  const busy = sending !== undefined || question.status === "answering";
  const options = question.kind === "indicator" ? (question.options ?? []) : [];
  return (
    <TintCard fill={fill} style={{ padding: space[4], gap: space[3] }} testID="plan-question">
      <Text variant="headline" accessibilityRole="header">
        {question.text}
      </Text>
      {canAnswer && options.length > 0 ? (
        <View style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }}>
          {options.map((o) => (
            <Button
              key={o.id}
              label={o.label}
              variant="secondary"
              size="small"
              busy={sending === o.id}
              disabled={busy}
              onPress={() => onAnswer(o.id as Answer)}
            />
          ))}
        </View>
      ) : null}
      {problem ? (
        <Text variant="footnote" color="danger" accessibilityRole="alert">
          {problem}
        </Text>
      ) : null}
    </TintCard>
  );
}
