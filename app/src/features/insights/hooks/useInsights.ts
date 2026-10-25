// The Insights screen's view model (roadmap F7 item 3; design 8; analytics 2): the plant, its whole timeline, its
// height and spot series, its plan's done tasks and its nearest plant on the map, read into the five sections by
// the pure model. A section whose data the API doesn't send yet (the proposed fields in domain/proposed.ts) is
// left out; live mode shows what the contracts carry today.
import { useRouter } from "expo-router";

import { isApiError, useAllPlants, useAllTasks, useGarden, useGardenMap, useInsightTimeline, usePlant, usePlantMeasurements } from "@/api";

import {
  afterSteps,
  diagnosisEntry,
  healthHeadline,
  heightReading,
  nearbyLine,
  nearestPlant,
  notCheckedRecently,
  rangeText,
  spotsReading,
  stepLanes,
  stripCells,
  stripSummary,
  weatherReading,
} from "../model/insights";

export function useInsights(plantId: string) {
  const router = useRouter();
  const plant = usePlant(plantId);
  const p = plant.data;
  const gardenId = p?.garden_id ?? "";
  const garden = useGarden(gardenId);
  const timeline = useInsightTimeline(plantId);
  const heights = usePlantMeasurements(plantId, "plant_height_cm");
  const spots = usePlantMeasurements(plantId, "affected_area_pct");
  const tasks = useAllTasks(gardenId);
  const map = useGardenMap(gardenId);
  const plants = useAllPlants(gardenId);
  const near = nearestPlant(map.data, plantId);
  const neighbourTimeline = useInsightTimeline(near?.plantId ?? "");

  const tz = garden.data?.time_zone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
  const entries = timeline.data ?? [];
  const cells = p ? stripCells(entries, p.status, tz) : [];
  const spotRead = spotsReading(spots.data ?? [], tz);
  const lanes = stepLanes(tasks.data ?? [], plantId, tz);
  const neighbour = near ? plants.data?.find((x) => x.id === near.plantId) : undefined;
  const diag = diagnosisEntry(entries);

  return {
    loading: plant.isPending || timeline.isPending,
    notFound: isApiError(plant.error) && plant.error.status === 404,
    error: (plant.error && !(isApiError(plant.error) && plant.error.status === 404)) || timeline.error ? (plant.error ?? timeline.error) : undefined,
    retry: () => void Promise.all([plant.refetch(), timeline.refetch()]),
    plant: p,
    range: rangeText(entries, tz),
    stale: notCheckedRecently(entries),
    health: { cells, headline: healthHeadline(entries, cells, tz), summary: stripSummary(cells) },
    spots: spots.isPending ? undefined : spotRead,
    lanes,
    after: afterSteps(lanes, spotRead),
    height: heights.isPending ? undefined : heightReading(heights.data ?? [], tz),
    heightLoaded: !heights.isPending,
    weather: weatherReading(entries, tz),
    nearby:
      near && neighbour && neighbourTimeline.data
        ? { plantId: neighbour.id, name: neighbour.name, status: neighbour.status, cm: near.cm, line: nearbyLine(neighbourTimeline.data, diag?.primary, tz) }
        : undefined,
    tz,
    actions: {
      back: () => (router.canGoBack() ? router.back() : router.dismissAll()),
      openScan: (scanId: string) => {
        const e = entries.find((x) => x.scan_id === scanId);
        if (e?.plan_id) router.push({ pathname: "/scans/[scanId]/result", params: { scanId } });
        else router.push({ pathname: "/scans/[scanId]", params: { scanId } });
      },
      openPlant: (id: string) => router.push({ pathname: "/plants/[plantId]", params: { plantId: id } }),
    },
  };
}
