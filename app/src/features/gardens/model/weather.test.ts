import type { ForecastDay, WeatherSummary } from "@/domain/types";

import { dayKind, heroWeatherLine, skyWords, upcomingDays, weatherView } from "./weather";

function day(date: string, o: Partial<ForecastDay> = {}): ForecastDay {
  return { date, t_min_c: 6, t_max_c: 13, mean_rh_pct: 62, hours_rh_over_90: 0, precip_mm: 0, frost_risk: false, ...o };
}

// Monday Oct 19, 2026: frost on Thursday Oct 22.
const weather: WeatherSummary = {
  provider: "open_meteo",
  fetched_at: "2026-10-19T14:00:00Z",
  location_precision: "city",
  current: { temp_c: 11.2, rh_pct: 84, precip_mm_last_24h: 3, wind_kmh: 9 },
  past_7_days: { hours_rh_over_90: 61, longest_humid_run_days: 3, mean_rh_pct: 82, precip_mm: 14 },
  soil: { et0_mm_last_24h: 1.2, moisture_0_9cm_m3m3: 0.31, moisture_9_27cm_m3m3: 0.34, temp_6cm_c: 9.5 },
  forecast: [
    day("2026-10-20", { precip_mm: 3.5, mean_rh_pct: 88 }),
    day("2026-10-21", { mean_rh_pct: 85 }),
    day("2026-10-22", { t_min_c: -1, t_max_c: 6, frost_risk: true }),
    day("2026-10-23", { mean_rh_pct: 70 }),
    day("2026-10-24"),
    day("2026-10-25"),
    day("2026-10-26"),
  ],
} as WeatherSummary;

describe("weatherView", () => {
  const view = weatherView(weather, "America/Toronto", "2026-10-19");

  it("shows the next six days with the frost day marked", () => {
    expect(view.days.map((d) => d.label)).toEqual(["Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
    const frost = view.days.filter((d) => d.frost);
    expect(frost.map((d) => d.label)).toEqual(["Thu"]);
    expect(frost[0]!.low).toBe("−1°");
    expect(frost[0]!.accessibilityLabel).toBe("Thursday, high 6°, low −1°, frost possible");
    expect(view.frostNote).toBe("Frost possible Thursday");
  });

  it("notes frost tonight, though today has no column", () => {
    const tonight = weatherView(weather, "America/Toronto", "2026-10-22");
    expect(tonight.frostNote).toBe("Frost possible tonight");
    expect(tonight.days.some((d) => d.frost)).toBe(false);
  });

  it("gives when it was fetched, in the garden's time zone", () => {
    expect(view.updated).toBe("Updated 10:00");
  });
});

describe("heroWeatherLine", () => {
  it("reads now and the coming frost", () => {
    expect(heroWeatherLine(weather, "2026-10-19")).toBe("11 °C after light rain, 84% humidity. Frost possible Thursday night.");
  });

  it("says tonight for today's frost, tomorrow for tomorrow's and nothing without one", () => {
    expect(heroWeatherLine(weather, "2026-10-22")).toBe("11 °C after light rain, 84% humidity. Frost possible tonight.");
    expect(heroWeatherLine(weather, "2026-10-21")).toBe("11 °C after light rain, 84% humidity. Frost possible tomorrow night.");
    expect(heroWeatherLine(weather, "2026-10-23")).toBe("11 °C after light rain, 84% humidity.");
  });

  it("words the sky from the past day's rain and the humidity", () => {
    expect([0, 0.4, 3, 12].map((precip_mm_last_24h) => skyWords({ precip_mm_last_24h, rh_pct: 84 }))).toEqual([
      "and cloudy",
      "and cloudy",
      "after light rain",
      "after rain",
    ]);
    expect(skyWords({ precip_mm_last_24h: 0, rh_pct: 60 })).toBe("and partly cloudy");
  });
});

describe("dayKind and upcomingDays", () => {
  it("draws frost, rain, cloud and sun from the numbers", () => {
    expect(dayKind(day("d", { frost_risk: true, precip_mm: 5 }))).toBe("snow");
    expect(dayKind(day("d", { precip_mm: 2 }))).toBe("rain");
    expect(dayKind(day("d", { mean_rh_pct: 85 }))).toBe("cloud");
    expect(dayKind(day("d", { mean_rh_pct: 70 }))).toBe("partly");
    expect(dayKind(day("d"))).toBe("sun");
  });

  it("skips today and the past", () => {
    expect(upcomingDays([day("2026-10-19"), day("2026-10-20")], "2026-10-19").map((d) => d.date)).toEqual(["2026-10-20"]);
  });
});
