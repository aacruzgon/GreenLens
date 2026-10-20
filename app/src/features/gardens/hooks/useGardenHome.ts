import { useRouter } from "expo-router";

import { flattenPages, imageSource, isApiError, useGarden, useGardens, useGardenWeather, useMemories, usePlants, useTasks } from "@/api";
import { can } from "@/domain/permissions";
import type { Task } from "@/domain/types";
import { taskMeta, useCompleteTask } from "@/features/tasks";

import { gardenNotes, localToday, openAlerts, pickHeroPhoto, placeName, plantCardNote, scanTargetOf, sortPlantsByHealth, todayTasks } from "../model/home";
import type { AlertView, GardenChoice, Loadable, PlantCardView, TaskRowView, WeatherCardState } from "../model/views";
import { heroWeatherLine, weatherView, weekday } from "../model/weather";
import { useActiveGardenStore } from "./activeGardenStore";

export type { AlertView, TaskRowView } from "../model/views";

function messageOf(error: unknown): string | undefined {
  return isApiError(error) ? error.message : undefined;
}

// The garden home's one view-model hook (README 5): joins the garden, its weather, plants and open tasks,
// the role checks and the actions. Components below take its plain data as props.
export function useGardenHome(gardenId: string) {
  const router = useRouter();
  const garden = useGarden(gardenId);
  const gardens = useGardens();
  const weather = useGardenWeather(gardenId);
  const plants = usePlants(gardenId);
  const memories = useMemories(gardenId);
  const tasks = useTasks(gardenId, { state: "open" });
  const { complete, done } = useCompleteTask();

  const g = garden.data;
  // Dates depend on the garden's time zone; until it loads they are unknown, and so are the sections built on them.
  const tz = g?.time_zone;
  const today = tz ? localToday(tz) : undefined;
  const gardenList = flattenPages(gardens.data);
  const gardenChoices: GardenChoice[] = gardenList.map((x) => ({ id: x.id, name: x.name }));
  const role = g?.my_role;
  const canWriteTasks = can(role, "task.write");
  const canScan = can(role, "scan.create");
  const canNote = can(role, "memory.note");

  const plantList = flattenPages(plants.data);
  const plantName = (id: string) => plantList.find((p) => p.id === id)?.name;
  const taskList = flattenPages(tasks.data);
  const checked = [...done.values()];

  const hero = pickHeroPhoto(plantList);
  const w = weather.data?.weather;

  const weatherState: WeatherCardState = w && tz && today
    ? { status: "ready", view: weatherView(w, tz, today) }
    : weather.error
      ? isApiError(weather.error) && weather.error.status === 503
        ? { status: "unavailable", onRetry: () => void weather.refetch() }
        : { status: "error", message: messageOf(weather.error), onRetry: () => void weather.refetch() }
      : { status: "loading" };

  const plantsView: Loadable<{ items: PlantCardView[] }> = plants.data
    ? {
        status: "ready",
        items: sortPlantsByHealth(plantList).map((p) => ({
          id: p.id,
          name: p.name,
          status: p.status,
          photo: imageSource(p.latest_scan?.thumbnail_url, p.latest_scan?.thumbnail_image_id),
          note: plantCardNote(p),
        })),
      }
    : plants.error
      ? { status: "error", message: messageOf(plants.error), retry: () => void plants.refetch() }
      : { status: "loading" };

  const toRow = (t: Task, day: string): TaskRowView => {
    const target = canScan ? scanTargetOf(t) : undefined;
    const name = target ? plantName(target) : undefined;
    return {
      id: t.id,
      title: t.title,
      meta: taskMeta(t, plantName, day),
      done: done.has(t.id),
      scanHint: target ? `Starts a scan of ${name ?? "this plant"}` : undefined,
    };
  };

  const todayView: Loadable<{ items: TaskRowView[] }> = tasks.data && today
    ? { status: "ready", items: todayTasks(taskList, today, checked).map((t) => toRow(t, today)) }
    : tasks.error
      ? { status: "error", message: messageOf(tasks.error), retry: () => void tasks.refetch() }
      : { status: "loading" };

  // An alert done here leaves at once (roadmap F1 exit checks: Done removes it).
  const alerts: AlertView[] = openAlerts(taskList)
    .filter((t) => !done.has(t.id))
    .map((t) => ({ id: t.id, title: t.title, detail: t.detail }));

  const notesView: Loadable<{ items: { id: string; text: string }[] }> = memories.data
    ? { status: "ready", items: gardenNotes(flattenPages(memories.data)) }
    : memories.error
      ? { status: "error", message: messageOf(memories.error), retry: () => void memories.refetch() }
      : { status: "loading" };

  const byId = (id: string) => taskList.find((t) => t.id === id);

  return {
    garden: g
      ? {
          status: "ready" as const,
          name: g.name,
          eyebrow: today ? `${placeName(g.location_label)} · ${weekday(today)}` : placeName(g.location_label),
          weatherLine: w && today ? heroWeatherLine(w, today) : undefined,
          note: g.soil_profile_status === "pending" ? "Soil data loading" : undefined,
        }
      : garden.error
        ? { status: "error" as const, message: messageOf(garden.error), retry: () => void garden.refetch() }
        : { status: "loading" as const },
    // While the plants load the hero keeps its full height (photo undefined), so nothing jumps when it arrives.
    hero: hero
      ? { photo: imageSource(hero.url, hero.imageId), label: hero.plantName }
      : { photo: plants.data ? null : undefined, label: "" },
    weather: weatherState,
    alerts,
    plants: plantsView,
    today: todayView,
    // Garden notes: memories with no plant (screens 3.1).
    notes: notesView,
    // The garden switcher lists the user's gardens when there are two or more (screens 3.1).
    gardens: { items: gardenChoices, activeId: gardenId },
    can: { writeTasks: canWriteTasks, note: canNote, scan: canScan, addPlant: can(role, "plant.write") },
    actions: {
      refresh: async () => {
        await Promise.all([garden.refetch(), weather.refetch(), plants.refetch(), tasks.refetch(), memories.refetch()]);
      },
      completeTask: (id: string) => {
        const task = byId(id);
        if (task && canWriteTasks) complete(task);
      },
      openTask: (id: string) => {
        const task = byId(id);
        const plantId = task && canScan ? scanTargetOf(task) : undefined;
        if (plantId) router.push({ pathname: "/scan/new", params: { plantId } });
      },
      addNote: () => router.push("/notes/new"),
      switchGarden: (id: string) => useActiveGardenStore.getState().setGardenId(id),
      // The new-scan sheet with nothing chosen; it lets the user name a new plant.
      addPlant: () => router.push("/scan/new"),
    },
  };
}

export type GardenHomeView = ReturnType<typeof useGardenHome>;
