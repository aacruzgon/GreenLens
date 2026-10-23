// The live analysis's endings other than a plan (roadmap F3 item 7; screens 3.3 and 6): a failed run with Retry
// or New scan, and a result too weak to name a problem with Take a closer photo. Solid dark cards on the photo.
import type { ReactNode } from "react";
import { View } from "react-native";

import { LOW_EVIDENCE_HEADLINE, LOW_EVIDENCE_SUBLINE } from "@/domain/diagnosis";
import type { ScanFailure } from "@/domain/types";
import { Button, Text, camera, radii, space } from "@/ui";

function OutcomeCard({ testID, children }: { testID: string; children: ReactNode }) {
  return (
    <View
      testID={testID}
      accessibilityLiveRegion="polite"
      style={{ padding: 18, gap: space[3], borderRadius: radii.sheet, borderCurve: "continuous", backgroundColor: camera.card, borderWidth: 1, borderColor: camera.hairline }}
    >
      {children}
    </View>
  );
}

export function FailureCard({
  failure,
  canAct,
  busy,
  onRetry,
  onNewScan,
}: {
  failure: ScanFailure;
  canAct: boolean;
  busy: boolean;
  onRetry: () => void;
  onNewScan: () => void;
}) {
  return (
    <OutcomeCard testID="failure-card">
      <Text variant="headline" accessibilityRole="header">
        {failure.retryable ? "The analysis stopped" : "We couldn't finish this scan"}
      </Text>
      <Text variant="subheadline" color="textMuted">
        {failure.message}
      </Text>
      {canAct ? (
        failure.retryable ? (
          <Button label="Retry" busy={busy} onPress={onRetry} testID="retry" />
        ) : (
          <Button label="New scan" onPress={onNewScan} testID="new-scan" />
        )
      ) : null}
    </OutcomeCard>
  );
}

/** `low_evidence`: no problem is named (screens 6). Take a closer photo answers an open photo request, if any. */
export function LowEvidenceCard({ canAct, onCloserPhoto }: { canAct: boolean; onCloserPhoto: () => void }) {
  return (
    <OutcomeCard testID="low-evidence-card">
      <Text variant="headline" accessibilityRole="header">
        {LOW_EVIDENCE_HEADLINE}
      </Text>
      <Text variant="subheadline" color="textMuted">
        {LOW_EVIDENCE_SUBLINE}
      </Text>
      {canAct ? <Button label="Take a closer photo" onPress={onCloserPhoto} testID="closer-photo" /> : null}
    </OutcomeCard>
  );
}
