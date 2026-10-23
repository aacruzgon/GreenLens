// `rescan_requested` (flows 9): the instructions, and Take a new photo for editors and owners, which starts a scan
// of that plant (useStartCapture, flow 4). `request_rescan` is offered to every role, so a viewer or reviewer
// reads the instructions only.
import { Card, Button, Text, space } from "@/ui";

import type { RescanRequest } from "../model/messageReducer";

export interface RescanCardProps {
  rescan: RescanRequest;
  plantName?: string;
  canScan: boolean;
  starting: boolean;
  onScan: () => void;
}

export function RescanCard({ rescan, plantName, canScan, starting, onScan }: RescanCardProps) {
  return (
    <Card style={{ gap: space[2] }} testID="rescan-card">
      <Text variant="headline" accessibilityRole="header">
        {plantName ? `A new photo of ${plantName}` : "A new photo"}
      </Text>
      <Text variant="subheadline">{rescan.instructions}</Text>
      {canScan ? <Button label="Take a new photo" size="small" onPress={onScan} busy={starting} style={{ alignSelf: "flex-end" }} /> : null}
    </Card>
  );
}
