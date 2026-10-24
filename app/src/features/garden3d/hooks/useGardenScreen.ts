// The garden screen's view model (roadmap F7 items 1–2; design 3.1, the 3D garden): the map, every plant's
// timeline and the forecast, as the scene for the chosen date and mode; the selection; and where each control
// leads. The 3D view draws on the iPhone; the 2D map in Expo Go, when the scene can't draw, and on **Map**.
import { useRouter } from "expo-router";
import { useEffect, useMemo, useReducer } from "react";

import { useAllPlants, useGarden, useGardenMap, useGardenWeather, useTimelines } from "@/api";
import { formatMonthDay, localToday } from "@/domain/format";
import { can } from "@/domain/permissions";
import type { PlantWithFrost } from "@/domain/proposed";
import { useStartCapture } from "@/features/capture";
import { useAskAbout } from "@/features/chat";
import { useActiveGardenId } from "@/features/gardens";
import { capabilities, GardenView, haptics } from "@/platform";

import {
  defaultSelection,
  gardenDates,
  gardenReducer,
  initialGardenState,
  nearestPlant,
  plantNote,
  plantRecords,
  plantSteps,
  topCard,
  type GardenAction,
  type GardenState,
  type GardenViewKind,
} from "../model/garden";
import { buildScene, sceneSummary, type GardenMode } from "../model/gardenScene";

export interface GardenScreenParams {
  /** The plant to open on. */
  plantId?: string;
  /** Plants a chat answer is about (its map block); the first is selected. */
  focus?: string[];
  mode?: GardenMode;
}

export function useGardenScreen(params: GardenScreenParams) {
  const router = useRouter();
  const gardenId = useActiveGardenId();
  const garden = useGarden(gardenId);
  const map = useGardenMap(gardenId);
  const plants = useAllPlants(gardenId);
  const weather = useGardenWeather(gardenId);
  const plantIds = useMemo(() => (plants.data ?? []).map((p) => p.id), [plants.data]);
  const timelines = useTimelines(plantIds);
  const startCapture = useStartCapture();
  const askAbout = useAskAbout(gardenId);

  const tz = garden.data?.time_zone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
  const today = localToday(tz);
  const forecast = useMemo(() => weather.data?.weather.forecast ?? [], [weather.data]);
  const records = useMemo(
    () => plantRecords((plants.data ?? []) as PlantWithFrost[], timelines.byPlant, tz),
    [plants.data, timelines.byPlant, tz],
  );
  const dates = useMemo(() => gardenDates(records, forecast, today), [records, forecast, today]);
  // Expo Go and the web have no GardenView; the garden opens on the map there (design 7).
  const canDraw3d = GardenView !== null;

  const [state, dispatch] = useReducer(
    (s: GardenState, a: GardenAction) => gardenReducer(dates, s, a),
    undefined,
    () => initialGardenState(dates, { view: canDraw3d ? "3d" : "map", mode: params.mode, selectedId: params.plantId ?? params.focus?.[0] ?? null }),
  );

  // The dates arrive after the first render: open on the latest scan (or the forecast in Frost) once they all do.
  // Only once every plant's timeline is in, since the forecast night alone would open on it, and once the forecast
  // has answered, since Frost asked for before it would stay on a scan date.
  const ready = plants.isSuccess && !timelines.isPending && !weather.isPending && dates.length > 0;
  useEffect(() => {
    if (ready) dispatch({ type: "init", mode: params.mode });
    // Only once the dates are first known.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  const date = dates[state.dateIndex] ?? dates.at(-1);
  const scene = useMemo(
    () => (map.data && date ? buildScene(map.data, records, date.date) : { beds: [], plants: [] }),
    [map.data, records, date],
  );

  // Select a plant once the scene is on its date: the one asked for, else the first that needs attention. Before
  // `init`, every pin is still "unknown" (no timeline yet), and the first pin would win (review of #205).
  const sceneReady = state.initialised && scene.plants.length > 0;
  useEffect(() => {
    if (!sceneReady) return;
    if (state.selectedId && scene.plants.some((p) => p.id === state.selectedId)) return;
    dispatch({ type: "select", id: defaultSelection(scene.plants, params.plantId ?? params.focus?.[0]) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sceneReady]);

  // Growth labels carry the height ("Kale · 55 cm"), and "not measured" where there is none (design 7).
  const labelled = scene.plants.map((p) =>
    state.mode === "growth" ? { ...p, label: p.height_m !== undefined ? `${p.name} · ${Math.round(p.height_m * 100)} cm` : `${p.name} · not measured` } : p,
  );
  const selected = labelled.find((p) => p.id === state.selectedId);
  const record = records.find((r) => r.id === state.selectedId);
  const near = selected ? nearestPlant(scene.plants, selected.id) : undefined;
  const role = garden.data?.my_role;

  const card =
    date && garden.data
      ? topCard({
          mode: state.mode,
          gardenName: garden.data.name,
          plants: scene.plants,
          records,
          date,
          forecast,
          ...(map.data ? { mappedOn: formatMonthDay(map.data.created_at, tz) } : {}),
        })
      : undefined;

  return {
    loading: garden.isPending || map.isPending || plants.isPending || (plantIds.length > 0 && timelines.isPending),
    error: garden.error ?? map.error ?? plants.error ?? undefined,
    retry: () => {
      void garden.refetch();
      void map.refetch();
      void plants.refetch();
    },
    gardenName: garden.data?.name ?? "Garden",
    /** No map yet: screens 3.8's empty state. */
    noMap: map.isSuccess && map.data === null,
    map: map.data ?? null,
    subtitle: map.data ? `Mapped ${formatMonthDay(map.data.created_at, tz)} · ${scene.plants.length} ${scene.plants.length === 1 ? "plant" : "plants"}` : undefined,
    summary: sceneSummary(scene),
    state,
    canDraw3d,
    dates,
    date,
    beds: scene.beds,
    plants: labelled,
    card,
    selected: selected
      ? {
          plant: selected,
          note: date ? plantNote(record, date) : undefined,
          near: near ? { name: near.plant.name, cm: near.cm } : undefined,
          steps: plantSteps(record, dates),
        }
      : undefined,
    can: {
      scan: can(role, "scan.create"),
      /** Edit opens the mapping session: editors, on a phone that can map (screens 3.8). */
      edit: can(role, "map.capture") && capabilities().mapping,
    },
    scanning: startCapture.isPending,
    asking: askAbout.opening,
    problem: askAbout.problem ?? (startCapture.error ? "The scan couldn't start. Check your network." : undefined),
    actions: {
      back: () => (router.canGoBack() ? router.back() : router.replace("/")),
      setMode: (mode: GardenMode) => dispatch({ type: "mode", mode }),
      setDate: (index: number) => {
        if (Math.round(index) !== state.dateIndex) void haptics.selection();
        dispatch({ type: "date", index });
      },
      select: (id: string | null) => dispatch({ type: "select", id }),
      setView: (view: GardenViewKind) => dispatch({ type: "view", view }),
      expand: (on: boolean) => dispatch({ type: "expand", on }),
      insights: () => {
        if (state.selectedId) router.push({ pathname: "/plants/[plantId]/insights", params: { plantId: state.selectedId } });
      },
      scan: () => {
        if (state.selectedId) void startCapture.start({ plantId: state.selectedId }, { push: true });
        else router.push("/scan/new");
      },
      ask: () => {
        if (state.selectedId) void askAbout.ask({ type: "plant", id: state.selectedId });
        else void askAbout.ask({ type: "garden", id: gardenId });
      },
      openPlant: (id: string) => router.push({ pathname: "/plants/[plantId]", params: { plantId: id } }),
      editMap: () => router.push("/garden/map-session"),
    },
  };
}

export type GardenScreenModel = ReturnType<typeof useGardenScreen>;
