// Contract shapes the app needs that the backend doesn't have yet (roadmap 17; design 5, "Chat: answers you can
// interact with"). Hand-written, unlike types.ts, because no `domain` type exists to generate them from. Each one
// is the app's proposal: when the backend lands its version (the `domain` type, its exact-JSON test,
// contracts.md 8 and openapi.json), `npm run gen:api` brings the real one, this file goes, and types.ts names it.
// Only mock mode sends them today; the live API never does, and every reader treats them as optional. The answer
// blocks follow the backend's version in PR #191 (S5 work 8: `AnswerBlock`, `FrostPlant`, `Lookalike`,
// `ProblemRef`), `FarmCitation.text` follows PR #189, and `Message` with `MessageReplay` PR #186.
import type { FarmCitation, ForecastDay, GardenMap, GrowthQuantity, Id, MeasurementPoint, Message, PlantStatus, Slug } from "./types";

/** A plant as a block shows it: the records' name, status and latest height, never the model's words. */
export interface BlockPlant {
  plant_id: Id;
  name: string;
  status: PlantStatus;
  /** The latest `plant_height_cm` point, when there is one. */
  height_cm?: number;
}

/** `FrostPlant = BlockPlant & { frost_tender? }`: absent when the species is unknown or its sources don't say. */
export type FrostPlant = BlockPlant & { frost_tender?: boolean };

/**
 * `AnswerBlock`, a tagged union by `kind` (design 5). Each holds the records it shows and the `call_id` of the
 * tool result they came from; the agent picks one with `show_block({ kind, call_id, focus_plant_ids? })`, which
 * the API checks against that turn's tool results and fills from the store. No block carries free text the model
 * wrote, and `show_block` sends no tool chip.
 */
export type AnswerBlock = { call_id: string } & (
  | {
      /** From `get_garden_map`. */
      kind: "garden_map";
      map: GardenMap;
      plants: BlockPlant[];
      /** The plants the answer is about, from the `show_block` call; each is on the map. May be empty. */
      focus_plant_ids: Id[];
      /** `get_garden_map`'s `neighbours`. */
      neighbours: Record<Id, Id[]>;
    }
  | {
      /** From `get_scan` (its weather snapshot) or `get_forecast`. */
      kind: "weather_hours";
      days: ForecastDay[];
      /** Set when the days are a scan's snapshot. */
      scan_id?: Id;
    }
  | {
      /** From `get_forecast`. The API refuses the block when no night has frost. */
      kind: "frost_planner";
      /** The forecast days with `frost_risk`, first one first. */
      nights: ForecastDay[];
      /**
       * The garden's live plants (not the map's pins, so a garden with no map has a planner), with the species'
       * `frost: "tender" | "hardy"` as `frost_tender`.
       */
      plants: FrostPlant[];
    }
  | {
      /** From `get_plant_measurements`. */
      kind: "measurement_trend";
      plant_id: Id;
      plant_name: string;
      quantity: GrowthQuantity;
      /** Oldest first, as the tool returns them. */
      points: MeasurementPoint[];
    }
  | {
      /** From `compare_lookalikes`. */
      kind: "candidates";
      disease: { problem_id: Slug; name: string };
      lookalikes: { problem_id: Slug; name: string; how_to_tell: string }[];
    }
);

export type AnswerBlockKind = AnswerBlock["kind"];

/** The `answer_block` SSE event: `{ v: 1, block }`, sent after the tool result it shows. */
export interface AnswerBlockEvent {
  v: 1;
  block: AnswerBlock;
}

/** `Message` with the proposed `blocks` (absent when empty, like `farm_citations`) and farm records' text. */
export type ChatMessage = Omit<Message, "farm_citations"> & {
  farm_citations?: FarmCitationWithText[];
  blocks?: AnswerBlock[];
};

/**
 * A farm citation with the record's text. E10.3 opens an `era` record's text in a sheet, but `FarmCitation`
 * carries no text and no route returns a farm record. Proposed: `text` on the citation for `era` records only
 * (they are synthetic, so nothing private leaves the server), or `GET /farm-records/{id}`.
 */
export type FarmCitationWithText = FarmCitation & { text?: string };
