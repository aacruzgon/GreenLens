// Contract shapes the app needs that the backend doesn't have yet (roadmap 17; design 5, "Chat: answers you can
// interact with"). Hand-written, unlike types.ts, because no `domain` type exists to generate them from. Each one
// is the app's proposal: when the backend lands its version (the `domain` type, its exact-JSON test,
// contracts.md 8 and openapi.json), `npm run gen:api` brings the real one, this file goes, and types.ts names it.
// Only mock mode sends them today; the live API never does, and every reader treats them as optional. The answer
// blocks follow the backend's version in PR #191 (S5 work 8: `AnswerBlock`, `FrostPlant`, `Lookalike`,
// `ProblemRef`), and `Message` with `MessageReplay` PR #186.
import type {
  ForecastDay,
  GardenMap,
  GrowthQuantity,
  Id,
  ImageKind,
  MeasureQuantity,
  MeasurementPoint,
  Message,
  OrganismRole,
  PastWeather,
  Plant,
  PlantStatus,
  ProblemKind,
  Slug,
  TimelineEntry,
  WeatherFit,
} from "./types";

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

/** `Message` with the proposed `blocks` (absent when empty, like `farm_citations`). */
export type ChatMessage = Message & {
  blocks?: AnswerBlock[];
};

/**
 * A timeline entry with the plant's status after that scan (roadmap F7: the garden's date scrubber and Insights'
 * status strip). `TimelineEntry` has the scan's status and its primary problem, not the plant's, so the app can
 * only guess a past status from them (`statusOfEntry`). Proposed: `plant_status` on the entry, as the plant's
 * status was set by that scan's diagnosis.
 */
export type TimelineEntryWithStatus = TimelineEntry & { plant_status?: PlantStatus };

/** `SpeciesSeed.frost` (PR #188), as the backend keeps it: `tender` is damaged by a light frost, `hardy` stands one. */
export type Frost = "tender" | "hardy";

/**
 * A plant whose species carries its `frost` (roadmap F7: Frost mode covers the tender plants). PR #188 adds
 * `frost` to the species seed, which the agent reads; the app has no route that returns it. Proposed: `frost` on
 * `Plant.species`, absent when the species is unknown or its sources don't say.
 */
export type PlantWithFrost = Omit<Plant, "species"> & { species?: NonNullable<Plant["species"]> & { frost?: Frost } };

// ---- Insights (roadmap F7 item 3; design 8; analytics 5)

/**
 * A measurement point of any quantity, with its spread (analytics 2, P3). `GET /plants/{id}/measurements` takes
 * only the growth quantities and sends a value per scan. Proposed: `?quantity=` takes every `MeasureQuantity`,
 * and each point carries the measurement's `min`, `max` and `n` over the measured items and the `image_kind` it
 * came from (the data is in `observation` already). `noise_cm`, for height and width, is the repeat-scan noise
 * once vision measures it (vision evaluation.md); until then the app draws ±2 cm or
 * 5% and says the band is provisional. All optional: the live API sends none of them yet.
 */
export type RangePoint = Omit<MeasurementPoint, "quantity"> & {
  quantity: MeasureQuantity;
  min?: number;
  max?: number;
  n?: number;
  image_kind?: ImageKind;
  noise_cm?: number;
};

/** `TimelineEntry.weather_7d`: the scan's weather snapshot, its past 7 days (analytics 5, G4). */
export type Weather7d = Pick<PastWeather, "hours_rh_over_90" | "longest_humid_run_days" | "precip_mm">;

/**
 * A timeline entry with what Insights reads off each scan (design 8): `low_evidence` (a "Not sure" with nothing
 * pointing to a problem draws grey, not amber; P1), the primary problem's `kind` and `weather_fit`, the scan's
 * `weather_7d` (G4), and its organisms of medium confidence or more (G6). Without them, one `GET /scans/{id}` per
 * scan; the app hides what needs them instead.
 */
export type InsightTimelineEntry = Omit<TimelineEntryWithStatus, "primary"> & {
  primary?: NonNullable<TimelineEntry["primary"]> & { kind?: ProblemKind; weather_fit?: WeatherFit };
  low_evidence?: boolean;
  weather_7d?: Weather7d;
  organisms?: { name: string; role: OrganismRole }[];
};
