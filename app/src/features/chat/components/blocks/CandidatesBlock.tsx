// Candidates compared (design 5): the problem and its look-alike side by side with how to tell them apart, from
// `compare_lookalikes`. Neither plate is drawn as the answer (screens 6).
import { View } from "react-native";

import type { AnswerBlock } from "@/domain/proposed";
import { CandidateCompare, Text, space } from "@/ui";

import { blockSummary } from "../../model/blocks";
import { BlockFrame } from "./BlockFrame";

type Block = Extract<AnswerBlock, { kind: "candidates" }>;

export function CandidatesBlock({ block }: { block: Block }) {
  const other = block.lookalikes[0];
  return (
    <BlockFrame title="How to tell them apart" icon="info" summary={blockSummary(block)} testID="block-candidates">
      {other ? (
        <CandidateCompare title={`${block.disease.name} or ${other.name}`} plates={[{ name: block.disease.name }, { name: other.name }]} />
      ) : null}
      <View style={{ gap: space[2] }}>
        {block.lookalikes.map((l) => (
          <Text key={l.problem_id} variant="subheadline">
            <Text variant="subheadline" emphasis>
              {l.name}:
            </Text>{" "}
            {l.how_to_tell}
          </Text>
        ))}
      </View>
    </BlockFrame>
  );
}
