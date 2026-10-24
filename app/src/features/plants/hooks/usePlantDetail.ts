// The plant page's view model (roadmap 10 item 8; screens 3.5): the plant, its timeline of scans, its notes, the
// role checks and where each thing leads. The growth curve is on Insights (F7); Ask joins with chat in F6.
import { useRouter } from "expo-router";

import { flattenPages, imageSource, isApiError, useGarden, useMemories, usePlant, useTimeline } from "@/api";
import { formatDay } from "@/domain/format";
import { can } from "@/domain/permissions";
import { useStartCapture } from "@/features/capture";

import { speciesStatusWord, timelineRow } from "../model/timeline";

export function usePlantDetail(plantId: string) {
  const router = useRouter();
  const plant = usePlant(plantId);
  const p = plant.data;
  const garden = useGarden(p?.garden_id ?? "");
  const timeline = useTimeline(plantId);
  const memories = useMemories(p?.garden_id ?? "", plantId);
  const startCapture = useStartCapture();

  const role = garden.data?.my_role;
  const tz = garden.data?.time_zone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
  const entries = flattenPages(timeline.data);

  const species = p
    ? p.species
      ? { name: p.species.common_name, latin: p.species.scientific_name, status: speciesStatusWord(p.species_status) }
      : p.custom_species_name
        ? { name: p.custom_species_name, status: speciesStatusWord(p.species_status) }
        : { name: "Species not known yet" }
    : undefined;

  return {
    loading: plant.isPending,
    notFound: isApiError(plant.error) && plant.error.status === 404,
    error: plant.error && !(isApiError(plant.error) && plant.error.status === 404) ? plant.error : undefined,
    retry: () => void plant.refetch(),
    plant: p,
    photo: p?.latest_scan?.thumbnail_url ? imageSource(p.latest_scan.thumbnail_url, p.latest_scan.thumbnail_image_id) : null,
    species,
    timeline: {
      loading: timeline.isPending,
      rows: entries.map((e) => ({
        ...timelineRow(e),
        day: formatDay(e.created_at, tz),
        thumb: imageSource(e.thumbnail_url, e.thumbnail_image_id),
      })),
      more: timeline.hasNextPage ? () => void timeline.fetchNextPage() : undefined,
    },
    notes: flattenPages(memories.data).map((m) => ({ id: m.id, text: m.text, shared: m.shared })),
    can: {
      changeSpecies: can(role, "species.confirm"),
      scan: can(role, "scan.create"),
      note: can(role, "memory.note"),
    },
    scanning: startCapture.isPending,
    actions: {
      back: () => (router.canGoBack() ? router.back() : router.dismissAll()),
      changeSpecies: () => router.push({ pathname: "/plants/[plantId]/species", params: { plantId } }),
      insights: () => router.push({ pathname: "/plants/[plantId]/insights", params: { plantId } }),
      scan: () => void startCapture.start({ plantId }),
      addNote: () => router.push({ pathname: "/notes/new", params: { plantId } }),
      open: (row: { scanId: string; opens: "result" | "scan" | "capture" }) => {
        if (row.opens === "result") router.push({ pathname: "/scans/[scanId]/result", params: { scanId: row.scanId } });
        else if (row.opens === "capture") router.push({ pathname: "/capture/[scanId]", params: { scanId: row.scanId } });
        else router.push({ pathname: "/scans/[scanId]", params: { scanId: row.scanId } });
      },
    },
  };
}
