import { useRouter } from "expo-router";
import { useReducer, useRef, useState, type RefObject } from "react";

import { isApiError, newKey, useAllPlants, useGarden, useGardenMap, usePutGardenMap, type IdempotencyKey } from "@/api";
import { can } from "@/domain/permissions";
import { haptics, mapping, type GardenMappingHandle, type TrackingState } from "@/platform";

import { emptyMapping, mappingReducer, pinnedIds, toMapInput, undoRemovesMarker, type MappingEvent, type PinPlant } from "../model/session";

export type MappingPhase =
  /** Waiting for the garden, its map and its plants. */
  | { type: "loading" }
  /** A world map of the current version is on this phone: edit it, or map again. */
  | { type: "choose" }
  | { type: "relocalising" }
  | { type: "mapping" }
  /** Another member uploaded since this session started (contracts 9, 409). */
  | { type: "stale"; current: number }
  | { type: "saved"; version: number };

// The mapping session's view-model hook (flows 11): the AR view's points into the session, the upload with the
// version the session started from, and the world map kept on the phone for the next edit.
export function useMappingSession(gardenId: string, view: RefObject<GardenMappingHandle | null>) {
  const router = useRouter();
  const garden = useGarden(gardenId);
  const map = useGardenMap(gardenId);
  const plants = useAllPlants(gardenId);
  const put = usePutGardenMap(gardenId);
  const [state, send] = useReducer(mappingReducer, emptyMapping);
  const [tracking, setTracking] = useState<TrackingState>("limited");
  const [chosen, setChosen] = useState<"choose" | "relocalising" | "mapping">();
  const [result, setResult] = useState<MappingPhase>();
  const [problem, setProblem] = useState<string>();
  // The version this session started from: 0 when the garden had no map (contracts 9, base_version).
  const [base, setBase] = useState<number>();
  // One Save is one action: a retry sends the same body under the same key.
  const action = useRef<{ key: IdempotencyKey; body: string }>(undefined);

  const ready = garden.data && map.isSuccess && plants.data;
  const startVersion = map.isSuccess ? (map.data?.version ?? 0) : undefined;
  // Fixed at the first answer: a refetch after another member's upload mustn't move it (React's state from render).
  if (base === undefined && startVersion !== undefined) setBase(startVersion);
  const current = map.data ?? null;
  const editable = current !== null && mapping.savedWorldMapVersion(gardenId) === current.version;
  const phase: MappingPhase = result ?? (!ready ? { type: "loading" } : { type: chosen ?? (editable ? "choose" : "mapping") });

  function dispatch(event: MappingEvent) {
    if (event.type === "undo" && undoRemovesMarker(state)) void view.current?.undoLast();
    send(event);
  }

  function onTracking(next: TrackingState) {
    // Every ARKit session has its own origin: a session that lost tracking can't continue its map (architecture 6.8).
    if (next === "lost" && tracking !== "lost" && (state.corners.length > 0 || state.beds.length > 0 || state.pins.length > 0)) {
      send({ type: "restart" });
      void view.current?.clearMarkers();
      setProblem("The phone lost its place, so the map starts again. Move slowly and keep the ground in view.");
      void haptics.warning();
    }
    setTracking(next);
  }

  // Finding a saved map can be cancelled for a fresh start (HIG, augmented reality: handling interruptions).
  const cancelled = useRef(false);

  async function edit() {
    cancelled.current = false;
    setChosen("relocalising");
    const outcome = await mapping.relocalise(gardenId);
    if (cancelled.current) return;
    if (outcome === "ok" && current) {
      const names = Object.fromEntries((plants.data ?? []).map((p) => [p.id, p.name]));
      send({ type: "load", map: current, names });
    } else {
      setProblem("Couldn't find the saved map here. Map the garden again from the start.");
    }
    setChosen("mapping");
  }

  async function save(replace?: number) {
    if (replace !== undefined) setBase(replace);
    const body = toMapInput(state, replace ?? base ?? 0);
    const text = JSON.stringify(body);
    if (action.current?.body !== text) action.current = { key: newKey(), body: text };
    setProblem(undefined);
    try {
      const saved = await put.mutateAsync(body, action.current.key);
      action.current = undefined;
      void haptics.success();
      // The phone keeps this session's space for the next edit; failing that only means mapping again next time.
      await mapping.saveWorldMap(gardenId, saved.version).catch(() => undefined);
      setResult({ type: "saved", version: saved.version });
    } catch (error) {
      if (isApiError(error) && error.code === "invalid_state" && typeof error.details?.current_map_version === "number") {
        setResult({ type: "stale", current: error.details.current_map_version });
        return;
      }
      setProblem(isApiError(error) ? error.message : "Couldn't save the map. Try again.");
    }
  }

  const pinned = pinnedIds(state);
  const role = garden.data?.my_role;
  return {
    phase,
    state,
    tracking,
    problem,
    canMap: can(role, "map.capture"),
    saving: put.isPending,
    /** Plants without a pin in this session first. */
    plants: [...(plants.data ?? [])].sort((a, b) => Number(pinned.has(a.id)) - Number(pinned.has(b.id)) || a.name.localeCompare(b.name)),
    pinned,
    actions: {
      dispatch,
      onTracking,
      choosePlant: (plant: PinPlant) => send({ type: "choosePlant", plant }),
      edit: () => void edit(),
      mapAgain: () => {
        cancelled.current = true;
        if (chosen === "relocalising") void view.current?.restart();
        setChosen("mapping");
      },
      /** Throws away this session's beds and pins and starts the AR session over. */
      startAgain: () => {
        send({ type: "restart" });
        void view.current?.restart();
      },
      save: () => void save(),
      /** After a 409: replace the newer map with this session's. */
      replace: (current: number) => {
        setResult(undefined);
        void save(current);
      },
      dismissProblem: () => setProblem(undefined),
      close: () => router.back(),
    },
  };
}
