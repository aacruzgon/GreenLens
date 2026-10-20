// The weather card and the hero's weather line, from GET /gardens/{id}/weather (screens 3.1).
// Weather is shown as a pattern, never a cause (design 6). The contract has no sky condition, so each day's
// pictogram is read from its frost flag, rain and humidity.
import { format } from "date-fns";

import { inZone } from "@/domain/format";
import type { ForecastDay, LocalDate, WeatherSummary } from "@/domain/types";

export type WeatherKind = "snow" | "rain" | "cloud" | "partly" | "sun";

export interface WeatherDayView {
  date: LocalDate;
  /** "Tue". */
  label: string;
  /** "Tuesday". */
  longLabel: string;
  kind: WeatherKind;
  high: string;
  low: string;
  frost: boolean;
  /** What VoiceOver reads for the column. */
  accessibilityLabel: string;
}

export interface WeatherView {
  days: WeatherDayView[];
  /** "Frost possible Thursday", or undefined without a frost day ahead. */
  frostNote?: string;
  /** "Updated 10:00", in the garden's time zone. */
  updated: string;
}

function localDate(d: LocalDate): Date {
  const [y = 1970, m = 1, day = 1] = d.split("-").map(Number);
  return new Date(y, m - 1, day);
}

export function weekday(d: LocalDate, style: "short" | "long" = "long"): string {
  return format(localDate(d), style === "short" ? "EEE" : "EEEE");
}

function degrees(c: number): string {
  // A true minus sign, as the mockup sets it ("−1°").
  const n = Math.round(c);
  return `${n < 0 ? "−" : ""}${Math.abs(n)}°`;
}

function mm(v: number): string {
  return v >= 10 ? String(Math.round(v)) : String(Math.round(v * 10) / 10);
}

export function dayKind(day: Pick<ForecastDay, "frost_risk" | "precip_mm" | "mean_rh_pct">): WeatherKind {
  if (day.frost_risk) return "snow";
  if (day.precip_mm >= 1) return "rain";
  if (day.mean_rh_pct >= 80) return "cloud";
  if (day.mean_rh_pct >= 68) return "partly";
  return "sun";
}

/** The first frost day ahead, if any. */
export function firstFrostDay(forecast: readonly ForecastDay[]): ForecastDay | undefined {
  return forecast.find((d) => d.frost_risk);
}

/** "tomorrow" or the weekday's name, for a date after today. */
function whenWord(date: LocalDate, today: LocalDate): string {
  const next = localDate(today);
  next.setDate(next.getDate() + 1);
  return format(next, "yyyy-MM-dd") === date ? "tomorrow" : weekday(date);
}

/** The forecast days after today, at most `count` (now plus the next six days: roadmap F1 item 2). */
export function upcomingDays(forecast: readonly ForecastDay[], today: LocalDate, count = 6): ForecastDay[] {
  return forecast.filter((d) => d.date > today).slice(0, count);
}

export function weatherView(w: WeatherSummary, timeZone: string, today: LocalDate): WeatherView {
  const days = upcomingDays(w.forecast, today).map((d): WeatherDayView => {
    const longLabel = weekday(d.date);
    const parts = [`${longLabel}`, `high ${degrees(d.t_max_c)}`, `low ${degrees(d.t_min_c)}`];
    if (d.precip_mm >= 1) parts.push(`rain ${mm(d.precip_mm)} mm`);
    if (d.frost_risk) parts.push("frost possible");
    return {
      date: d.date,
      label: weekday(d.date, "short"),
      longLabel,
      kind: dayKind(d),
      high: degrees(d.t_max_c),
      low: degrees(d.t_min_c),
      frost: d.frost_risk,
      accessibilityLabel: parts.join(", "),
    };
  });
  const frost = firstFrostDay(w.forecast.filter((d) => d.date > today));
  return {
    days,
    frostNote: frost ? `Frost possible ${whenWord(frost.date, today)}` : undefined,
    updated: `Updated ${format(inZone(w.fetched_at, timeZone), "HH:mm")}`,
  };
}

/**
 * Now, in words: "light rain" after 1 to 10 mm in the past day, "rain" above that, else "cloudy" or "partly
 * cloudy" from the humidity (the contract has no sky condition; the same reading as the day pictograms).
 */
export function skyWords(current: Pick<WeatherSummary["current"], "precip_mm_last_24h" | "rh_pct">): string {
  const rain = current.precip_mm_last_24h;
  if (rain >= 10) return "rain";
  if (rain >= 1) return "light rain";
  return current.rh_pct >= 80 ? "cloudy" : "partly cloudy";
}

/** The hero's line, as the mockup sets it: "14 °C and light rain, 92% humidity. Frost possible Thursday night." */
export function heroWeatherLine(w: WeatherSummary, today: LocalDate): string {
  const now = `${degrees(w.current.temp_c).replace("°", " °C")} and ${skyWords(w.current)}, ${Math.round(w.current.rh_pct)}% humidity.`;
  const frost = firstFrostDay(w.forecast.filter((d) => d.date > today));
  return frost ? `${now} Frost possible ${whenWord(frost.date, today)} night.` : now;
}
