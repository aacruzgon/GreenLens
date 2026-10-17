import { PLANT_STATUS_LABEL } from "@/domain/format";
import { STATUS_LABEL } from "@/domain/scan";
import type { PlantStatus, ScanStatus } from "@/domain/types";

import { Chip, type ChipTone } from "../primitives/Chip";

const plantTone: Record<PlantStatus, ChipTone> = { healthy: "healthy", watch: "watch", sick: "sick", dead: "dead" };
const plantIcon = { healthy: "checkmark-circle", watch: "eye", sick: "medkit", dead: "close-circle" } as const;

const scanTone: Record<ScanStatus, ChipTone> = {
  pending: "neutral",
  queued: "info",
  analysing: "info",
  awaiting_answer: "watch",
  diagnosed: "accent",
  planned: "healthy",
  failed: "danger",
};

export function PlantStatusChip({ status }: { status: PlantStatus }) {
  return <Chip label={PLANT_STATUS_LABEL[status]} tone={plantTone[status]} icon={plantIcon[status]} accessibilityLabel={`Status: ${PLANT_STATUS_LABEL[status]}`} />;
}

export function ScanStatusChip({ status }: { status: ScanStatus }) {
  return <Chip label={STATUS_LABEL[status]} tone={scanTone[status]} accessibilityLabel={`Scan: ${STATUS_LABEL[status]}`} />;
}
