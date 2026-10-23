import { useRouter } from "expo-router";
import { useEffect, useReducer, useRef, useState, type RefObject } from "react";

import { isApiError, newKey, useAllPlants, useGarden, useGardenMap, usePutGardenMap, type IdempotencyKey } from "@/api";
import { can } from "@/domain/permissions";
import { haptics, mapping, type GardenMappingHandle, type TrackingState } from "@/platform";

import {
  emptyMapping,
  mappingReducer,
  pinnedIds,
  saveRefusal,
  toMapInput,
  undoRemovesMarker,
  type MappingEvent,
  type MappingState,
  type PinPlant,
} from "../model/session";

export type MappingPhase =
  /** Waiting for the garden, its map and its plants. */
  | { type: "loading" }
  /** One of them couldn't be loaded (screens 4: an error with Try again). */
  | { type: "failed" }
  /** A world map of the current version is on this phone: edit it, or map again. */
  | { type: "choose" }
  | { type: "relocalising" }
  | { type: "mapping" }
  /** Another member uploaded since this session started (contracts 9, 409). */
  | { type: "stale"; current: number }
  /** `dropped`: pins left out because their plants were deleted while mapping. */
  | { type: "saved"; version: number; dropped: number };

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
  // The camera taken away by a call or another app; ARKit finds its place again after.
  const [interrupted, setInterrupted] = useState(false);
  const [chosen, setChosen] = useState<"choose" | "relocalising" | "mapping">();
  const [result, setResult] = useState<MappingPhase>();
  const [problem, setProblem] = useState<string>();
  // The version this session started from: 0 when the garden had no map (contracts 9, base_version).
  const [base, setBase] = useState<number>();
  // One Save is one action: a retry sends the same body under the same key.
  const action = useRef<{ key: IdempotencyKey; body: string }>(undefined);

  const ready = garden.data && map.isSuccess && plants.data;
  const failed = !ready && (garden.isError || map.isError || plants.isError);
  const startVersion = map.isSuccess ? (map.data?.version ?? 0) : undefined;
  // Fixed at the first answer: a refetch after another member's upload mustn't move it (React's state from render).
  if (base === undefined && startVersion !== undefined) setBase(startVersion);
  const current = map.data ?? null;
  const savedVersion = mapping.savedWorldMapVersion(gardenId);
  const editable = current !== null && savedVersion === current.version;
  const phase: MappingPhase =
    result ?? (failed ? { type: "failed" } : !ready ? { type: "loading" } : { type: chosen ?? (editable ? "choose" : "mapping") });

  // A world map older than the version this session started from can't be edited again (contracts 9), so it
  // doesn't stay on the phone. Measured against `base`, which a save or another member's upload doesn't move, and
  // never while this session is saving its own.
  const busy = put.isPending || result !== undefined;
  useEffect(() => {
    if (!busy && base !== undefined && savedVersion !== null && savedVersion < base) mapping.remove(gardenId);
  }, [gardenId, savedVersion, base, busy]);

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

  /** Uploads `session` from `from`. A 400 for deleted plants drops their pins and resends once per plant set. */
  async function save(session: MappingState, from: number, dropped = 0) {
    const body = toMapInput(session, from);
    const text = JSON.stringify(body);
    if (action.current?.body !== text) action.current = { key: newKey(), body: text };
    setProblem(undefined);
    try {
      const saved = await put.mutateAsync(body, action.current.key);
      action.current = undefined;
      void haptics.success();
      // The phone keeps this session's space for the next edit; failing that only means mapping again next time.
      await mapping.saveWorldMap(gardenId, saved.version).catch(() => undefined);
      setResult({ type: "saved", version: saved.version, dropped });
    } catch (error) {
      const refusal = isApiError(error) ? saveRefusal(error.code, error.details) : { type: "other" as const };
      if (refusal.type === "stale") {
        setResult({ type: "stale", current: refusal.current });
        return;
      }
      if (refusal.type === "unknownPlants") {
        // The plants were deleted while mapping: their pins go, and the rest is sent again from the same base under
        // a new key, since the 400 is stored under the old one (contracts 9).
        const drop = { type: "dropPlants", plantIds: refusal.plantIds } as const;
        const next = mappingReducer(session, drop);
        send(drop);
        action.current = undefined;
        if (next !== session) {
          await save(next, from, dropped + session.pins.length - next.pins.length);
          return;
        }
      }
      setProblem(isApiError(error) ? error.message : "Couldn't save the map. Try again.");
    }
  }

  const pinned = pinnedIds(state);
  const role = garden.data?.my_role;
  const hasContent = state.beds.length > 0 || state.pins.length > 0;
  return {
    phase,
    state,
    tracking,
    interrupted,
    problem,
    canMap: can(role, "map.capture"),
    saving: put.isPending,
    /** Save works only while mapping, with something mapped and the camera there. */
    canSave: phase.type === "mapping" && hasContent && !interrupted,
    /** Plants without a pin in this session first. */
    plants: [...(plants.data ?? [])].sort((a, b) => Number(pinned.has(a.id)) - Number(pinned.has(b.id)) || a.name.localeCompare(b.name)),
    pinned,
    actions: {
      dispatch,
      onTracking,
      onInterruption: setInterrupted,
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
      /** `closeOpenBed` names the bed still being drawn, closed first; without it, the open corners are left out. */
      save: (closeOpenBed?: string) => {
        let session = state;
        if (closeOpenBed !== undefined) {
          const close = { type: "closeBed", name: closeOpenBed } as const;
          session = mappingReducer(session, close);
          send(close);
        }
        void save(session, base ?? 0);
      },
      /** After a 409: replace the newer map with this session's. */
      replace: (newer: number) => {
        setResult(undefined);
        setBase(newer);
        void save(state, newer);
      },
      /**
       * After a 409: keep their map, and map again from it. Their beds can't be placed in this session's space (each
       * session has its own origin), so the session starts over with their version as its base.
       */
      mapAgainFromTheirs: (newer: number) => {
        setResult(undefined);
        setBase(newer);
        action.current = undefined;
        void map.refetch();
        send({ type: "restart" });
        void view.current?.restart();
        setChosen("mapping");
      },
      retry: () => {
        if (garden.isError) void garden.refetch();
        if (map.isError) void map.refetch();
        if (plants.isError) void plants.refetch();
      },
      dismissProblem: () => setProblem(undefined),
      close: () => router.back(),
    },
  };
}
