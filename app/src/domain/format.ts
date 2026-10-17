import { formatDistanceToNowStrict } from "date-fns";

import type { ConfidenceBand, PlantStatus } from "./types";

// Units, confidence bands and dates (docs/frontend 6). Bands and statuses are always words, never colour alone.

export const BAND_LABEL: Record<ConfidenceBand, string> = { high: "High", medium: "Medium", low: "Low" };

export const PLANT_STATUS_LABEL: Record<PlantStatus, string> = {
  healthy: "Healthy",
  watch: "Watch",
  sick: "Needs care",
  dead: "Dead",
};

export function celsius(t: number): string {
  return `${Math.round(t)}°C`;
}

export function millimetres(mm: number): string {
  return mm >= 10 ? `${Math.round(mm)} mm` : `${mm.toFixed(1)} mm`;
}

export function percent(p: number): string {
  return `${Math.round(p)}%`;
}

export function ago(timestamp: string, now: Date = new Date()): string {
  const date = new Date(timestamp);
  if (Math.abs(now.getTime() - date.getTime()) < 60_000) return "just now";
  return `${formatDistanceToNowStrict(date)} ago`;
}
