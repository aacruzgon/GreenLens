// One answer block, by kind (design 5). Interactions inside a block are local; anything that writes goes through
// a card. A kind the app doesn't know is skipped: the text answer makes sense without it.
import type { AnswerBlock } from "@/domain/proposed";

import { CandidatesBlock } from "./CandidatesBlock";
import { FrostPlannerBlock } from "./FrostPlannerBlock";
import { GardenMapBlock } from "./GardenMapBlock";
import { MeasurementTrendBlock } from "./MeasurementTrendBlock";
import { WeatherHoursBlock } from "./WeatherHoursBlock";

export interface AnswerBlockViewProps {
  block: AnswerBlock;
  canWrite: boolean;
  busy: boolean;
  onAsk: (text: string) => void;
  onOpenPlant: (plantId: string) => void;
}

export function AnswerBlockView({ block, canWrite, busy, onAsk, onOpenPlant }: AnswerBlockViewProps) {
  switch (block.kind) {
    case "garden_map":
      return <GardenMapBlock block={block} onOpenPlant={onOpenPlant} />;
    case "weather_hours":
      return <WeatherHoursBlock block={block} />;
    case "frost_planner":
      return <FrostPlannerBlock block={block} canWrite={canWrite} busy={busy} onAsk={onAsk} />;
    case "measurement_trend":
      return <MeasurementTrendBlock block={block} onOpenPlant={onOpenPlant} />;
    case "candidates":
      return <CandidatesBlock block={block} />;
    default:
      return null;
  }
}
