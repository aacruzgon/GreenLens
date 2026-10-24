// One answer block, by kind (design 5). Interactions inside a block are local, except the frost planner's Add,
// which the user starts in the app's own UI and so writes with no card (S5). A kind the app doesn't know is
// skipped: the text answer makes sense without it.
import type { AnswerBlock } from "@/domain/proposed";

import { CandidatesBlock } from "./CandidatesBlock";
import { FrostPlannerBlock } from "./FrostPlannerBlock";
import { GardenMapBlock } from "./GardenMapBlock";
import { MeasurementTrendBlock } from "./MeasurementTrendBlock";
import { WeatherHoursBlock } from "./WeatherHoursBlock";

export interface AnswerBlockViewProps {
  block: AnswerBlock;
  gardenId: string;
  canAddTasks: boolean;
  onOpenPlant: (plantId: string) => void;
}

export function AnswerBlockView({ block, gardenId, canAddTasks, onOpenPlant }: AnswerBlockViewProps) {
  switch (block.kind) {
    case "garden_map":
      return <GardenMapBlock block={block} onOpenPlant={onOpenPlant} />;
    case "weather_hours":
      return <WeatherHoursBlock block={block} />;
    case "frost_planner":
      return <FrostPlannerBlock block={block} gardenId={gardenId} canAddTasks={canAddTasks} />;
    case "measurement_trend":
      return <MeasurementTrendBlock block={block} onOpenPlant={onOpenPlant} />;
    case "candidates":
      return <CandidatesBlock block={block} />;
    default:
      return null;
  }
}
