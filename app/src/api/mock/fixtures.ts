import { Asset } from "expo-asset";

import type { Citation, Garden, Me, Plant, Task, WeatherSummary } from "@/domain/types";

import { localDate, ts, ulid } from "./util";

// The mock's starting data (docs/frontend 15, fixture store). Shapes come from the generated types,
// so a contract change that breaks these fails type checking.

const created = ts(new Date(Date.now() - 20 * 86_400_000));

export function seedMe(): Me {
  return { id: ulid(), display_name: "", location_consent: false, created_at: created };
}

export function seedGarden(): Garden {
  return {
    id: ulid(),
    name: "Back garden",
    my_role: "owner",
    location_label: "Toronto, ON",
    location_precision: "postal_code",
    time_zone: "America/Toronto",
    soil_profile_status: "ready",
    member_count: 3,
    created_at: created,
  };
}

// Stand-ins for the plants' photos: the design mockups' drawings (docs/design/mockups), rendered to JPEG.
const PHOTOS = {
  tomato: require("../../../assets/mock/tomato.jpg"),
  pepper: require("../../../assets/mock/pepper.jpg"),
  basil: require("../../../assets/mock/basil.jpg"),
  lettuce: require("../../../assets/mock/lettuce.jpg"),
  kale: require("../../../assets/mock/kale.jpg"),
} as const;

function photo(kind: keyof typeof PHOTOS): string {
  return Asset.fromModule(PHOTOS[kind]).uri;
}

const daysAgo = (n: number) => ts(new Date(Date.now() - n * 86_400_000));

/**
 * The design's demo story (docs/design/design.md 5; analytics 6, seeded data): Tomato 1 was diagnosed with
 * early blight 6 days ago and is recovering; its plan asks for a weekly check of Tomato 2, 40 cm away,
 * which is due today and leads to the live scan. Pepper's latest result is uncertain, so it is watch.
 */
export function seedPlants(gardenId: string): Plant[] {
  const base = { garden_id: gardenId, species_candidates: [], created_at: created };
  const scanned = (kind: keyof typeof PHOTOS, days: number) => {
    const imageId = ulid();
    return { scan_id: ulid(), created_at: daysAgo(days), status: "planned" as const, thumbnail_image_id: imageId, thumbnail_url: photo(kind) };
  };
  const tomato = { id: "tomato", scientific_name: "Solanum lycopersicum", common_name: "Tomato" };
  return [
    { ...base, id: ulid(), name: "Tomato 1", species: tomato, species_status: "confirmed", status: "sick", latest_scan: scanned("tomato", 0) },
    {
      ...base,
      id: ulid(),
      name: "Pepper",
      species: { id: "pepper", scientific_name: "Capsicum annuum", common_name: "Pepper" },
      species_status: "confirmed",
      status: "watch",
      latest_scan: scanned("pepper", 0),
    },
    {
      ...base,
      id: ulid(),
      name: "Basil",
      species: { id: "basil", scientific_name: "Ocimum basilicum", common_name: "Basil" },
      species_status: "confirmed",
      status: "healthy",
      latest_scan: scanned("basil", 0),
    },
    { ...base, id: ulid(), name: "Tomato 2", species: tomato, species_status: "confirmed", status: "healthy", latest_scan: scanned("tomato", 4) },
    {
      ...base,
      id: ulid(),
      name: "Lettuce",
      species: { id: "lettuce", scientific_name: "Lactuca sativa", common_name: "Lettuce" },
      species_status: "confirmed",
      status: "healthy",
      latest_scan: scanned("lettuce", 0),
    },
    {
      ...base,
      id: ulid(),
      name: "Kale",
      species: { id: "kale", scientific_name: "Brassica oleracea", common_name: "Kale" },
      species_status: "suggested",
      status: "healthy",
      latest_scan: scanned("kale", 0),
    },
  ];
}

/** Frost is forecast 5 days out ("Thursday" in the story, whatever today is). */
export const FROST_DAY = 5;

export function seedWeather(): WeatherSummary {
  const forecast = Array.from({ length: 7 }, (_, i) => ({
    date: localDate(i),
    t_min_c: [8, 6, 4, 2, 1, -1, 3][i]!,
    t_max_c: [14, 12, 11, 10, 8, 6, 9][i]!,
    precip_mm: [3, 1.2, 0, 0, 0, 0.4, 2.1][i]!,
    mean_rh_pct: [92, 88, 80, 72, 70, 66, 78][i]!,
    hours_rh_over_90: [14, 10, 6, 0, 0, 0, 4][i]!,
    frost_risk: i === FROST_DAY,
  }));
  return {
    provider: "open_meteo",
    fetched_at: ts(),
    location_precision: "postal_code",
    current: { temp_c: 14.2, rh_pct: 92, wind_kmh: 9, precip_mm_last_24h: 3 },
    past_7_days: { precip_mm: 21.4, mean_rh_pct: 89, hours_rh_over_90: 61, longest_humid_run_days: 3 },
    forecast,
    soil: { moisture_0_9cm_m3m3: 0.24, moisture_9_27cm_m3m3: 0.28, temp_6cm_c: 10.6, et0_mm_last_24h: 1.1 },
  };
}

export function seedTasks(gardenId: string, plants: Plant[]): Task[] {
  const [tomato1, , , tomato2] = plants;
  const frost = localDate(FROST_DAY);
  const night = new Date(`${frost}T12:00:00`).toLocaleDateString("en-CA", { weekday: "long" });
  return [
    {
      id: ulid(),
      garden_id: gardenId,
      source: "forecast_alert",
      title: `Frost on ${night} night`,
      detail: `Cover the tomatoes and peppers by ${night} evening. Low of −1 °C.`,
      due: localDate(FROST_DAY - 1),
      trigger: { kind: "frost", date: frost },
      created_at: ts(),
    },
    {
      id: ulid(),
      garden_id: gardenId,
      plant_id: tomato1!.id,
      source: "plan",
      title: "Check Tomato 1 for new spots",
      detail: "Tomato 1 · Monitoring · every 3 days",
      due: localDate(0),
      repeat: { every_days: 3 },
      created_at: daysAgo(6),
    },
    {
      id: ulid(),
      garden_id: gardenId,
      plant_id: tomato2!.id,
      source: "plan",
      title: "Check Tomato 2's lower leaves",
      detail: "Tomato 2 · Monitoring · weekly, from Tomato 1's plan",
      due: localDate(0),
      repeat: { every_days: 7 },
      created_at: daysAgo(6),
    },
  ];
}

export const CITATIONS: Record<string, Citation> = {
  omafra_early_blight: {
    source_id: "omafra_early_blight",
    title: "Early blight of tomato",
    publisher: "Ontario Ministry of Agriculture, Food and Agribusiness",
    url: "https://www.ontario.ca/page/early-blight-tomato",
  },
  umn_tomato_diseases: {
    source_id: "umn_tomato_diseases",
    title: "Early blight in tomato and potato",
    publisher: "University of Minnesota Extension",
    url: "https://extension.umn.edu/disease-management/early-blight-tomato-and-potato",
  },
};
