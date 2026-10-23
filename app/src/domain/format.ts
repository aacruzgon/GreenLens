// Units, confidence band words and local dates (in the garden's time zone).
import { addDays, format, formatDistanceStrict, isSameDay } from "date-fns";
import { TZDate } from "@date-fns/tz";
import type { ConfidenceBand, LocalDate, MeasureQuantity } from "./types";

export const BAND_WORD: Record<ConfidenceBand, string> = {
  low: "low",
  medium: "medium",
  high: "high",
};

export function bandWord(band: ConfidenceBand): string {
  return BAND_WORD[band];
}

const UNIT: Record<MeasureQuantity, string> = {
  plant_height_cm: "cm",
  canopy_width_cm: "cm",
  leaf_length_cm: "cm",
  leaf_area_cm2: "cm²",
  lesion_diameter_mm: "mm",
  lesion_count: "",
  affected_area_pct: "%",
  hole_diameter_mm: "mm",
  hole_count: "",
};

export function unitOf(q: MeasureQuantity): string {
  return UNIT[q];
}

/** Round to a precision the value carries: whole numbers from 10 up, one decimal below. */
function roundForDisplay(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 10) return String(Math.round(value));
  return String(Math.round(value * 10) / 10);
}

/** "2 mm", "34 cm", "12 %"; counts have no unit. */
export function formatMeasurement(q: MeasureQuantity, value: number): string {
  const unit = UNIT[q];
  const n = roundForDisplay(value);
  if (unit === "") return n;
  return unit === "%" ? `${n} %` : `${n} ${unit}`;
}

/** "about 2 mm", the wording for a lesion or hole size. */
export function formatApprox(q: MeasureQuantity, value: number): string {
  return `about ${formatMeasurement(q, value)}`;
}

/** An ISO timestamp as a date in `timeZone` (IANA). */
export function inZone(iso: string, timeZone: string): TZDate {
  const d = new Date(iso);
  return new TZDate(d.getTime(), timeZone);
}

/** "Oct 19, 2026" in the garden's time zone. */
export function formatDate(iso: string, timeZone: string): string {
  return format(inZone(iso, timeZone), "MMM d, yyyy");
}

/** "Oct 19, 2026, 3:05 PM" in the garden's time zone. */
export function formatDateTime(iso: string, timeZone: string): string {
  return format(inZone(iso, timeZone), "MMM d, yyyy, h:mm a");
}

/** "Today", "Yesterday" or the date, by the garden's calendar. */
export function formatDay(iso: string, timeZone: string, now: Date = new Date()): string {
  const d = inZone(iso, timeZone);
  const today = new TZDate(now.getTime(), timeZone);
  if (isSameDay(d, today)) return "Today";
  const yesterday = new TZDate(today.getTime(), timeZone);
  yesterday.setDate(yesterday.getDate() - 1);
  if (isSameDay(d, yesterday)) return "Yesterday";
  return format(d, "MMM d");
}

/** "3 days ago". */
export function formatAgo(iso: string, now: Date = new Date()): string {
  return formatDistanceStrict(new Date(iso), now, { addSuffix: true });
}

/** A LocalDate ("2026-10-19") as "Oct 19". It has no zone, so it is not converted. */
export function formatLocalDate(local: string): string {
  const [y = 1970, m = 1, d = 1] = local.split("-").map(Number);
  return format(new Date(y, m - 1, d), "MMM d");
}

/** Today's date in the garden's calendar, as a LocalDate ("2026-10-19"). */
export function localToday(timeZone: string, now: Date = new Date()): LocalDate {
  return format(new TZDate(now.getTime(), timeZone), "yyyy-MM-dd");
}

/** A LocalDate `days` later (or earlier, when negative). Calendar arithmetic, no time zone. */
export function addLocalDays(local: LocalDate, days: number): LocalDate {
  const [y = 1970, m = 1, d = 1] = local.split("-").map(Number);
  return format(addDays(new Date(y, m - 1, d), days), "yyyy-MM-dd");
}
