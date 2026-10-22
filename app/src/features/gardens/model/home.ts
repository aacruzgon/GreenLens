// The garden home's pure selectors (screens 3.1): plants by health, the hero photo, and which tasks are
// today's in the garden's time zone. No React, no api (README 5).
import { format } from "date-fns";
import { TZDate } from "@date-fns/tz";

import type { LocalDate, Memory, Plant, PlantStatus, Task } from "@/domain/types";

/** Sick first, then watch, healthy and dead (roadmap F1 item 4). */
const HEALTH_ORDER: Record<PlantStatus, number> = { sick: 0, watch: 1, healthy: 2, dead: 3 };

/** Plants sorted by health, sick first; the server's order is kept within a status. */
export function sortPlantsByHealth<T extends Pick<Plant, "status">>(plants: readonly T[]): T[] {
  return plants
    .map((plant, i) => ({ plant, i }))
    .sort((a, b) => HEALTH_ORDER[a.plant.status] - HEALTH_ORDER[b.plant.status] || a.i - b.i)
    .map(({ plant }) => plant);
}

export interface HeroPhoto {
  plantId: string;
  plantName: string;
  url: string;
  imageId?: string;
}

/**
 * The photo at the top of the home: the garden presented like a showroom (design 3.1), so a healthy plant's
 * latest whole-plant shot is preferred, the most recent first. Without one, any plant's photo; without any, none.
 */
export function pickHeroPhoto(plants: readonly Plant[]): HeroPhoto | null {
  const withPhoto = plants.filter((p) => p.latest_scan?.thumbnail_url && p.status !== "dead");
  if (withPhoto.length === 0) return null;
  const recentFirst = [...withPhoto].sort((a, b) => b.latest_scan!.created_at.localeCompare(a.latest_scan!.created_at));
  const pick = recentFirst.find((p) => p.status === "healthy") ?? recentFirst[0]!;
  const scan = pick.latest_scan!;
  return { plantId: pick.id, plantName: pick.name, url: scan.thumbnail_url!, imageId: scan.thumbnail_image_id };
}

/** "Scan not submitted" for a plant whose latest scan is still waiting for its photos (screens 3.1). */
export function plantCardNote(plant: Pick<Plant, "latest_scan">): string | undefined {
  return plant.latest_scan?.status === "pending" ? "Scan not submitted" : undefined;
}

/** Today's date in the garden's calendar, as a LocalDate ("2026-10-19"). */
export function localToday(timeZone: string, now: Date = new Date()): LocalDate {
  return format(new TZDate(now.getTime(), timeZone), "yyyy-MM-dd");
}

export function isOpenTask(task: Pick<Task, "done_at" | "dismissed_at">): boolean {
  return !task.done_at && !task.dismissed_at;
}

function byDueThenCreated(a: Task, b: Task): number {
  const da = a.due ?? "9999-12-31";
  const db = b.due ?? "9999-12-31";
  if (da !== db) return da < db ? -1 : 1;
  return a.created_at.localeCompare(b.created_at);
}

/** Open forecast alerts, soonest first: the frost cards under the weather (screens 3.1, alert inbox). */
export function openAlerts(tasks: readonly Task[]): Task[] {
  return tasks.filter((t) => t.source === "forecast_alert" && isOpenTask(t)).sort(byDueThenCreated);
}

/**
 * Today's list: open tasks due today or overdue, in the garden's calendar, oldest first. Forecast alerts
 * have their own cards above, so they are left out here. `checked` holds tasks checked on this screen:
 * they stay in the list, drawn done, after the open list refetches without them.
 */
export function todayTasks(tasks: readonly Task[], today: LocalDate, checked: readonly Task[] = []): Task[] {
  const listed = new Set(tasks.map((t) => t.id));
  const keep = new Set(checked.map((t) => t.id));
  return [...tasks, ...checked.filter((t) => !listed.has(t.id))]
    .filter((t) => t.source !== "forecast_alert")
    .filter((t) => keep.has(t.id) || (isOpenTask(t) && t.due !== undefined && t.due <= today))
    .sort(byDueThenCreated);
}

/** A plan's repeating task on a plant is a monitoring check: it leads to a scan of that plant (roadmap F1 item 5). */
export function scanTargetOf(task: Pick<Task, "source" | "plant_id" | "repeat">): string | undefined {
  return task.source === "plan" && task.repeat && task.plant_id ? task.plant_id : undefined;
}

/**
 * A jar-test task's scan, which opens that scan's jar capture (flows 8). Only a user's task carries a scan link on the
 * server, and only the jar test makes one (contracts 9); a plan's task opens a new scan instead (scanTargetOf).
 */
export function jarScanOf(task: Pick<Task, "source" | "scan_id">): string | undefined {
  return task.source === "user" ? task.scan_id : undefined;
}

/** "Toronto" from "Toronto, ON": the place in the hero's eyebrow. */
export function placeName(locationLabel: string): string {
  return locationLabel.split(",")[0]!.trim() || locationLabel;
}

/** Mixes two #RRGGBB colours: t = 0 gives a, 1 gives b. */
export function mixHex(a: string, b: string, t: number): string {
  const k = Math.min(1, Math.max(0, t));
  const ch = (hex: string, i: number) => parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16);
  const out = [0, 1, 2].map((i) => Math.round(ch(a, i) + (ch(b, i) - ch(a, i)) * k));
  return `#${out.map((v) => v.toString(16).padStart(2, "0")).join("").toUpperCase()}`;
}

/** The empty plants section's sentence (screens 4): the action only when the role can scan. */
export function emptyPlantsTitle(canScan: boolean): string {
  return canScan ? "Add your first plant by scanning it" : "No plants yet";
}

/** Garden notes: the memories with no plant (screens 3.1). */
export function gardenNotes(memories: readonly Pick<Memory, "id" | "text" | "plant_id">[]): { id: string; text: string }[] {
  return memories.filter((m) => !m.plant_id).map((m) => ({ id: m.id, text: m.text }));
}
