// A garden mapping session (architecture 6.8; flows 11; contracts 9, garden map): the beds drawn corner by corner,
// a pin per plant, north from the compass, and the upload they make. Pure; the AR view only reports points.
import { inPolygon, polygonArea } from "@/domain/map";
import type { GardenMap, GardenMapInput } from "@/domain/types";

export type Point = [number, number];

export interface BedDraft {
  id: string;
  name: string;
  polygon: Point[];
}

/** An existing plant, or one named during the session and created with the upload. */
export type PinPlant = { plantId: string; name: string } | { newName: string };

export interface PinDraft {
  plant: PinPlant;
  at: Point;
  canopy?: number;
}

export interface MappingState {
  beds: BedDraft[];
  /** The corners of the bed being drawn. */
  corners: Point[];
  pins: PinDraft[];
  /** The plant the next pin is for. */
  next?: PinPlant;
  north?: { deg: number; accuracy: number };
  /** Every earlier state, for Undo; `marker` says whether the step put a marker in the AR view. */
  history: { state: Omit<MappingState, "history">; marker: boolean }[];
}

export type MappingEvent =
  | { type: "corner"; at: Point }
  | { type: "closeBed"; name: string }
  | { type: "choosePlant"; plant: PinPlant }
  | { type: "pin"; at: Point; canopy?: number }
  | { type: "heading"; deg: number; accuracy: number }
  | { type: "undo" }
  /** Tracking was lost: the garden is mapped again from the start (architecture 6.8). */
  | { type: "restart" }
  | { type: "load"; map: GardenMap; names: Record<string, string> };

/** Limits of contracts 9. */
export const MAX_BEDS = 20;
export const MAX_PINS = 200;
export const MAX_CORNERS = 50;
/** North is sent only with a compass accuracy of 20° or better. */
export const NORTH_ACCURACY = 20;

export const emptyMapping: MappingState = { beds: [], corners: [], pins: [], history: [] };

const pinKey = (p: PinPlant) => ("plantId" in p ? p.plantId : `new:${p.newName.trim().toLowerCase()}`);

function snapshot({ history: _, ...rest }: MappingState) {
  return rest;
}

function step(state: MappingState, next: Omit<MappingState, "history">, marker: boolean): MappingState {
  return { ...next, history: [...state.history, { state: snapshot(state), marker }] };
}

export function mappingReducer(state: MappingState, event: MappingEvent): MappingState {
  const s = snapshot(state);
  switch (event.type) {
    case "corner":
      if (state.corners.length >= MAX_CORNERS || state.beds.length >= MAX_BEDS) return state;
      return step(state, { ...s, corners: [...s.corners, event.at] }, true);
    case "closeBed": {
      if (state.corners.length < 3) return state;
      const bed: BedDraft = { id: `bed-${state.beds.length + 1}-${Date.now().toString(36)}`, name: event.name.trim() || `Bed ${state.beds.length + 1}`, polygon: s.corners };
      return step(state, { ...s, beds: [...s.beds, bed], corners: [] }, false);
    }
    case "choosePlant":
      return { ...state, next: event.plant };
    case "pin": {
      if (!state.next || state.pins.length >= MAX_PINS) return state;
      // A plant appears at most once: pinning it again moves its pin.
      const key = pinKey(state.next);
      const pins = [...s.pins.filter((p) => pinKey(p.plant) !== key), { plant: state.next, at: event.at, ...(event.canopy !== undefined ? { canopy: event.canopy } : {}) }];
      return step(state, { ...s, pins, next: undefined }, true);
    }
    case "heading":
      return { ...state, north: { deg: event.deg, accuracy: event.accuracy } };
    case "undo": {
      const last = state.history[state.history.length - 1];
      if (!last) return state;
      return { ...last.state, north: state.north, next: state.next, history: state.history.slice(0, -1) };
    }
    case "restart":
      return { ...emptyMapping, north: state.north };
    case "load":
      return {
        ...emptyMapping,
        beds: event.map.beds.map((b) => ({ id: b.id, name: b.name ?? "", polygon: b.polygon_m.map(([x, y]) => [x, y] as Point) })),
        pins: event.map.pins.map((p) => ({
          plant: { plantId: p.plant_id, name: event.names[p.plant_id] ?? "Plant" },
          at: [p.x_m, p.y_m] as Point,
          ...(p.canopy_radius_m !== undefined ? { canopy: p.canopy_radius_m } : {}),
        })),
        north: state.north,
      };
  }
}

/** Whether the last step put a marker in the AR view, so Undo removes it there too. */
export function undoRemovesMarker(state: MappingState): boolean {
  return state.history[state.history.length - 1]?.marker ?? false;
}

/** Plants already pinned in this session, by ID. */
export function pinnedIds(state: MappingState): Set<string> {
  return new Set(state.pins.flatMap((p) => ("plantId" in p.plant ? [p.plant.plantId] : [])));
}

const round = (m: number) => Math.round(m * 1000) / 1000;

/** The upload: each pin in the bed it stands in, north only when the compass was good enough. */
export function toMapInput(state: MappingState, baseVersion: number): GardenMapInput {
  return {
    base_version: baseVersion,
    beds: state.beds.map((b) => ({ id: b.id, ...(b.name ? { name: b.name } : {}), polygon_m: b.polygon.map(([x, y]) => [round(x), round(y)]) })),
    pins: state.pins.map((p) => {
      const bed = state.beds.find((b) => inPolygon(p.at[0], p.at[1], b.polygon));
      return {
        ...("plantId" in p.plant ? { plant_id: p.plant.plantId } : { new_plant: { name: p.plant.newName.trim() } }),
        x_m: round(p.at[0]),
        y_m: round(p.at[1]),
        ...(p.canopy !== undefined ? { canopy_radius_m: Math.round(p.canopy * 100) / 100 } : {}),
        ...(bed ? { bed_id: bed.id } : {}),
      };
    }),
    ...(state.north && state.north.accuracy <= NORTH_ACCURACY ? { north_deg: Math.round(state.north.deg * 10) / 10 } : {}),
  };
}

/** What the session has so far, in words: "2 beds · 6 plants · north". */
export function sessionSummary(state: MappingState): string {
  const beds = state.beds.length === 1 ? "1 bed" : `${state.beds.length} beds`;
  const plants = state.pins.length === 1 ? "1 plant" : `${state.pins.length} plants`;
  const north = state.north && state.north.accuracy <= NORTH_ACCURACY ? " · north" : "";
  return `${beds} · ${plants}${north}`;
}

/** The open bed's area so far, from 3 corners on: "2.4 m²". */
export function openBedArea(state: MappingState): string | undefined {
  return state.corners.length >= 3 ? `${polygonArea(state.corners).toFixed(1)} m²` : undefined;
}
