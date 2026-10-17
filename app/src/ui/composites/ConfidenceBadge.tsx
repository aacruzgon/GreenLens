import { BAND_LABEL } from "@/domain/format";
import type { ConfidenceBand } from "@/domain/types";

import { Chip } from "../primitives/Chip";

/** Only on findings and measurements, never on a diagnosis (docs/frontend 14). */
export function ConfidenceBadge({ band }: { band: ConfidenceBand }) {
  const tone = band === "high" ? "accent" : band === "medium" ? "info" : "neutral";
  return <Chip label={BAND_LABEL[band]} tone={tone} accessibilityLabel={`${BAND_LABEL[band]} confidence`} />;
}
