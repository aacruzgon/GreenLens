import type { Plant, Task } from "@/domain/types";

import { emptyPlantsTitle, gardenNotes, isOpenTask, jarScanOf, localToday, mixHex, openAlerts, pickHeroPhoto, placeName, plantCardNote, scanTargetOf, sortPlantsByHealth, todayTasks } from "./home";

function plant(id: string, status: Plant["status"], scan?: { at: string; url?: string; pending?: boolean }): Plant {
  return {
    id,
    garden_id: "g",
    name: id,
    status,
    species_status: "confirmed",
    species_candidates: [],
    created_at: "2026-09-01T10:00:00Z",
    ...(scan
      ? {
          latest_scan: {
            scan_id: `s_${id}`,
            created_at: scan.at,
            status: scan.pending ? "pending" : "complete",
            ...(scan.url ? { thumbnail_url: scan.url, thumbnail_image_id: `img_${id}` } : {}),
          },
        }
      : {}),
  } as Plant;
}

function task(id: string, o: Partial<Task> = {}): Task {
  return { id, garden_id: "g", source: "plan", title: id, created_at: "2026-10-10T10:00:00Z", ...o };
}

describe("sortPlantsByHealth", () => {
  it("puts sick first, then watch, healthy and dead, keeping the server's order within a status", () => {
    const sorted = sortPlantsByHealth([
      plant("basil", "healthy"),
      plant("dead", "dead"),
      plant("pepper", "watch"),
      plant("tomato1", "sick"),
      plant("kale", "healthy"),
    ]);
    expect(sorted.map((p) => p.id)).toEqual(["tomato1", "pepper", "basil", "kale", "dead"]);
  });
});

describe("pickHeroPhoto", () => {
  it("prefers the latest healthy photo", () => {
    const hero = pickHeroPhoto([
      plant("tomato1", "sick", { at: "2026-10-19T10:00:00Z", url: "u1" }),
      plant("basil", "healthy", { at: "2026-10-18T10:00:00Z", url: "u2" }),
      plant("kale", "healthy", { at: "2026-10-17T10:00:00Z", url: "u3" }),
    ]);
    expect(hero).toEqual({ plantId: "basil", plantName: "basil", url: "u2", imageId: "img_basil" });
  });

  it("falls back to any photo, and to none", () => {
    expect(pickHeroPhoto([plant("tomato1", "sick", { at: "2026-10-19T10:00:00Z", url: "u1" })])?.plantId).toBe("tomato1");
    expect(pickHeroPhoto([plant("basil", "healthy")])).toBeNull();
  });
});

describe("plantCardNote", () => {
  it("says a pending scan wasn't submitted", () => {
    expect(plantCardNote(plant("a", "healthy", { at: "2026-10-19T10:00:00Z", pending: true }))).toBe("Scan not submitted");
    expect(plantCardNote(plant("a", "healthy", { at: "2026-10-19T10:00:00Z" }))).toBeUndefined();
  });
});

describe("localToday", () => {
  it("uses the garden's calendar, not the device's", () => {
    const now = new Date("2026-10-20T02:30:00Z");
    expect(localToday("America/Toronto", now)).toBe("2026-10-19");
    expect(localToday("Europe/Berlin", now)).toBe("2026-10-20");
  });
});

describe("todayTasks", () => {
  const today = "2026-10-19";
  const tasks = [
    task("later", { due: "2026-10-22" }),
    task("today", { due: today, created_at: "2026-10-15T10:00:00Z" }),
    task("overdue", { due: "2026-10-17" }),
    task("no_due"),
    task("done", { due: today, done_at: "2026-10-19T08:00:00Z" }),
    task("dismissed", { due: today, dismissed_at: "2026-10-19T08:00:00Z" }),
    task("frost", { due: today, source: "forecast_alert" }),
  ];

  it("lists open tasks due today or overdue, oldest first, without forecast alerts", () => {
    expect(todayTasks(tasks, today).map((t) => t.id)).toEqual(["overdue", "today"]);
  });

  it("keeps tasks checked on the screen after the open list drops them", () => {
    const checked = task("checked", { due: today, created_at: "2026-10-01T10:00:00Z" });
    expect(todayTasks(tasks, today, [checked]).map((t) => t.id)).toEqual(["overdue", "checked", "today"]);
  });
});

describe("openAlerts", () => {
  it("lists open forecast alerts, soonest first", () => {
    const alerts = openAlerts([
      task("thu", { source: "forecast_alert", due: "2026-10-21" }),
      task("old", { source: "forecast_alert", due: "2026-10-17", done_at: "2026-10-17T18:00:00Z" }),
      task("tue", { source: "forecast_alert", due: "2026-10-20" }),
      task("plan", { due: "2026-10-19" }),
    ]);
    expect(alerts.map((t) => t.id)).toEqual(["tue", "thu"]);
  });
});

describe("scanTargetOf", () => {
  it("leads a plan's repeating check to a scan of its plant", () => {
    expect(scanTargetOf(task("t2", { plant_id: "tomato2", repeat: { every_days: 7 } }))).toBe("tomato2");
    expect(scanTargetOf(task("mulch", { plant_id: "tomato1" }))).toBeUndefined();
    expect(scanTargetOf(task("user", { source: "user", plant_id: "tomato1", repeat: { every_days: 7 } }))).toBeUndefined();
  });
});

describe("jarScanOf", () => {
  it("leads a user's jar-test task to its scan's jar capture, and never a plan's task", () => {
    expect(jarScanOf({ source: "user", scan_id: "s1" })).toBe("s1");
    expect(jarScanOf({ source: "user" })).toBeUndefined();
    expect(jarScanOf({ source: "plan", scan_id: "s1" })).toBeUndefined();
  });
});

describe("small helpers", () => {
  it("names the place and mixes colours", () => {
    expect(placeName("Toronto, ON")).toBe("Toronto");
    expect(mixHex("#000000", "#FFFFFF", 0.5)).toBe("#808080");
    expect(mixHex("#E3EBDC", "#F6F7F2", 1)).toBe("#F6F7F2");
    expect(isOpenTask({})).toBe(true);
  });
});

describe("emptyPlantsTitle", () => {
  it("offers scanning only to roles that can scan", () => {
    expect(emptyPlantsTitle(true)).toBe("Add your first plant by scanning it");
    expect(emptyPlantsTitle(false)).toBe("No plants yet");
  });
});

describe("gardenNotes", () => {
  it("keeps the memories with no plant", () => {
    const notes = gardenNotes([
      { id: "a", text: "Shade at noon" },
      { id: "b", text: "Tomato 1 wilts", plant_id: "p1" },
    ]);
    expect(notes).toEqual([{ id: "a", text: "Shade at noon" }]);
  });
});
