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
    name: "Backyard",
    my_role: "owner",
    location_label: "Toronto, ON",
    location_precision: "postal_code",
    time_zone: "America/Toronto",
    soil_profile_status: "ready",
    member_count: 3,
    created_at: created,
  };
}

export function seedPlants(gardenId: string): Plant[] {
  const base = { garden_id: gardenId, species_candidates: [], created_at: created };
  return [
    {
      ...base,
      id: ulid(),
      name: "Tomato by the fence",
      species: { id: "tomato", scientific_name: "Solanum lycopersicum", common_name: "Tomato" },
      species_status: "confirmed",
      status: "watch",
      latest_scan: { scan_id: ulid(), created_at: ts(new Date(Date.now() - 3 * 86_400_000)), status: "planned" },
    },
    {
      ...base,
      id: ulid(),
      name: "Basil pot",
      species: { id: "basil", scientific_name: "Ocimum basilicum", common_name: "Basil" },
      species_status: "confirmed",
      status: "healthy",
      latest_scan: { scan_id: ulid(), created_at: ts(new Date(Date.now() - 6 * 86_400_000)), status: "planned" },
    },
    {
      ...base,
      id: ulid(),
      name: "Zucchini, back bed",
      species: { id: "zucchini", scientific_name: "Cucurbita pepo", common_name: "Zucchini" },
      species_status: "suggested",
      status: "sick",
      latest_scan: { scan_id: ulid(), created_at: ts(new Date(Date.now() - 1 * 86_400_000)), status: "planned" },
    },
    {
      ...base,
      id: ulid(),
      name: "Kale",
      species_status: "unknown",
      status: "healthy",
    },
  ];
}

export function seedWeather(): WeatherSummary {
  const forecast = Array.from({ length: 7 }, (_, i) => ({
    date: localDate(i),
    t_min_c: [9, 7, 4, 1, 0, 3, 6][i]!,
    t_max_c: [17, 15, 12, 9, 8, 11, 14][i]!,
    precip_mm: [0, 4.2, 11.5, 2.1, 0, 0, 1.3][i]!,
    mean_rh_pct: [72, 88, 94, 85, 70, 66, 74][i]!,
    hours_rh_over_90: [2, 9, 16, 6, 0, 0, 3][i]!,
    frost_risk: i === 4,
  }));
  return {
    provider: "open_meteo",
    fetched_at: ts(),
    location_precision: "postal_code",
    current: { temp_c: 14.2, rh_pct: 78, wind_kmh: 12, precip_mm_last_24h: 0.4 },
    past_7_days: { precip_mm: 18.6, mean_rh_pct: 84, hours_rh_over_90: 41, longest_humid_run_days: 3 },
    forecast,
    soil: { moisture_0_9cm_m3m3: 0.21, moisture_9_27cm_m3m3: 0.27, temp_6cm_c: 11.4, et0_mm_last_24h: 1.6 },
  };
}

export function seedTasks(gardenId: string, plants: Plant[]): Task[] {
  const tomato = plants[0]!;
  const zucchini = plants[2]!;
  return [
    {
      id: ulid(),
      garden_id: gardenId,
      source: "forecast_alert",
      title: "Frost likely on " + localDate(4),
      detail: "Cover tender plants in the evening, or move pots indoors.",
      due: localDate(3),
      trigger: { kind: "frost", date: localDate(4) },
      created_at: ts(),
    },
    {
      id: ulid(),
      garden_id: gardenId,
      plant_id: tomato.id,
      source: "plan",
      title: "Check the lower leaves for new spots",
      detail: "Look at the leaves closest to the soil, top and underside.",
      due: localDate(0),
      repeat: { every_days: 3 },
      created_at: ts(),
    },
    {
      id: ulid(),
      garden_id: gardenId,
      plant_id: zucchini.id,
      source: "plan",
      title: "Water at the base in the morning",
      detail: "Keep the leaves dry to slow powdery mildew.",
      due: localDate(1),
      created_at: ts(),
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
