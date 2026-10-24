// The mock Transport (docs/frontend/README.md 15): a router on method and path that answers the
// REST endpoints from the fixture store with the real status codes and ErrorBody errors, and
// streams scan events as a Server-Sent Events byte stream.

import { polygonArea } from "@/domain/map";

import type { Transport } from "../transport";
import { registerChatRoutes } from "./chat";
import { registerConnectorRoutes } from "./connectors";
import { addMember, myRole, registerMemberRoutes } from "./members";
import { registerInsightRoutes } from "./insights";
import { answerRun, guidedCheck, scriptFor, speciesRerun, startRun, takeOutcome } from "./runs";
import { scenario } from "./scenario";
import { mockStore, type MockStore } from "./store";
import { MOCK_IMAGE_SIZE, mockImageUrl, type MockImageName } from "./images";
import { emptyResponse, errorResponse, jsonResponse, MOCK_ORIGIN, nowTs, paginate, type S } from "./util";

// ---- request context

interface Ctx {
  method: string;
  params: Record<string, string>;
  query: URLSearchParams;
  headers: Headers;
  store: MockStore;
  json(): Promise<unknown>;
  form(): Promise<FormFields>;
}

interface FormFields {
  kind?: string;
  questionId?: string;
  hasDepth: boolean;
  /** The file part's media type, when the runtime exposes it. */
  fileType?: string;
}

type Handler = (c: Ctx) => Response | Promise<Response>;

interface Route {
  method: string;
  re: RegExp;
  keys: string[];
  /** The lowest role allowed, or none. */
  minRole?: S["Role"];
  handler: Handler;
}

const routes: Route[] = [];

const RANK: Record<S["Role"], number> = { viewer: 0, reviewer: 1, editor: 2, owner: 3 };

/** `minRole`: `true` means editor. */
function route(method: string, pattern: string, handler: Handler, minRole: S["Role"] | boolean = false): void {
  const keys: string[] = [];
  const re = new RegExp(
    "^" + pattern.replace(/:([a-z_]+)/g, (_m, k: string) => (keys.push(k), "([^/]+)")) + "/?$",
  );
  routes.push({ method, re, keys, minRole: minRole === true ? "editor" : minRole || undefined, handler });
}

class BadRequest extends Error {
  constructor(readonly field: string, readonly problem: string) {
    super(problem);
  }
}

const invalid = (field: string, problem: string) =>
  errorResponse("invalid_request", { field, problem });

const notFound = () => errorResponse("not_found");

function str(body: Record<string, unknown>, key: string, required = true): string | undefined {
  const v = body[key];
  if (v === undefined || v === null) {
    if (required) throw new BadRequest(key, "required");
    return undefined;
  }
  if (typeof v !== "string" || v.length === 0) throw new BadRequest(key, "must be a non-empty string");
  return v;
}

async function jsonObject(c: Ctx): Promise<Record<string, unknown>> {
  const body = await c.json();
  if (typeof body !== "object" || body === null || Array.isArray(body)) {
    throw new BadRequest("body", "expected a JSON object");
  }
  return body as Record<string, unknown>;
}

// ---- reading bodies, whichever form the runtime hands over

async function readText(input: string | Request, init?: RequestInit): Promise<string | undefined> {
  const b = init?.body;
  if (typeof b === "string") return b;
  if (b instanceof URLSearchParams) return b.toString();
  if (b == null && typeof input !== "string") {
    try {
      return await input.clone().text();
    } catch {
      return undefined;
    }
  }
  return undefined;
}

type FormLike = {
  get?: (name: string) => unknown;
  has?: (name: string) => boolean;
  getParts?: () => { fieldName: string; string?: string }[];
};

async function readForm(input: string | Request, init?: RequestInit): Promise<FormFields> {
  let form: FormLike | undefined;
  if (typeof FormData !== "undefined" && init?.body instanceof FormData) form = init.body as unknown as FormLike;
  else if (init?.body == null && typeof input !== "string" && (input as { _bodyFormData?: unknown })._bodyFormData) {
    // React Native's fetch is whatwg-fetch: its formData() reads the body as text, which throws for a FormData
    // body, so a phone's upload lost its fields and defaulted to whole_plant. The request keeps the form itself.
    form = (input as unknown as { _bodyFormData: FormLike })._bodyFormData;
  } else if (init?.body == null && typeof input !== "string") {
    try {
      form = (await input.clone().formData()) as unknown as FormLike;
    } catch {
      form = undefined;
    }
  }
  const out: FormFields = { hasDepth: false };
  if (!form) return out;
  const field = (name: string): string | undefined => {
    const v = form.get?.(name);
    if (typeof v === "string") return v;
    const part = form.getParts?.().find((p) => p.fieldName === name);
    return typeof part?.string === "string" ? part.string : undefined;
  };
  out.kind = field("kind");
  out.questionId = field("question_id");
  const file = form.get?.("file") as { type?: unknown } | string | null | undefined;
  if (file && typeof file === "object" && typeof file.type === "string" && file.type) out.fileType = file.type;
  out.hasDepth =
    Boolean(form.has?.("depth")) || Boolean(form.getParts?.().some((p) => p.fieldName === "depth"));
  return out;
}

// ---- lookups

const IMAGE_KINDS: readonly S["ImageKind"][] = [
  "whole_plant", "leaf_closeup", "leaf_underside", "soil_surface", "flower_fruit", "squeeze_test", "jar_test",
];
const KIND_LIMIT: Record<S["ImageKind"], number> = {
  whole_plant: 1, leaf_closeup: 2, leaf_underside: 1, soil_surface: 1, flower_fruit: 1,
  squeeze_test: 1, jar_test: 1,
};
const ACCEPTED = ["image/jpeg", "image/png", "image/webp", "image/heic"];

/** Which bundled photo an upload shows: the plant's own, or an early-blight close-up for tomato leaves. */
function photoFor(store: MockStore, plantId: string, kind: S["ImageKind"]): MockImageName {
  const species = store.plants.get(plantId)?.species?.id;
  const own: MockImageName =
    species === "pepper" || species === "basil" || species === "lettuce" || species === "kale" ? species : "tomato";
  if (own === "tomato" && (kind === "leaf_closeup" || kind === "leaf_underside")) return "tomato-blight";
  return own;
}

function gardenView(store: MockStore, g: S["Garden"]): S["Garden"] {
  const members = store.people.members.get(g.id)?.length;
  return { ...g, my_role: myRole(store, g.id) ?? scenario.get().role, member_count: members || g.member_count };
}

/** A GardenLocationInput's label and precision, as the server's geocoder would give them; a string is the problem. */
function locate(loc: unknown): { label: string; precision: S["LocationPrecision"] } | { field: string; problem: string } {
  if (typeof loc !== "object" || loc === null) return { field: "location", problem: "required" };
  const l = loc as Record<string, unknown>;
  const label =
    (typeof l.city === "string" && l.city) || (typeof l.postal_code === "string" && l.postal_code) || "";
  if (!label && !(typeof l.lat === "number" && typeof l.lon === "number")) {
    return { field: "location.city", problem: "give a city, a postal code or lat and lon" };
  }
  return {
    label: label || "Near you",
    precision: label ? (typeof l.city === "string" ? "city" : "postal_code") : l.precision === "exact" ? "exact" : "about_1_km",
  };
}

// ---- /me

route("GET", "/me", (c) =>
  jsonResponse(200, scenario.get().emptyAccount && !c.store.nameSet ? { ...c.store.user, display_name: "" } : c.store.user),
);

route("PATCH", "/me", async (c) => {
  const body = await jsonObject(c);
  const user = c.store.user;
  if (body.display_name !== undefined) {
    if (typeof body.display_name !== "string" || !body.display_name.trim()) {
      return invalid("display_name", "must be a non-empty string");
    }
    user.display_name = body.display_name.trim();
    c.store.nameSet = true;
  }
  if (body.location_consent !== undefined) {
    if (typeof body.location_consent !== "boolean") return invalid("location_consent", "must be a boolean");
    user.location_consent = body.location_consent;
    // Withdrawn consent rounds every garden the user owns to about 10 km (contracts 9, account).
    if (!body.location_consent) {
      for (const g of c.store.gardens.values()) {
        if (myRole(c.store, g.id) !== "owner") continue;
        if (g.location_precision === "exact" || g.location_precision === "about_1_km") g.location_precision = "about_10_km";
      }
    }
  }
  return jsonResponse(200, user);
});

route("POST", "/me/push-tokens", async (c) => {
  const body = await jsonObject(c);
  const token = str(body, "token");
  const platform = str(body, "platform");
  if (!["ios", "android", "web"].includes(platform ?? "")) return invalid("platform", "ios, android or web");
  c.store.pushTokens.add(token!);
  return emptyResponse(204);
});

route("DELETE", "/me/push-tokens/:token", (c) => {
  c.store.pushTokens.delete(decodeURIComponent(c.params.token));
  return emptyResponse(204);
});

// ---- gardens

route("GET", "/gardens", (c) =>
  jsonResponse(
    200,
    paginate(
      scenario.get().emptyAccount
        ? []
        : [...c.store.gardens.values()]
            .filter((g) => myRole(c.store, g.id))
            .sort((a, b) => a.name.localeCompare(b.name))
            .map((g) => gardenView(c.store, g)),
      c.query,
    ),
  ),
);

route("POST", "/gardens", async (c) => {
  const body = await jsonObject(c);
  const name = str(body, "name")!;
  const place = locate(body.location);
  if ("field" in place) return invalid(place.field, place.problem);
  const garden: S["Garden"] = {
    id: c.store.nextId("garden"),
    name,
    location_label: place.label,
    location_precision: place.precision,
    time_zone: "America/Toronto",
    member_count: 1,
    my_role: "owner",
    soil_profile_status: "pending",
    created_at: nowTs(),
  };
  if (scenario.get().emptyAccount) {
    // The account starts empty: the demo data goes, and the name stays unset until PATCH /me.
    c.store.clearAccount();
    scenario.set({ emptyAccount: false });
  }
  c.store.gardens.set(garden.id, garden);
  c.store.maps.set(garden.id, null);
  addMember(c.store, garden.id, c.store.user, "owner");
  return jsonResponse(201, gardenView(c.store, garden));
}, true);

route("GET", "/gardens/:id", (c) => {
  const g = c.store.gardens.get(c.params.id);
  return g ? jsonResponse(200, gardenView(c.store, g)) : notFound();
});

route("PATCH", "/gardens/:id", async (c) => {
  const g = c.store.gardens.get(c.params.id);
  if (!g) return notFound();
  const body = await jsonObject(c);
  if (body.name !== undefined) g.name = str(body, "name")!;
  if (body.location !== undefined) {
    const place = locate(body.location);
    if ("field" in place) return invalid(place.field, place.problem);
    g.location_label = place.label;
    g.location_precision = place.precision;
  }
  return jsonResponse(200, gardenView(c.store, g));
}, "owner");

route("GET", "/gardens/:id/weather", (c) =>
  c.store.gardens.has(c.params.id) ? jsonResponse(200, { weather: c.store.weather }) : notFound(),
);

route("GET", "/gardens/:id/map", (c) =>
  c.store.gardens.has(c.params.id)
    ? jsonResponse(200, { map: c.store.maps.get(c.params.id) ?? null })
    : notFound(),
);

// A mapping session's upload (contracts 9, garden map): a whole new version, refused when another member mapped
// since the session started. Plants named in the session are created with it.
route("PUT", "/gardens/:id/map", async (c) => {
  const gardenId = c.params.id;
  if (!c.store.gardens.has(gardenId)) return notFound();
  const body = (await jsonObject(c)) as unknown as S["GardenMapInput"];
  const current = c.store.maps.get(gardenId) ?? null;
  const version = current?.version ?? 0;
  if (body.base_version !== version) return errorResponse("invalid_state", { current_map_version: version });
  if (!Array.isArray(body.beds) || body.beds.length > 20) return invalid("beds", "at most 20");
  if (!Array.isArray(body.pins) || body.pins.length > 200) return invalid("pins", "at most 200");
  for (const bed of body.beds) {
    if (!Array.isArray(bed.polygon_m) || bed.polygon_m.length < 3 || bed.polygon_m.length > 50) return invalid("beds.polygon_m", "3–50 points");
  }
  // Every plant that isn't this garden's, listed at once, before anything is created (contracts 9).
  const unknown = body.pins.flatMap((pin) => {
    if (!pin.plant_id) return [];
    const plant = c.store.plants.get(pin.plant_id);
    return plant && plant.garden_id === gardenId ? [] : [pin.plant_id];
  });
  if (unknown.length > 0) return errorResponse("invalid_request", { field: "pins", problem: "unknown_plant", plant_ids: unknown });
  const pins: S["Pin"][] = [];
  for (const pin of body.pins) {
    let plantId = pin.plant_id;
    if (!plantId && pin.new_plant) {
      const plant = createPlant(c.store, gardenId, pin.new_plant.name, pin.new_plant.species_id);
      if (typeof plant === "string") return invalid("pins.new_plant.species_id", plant);
      plantId = plant.id;
    }
    const plant = plantId ? c.store.plants.get(plantId) : undefined;
    if (!plant || plant.garden_id !== gardenId) return invalid("pins.plant_id", "not a plant of this garden");
    pins.push({ plant_id: plant.id, x_m: pin.x_m, y_m: pin.y_m, ...(pin.canopy_radius_m !== undefined ? { canopy_radius_m: pin.canopy_radius_m } : {}), ...(pin.bed_id ? { bed_id: pin.bed_id } : {}) });
  }
  const map: S["GardenMap"] = {
    garden_id: gardenId,
    version: version + 1,
    created_at: nowTs(),
    ...(current?.created_by ? { created_by: current.created_by } : {}),
    ...(body.north_deg !== undefined ? { north_deg: body.north_deg } : {}),
    beds: body.beds.map((b) => ({ id: b.id, ...(b.name ? { name: b.name } : {}), polygon_m: b.polygon_m, area_m2: Math.round(polygonArea(b.polygon_m) * 100) / 100 })),
    pins,
  };
  c.store.maps.set(gardenId, map);
  // Every pin moves to the new version; a plant left without one loses its position (architecture 6.8).
  for (const plant of c.store.plants.values()) {
    if (plant.garden_id !== gardenId) continue;
    const pin = pins.find((p) => p.plant_id === plant.id);
    if (pin) plant.position = { map_version: map.version, x_m: pin.x_m, y_m: pin.y_m };
    else delete plant.position;
  }
  return jsonResponse(200, map);
}, true);

// ---- plants

route("GET", "/plants", (c) => {
  const gardenId = c.query.get("garden_id");
  if (!gardenId) return invalid("garden_id", "required");
  if (!c.store.gardens.has(gardenId)) return notFound();
  const items = [...c.store.plants.values()]
    .filter((p) => p.garden_id === gardenId)
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((p) => c.store.plantView(p));
  return jsonResponse(200, paginate(items, c.query));
});

route("POST", "/plants", async (c) => {
  const body = await jsonObject(c);
  const gardenId = str(body, "garden_id")!;
  const name = str(body, "name")!;
  if (!c.store.gardens.has(gardenId)) return notFound();
  const speciesId = str(body, "species_id", false);
  const plant = createPlant(c.store, gardenId, name, speciesId);
  if (typeof plant === "string") return invalid("species_id", plant);
  return jsonResponse(201, c.store.plantView(plant));
}, true);

function createPlant(store: MockStore, gardenId: string, name: string, speciesId?: string) {
  const species = speciesId ? store.species.find((s) => s.id === speciesId) : undefined;
  if (speciesId && !species) return "unknown species";
  const plant: S["Plant"] = {
    id: store.nextId("plant"),
    garden_id: gardenId,
    name,
    status: "healthy",
    species_status: species ? "confirmed" : "unknown",
    species_candidates: [],
    created_at: nowTs(),
    ...(species
      ? { species: { id: species.id, common_name: species.common_name, scientific_name: species.scientific_name } }
      : {}),
  };
  store.plants.set(plant.id, plant);
  return plant;
}

route("GET", "/plants/:id", (c) => {
  const p = c.store.plants.get(c.params.id);
  return p ? jsonResponse(200, c.store.plantView(p)) : notFound();
});

route("PATCH", "/plants/:id", async (c) => {
  const p = c.store.plants.get(c.params.id);
  if (!p) return notFound();
  const body = await jsonObject(c);
  if (body.name !== undefined) p.name = str(body, "name")!;
  if (body.status !== undefined) {
    if (!["healthy", "watch", "sick", "dead"].includes(String(body.status))) return invalid("status", "unknown status");
    p.status = body.status as S["PlantStatus"];
  }
  return jsonResponse(200, c.store.plantView(p));
}, true);

// Flows 7: the plant's species, and its latest scan following it (domain rules/scan_state.rs, `species_follow`):
// only a scan created in the last 24 hours that isn't failed follows. A pending or queued one runs with the new
// species, no rerun; one past `queued`, mid-run too, reruns when the species differs from its run's working one.
route("POST", "/plants/:id/species", async (c) => {
  const p = c.store.plants.get(c.params.id);
  if (!p) return notFound();
  const body = await jsonObject(c);
  const speciesId = str(body, "species_id", false);
  const custom = str(body, "custom_name", false)?.trim();
  if (!speciesId === !custom) return invalid("species_id", "species_id or custom_name, not both");
  if (custom && custom.length > 80) return invalid("custom_name", "at most 80 characters");
  const known = speciesId ? c.store.species.find((s) => s.id === speciesId) : undefined;
  if (speciesId && !known) return invalid("species_id", "unknown species");
  const before = p.species?.id ?? null;
  if (known) {
    p.species = { id: known.id, common_name: known.common_name, scientific_name: known.scientific_name };
    delete p.custom_species_name;
  } else {
    delete p.species;
    p.custom_species_name = custom;
  }
  p.species_status = "confirmed";
  p.species_candidates = [];
  const latest = c.store.scansOf(p.id).at(-1);
  const recent = latest && Date.now() - Date.parse(latest.scan.created_at) < 24 * 3_600_000 && latest.scan.status !== "failed";
  if (latest && recent) {
    const early = latest.scan.status === "pending" || latest.scan.status === "queued";
    const working = latest.workingSpeciesId === undefined ? before : latest.workingSpeciesId;
    const rerun = !early && (known?.id ?? null) !== working;
    if (rerun) speciesRerun(c.store, latest.scan.id, known?.id ?? null, custom);
    else if (latest.scan.run > 0) {
      c.store.emit(latest.scan.id, { type: "species_confirmed", species_id: known?.id ?? null, ...(custom ? { custom_name: custom } : {}), rerun: false });
    }
  }
  return jsonResponse(200, c.store.plantView(p));
}, true);

// The timeline and measurements, with the proposed Insights fields (insights.ts).
registerInsightRoutes(route);

// ---- tasks and memories

function byDue(a: S["Task"], b: S["Task"]): number {
  if (a.due && b.due && a.due !== b.due) return a.due < b.due ? -1 : 1;
  if (a.due && !b.due) return -1;
  if (!a.due && b.due) return 1;
  return a.created_at.localeCompare(b.created_at);
}

route("GET", "/tasks", (c) => {
  const gardenId = c.query.get("garden_id");
  if (!gardenId) return invalid("garden_id", "required");
  if (!c.store.gardens.has(gardenId)) return notFound();
  const source = c.query.get("source");
  const state = c.query.get("state");
  if (state && !["open", "done", "dismissed"].includes(state)) return invalid("state", "open, done or dismissed");
  const items = c.store.tasks
    .filter((t) => t.garden_id === gardenId)
    .filter((t) => !source || t.source === source)
    .filter((t) => {
      if (state === "open") return !t.done_at && !t.dismissed_at;
      if (state === "done") return Boolean(t.done_at);
      if (state === "dismissed") return Boolean(t.dismissed_at);
      return true;
    })
    .sort(byDue);
  return jsonResponse(200, paginate(items, c.query));
});

route("POST", "/tasks", async (c) => {
  const body = await jsonObject(c);
  const gardenId = str(body, "garden_id")!;
  if (!c.store.gardens.has(gardenId)) return notFound();
  const task: S["Task"] = {
    id: c.store.nextId("task"),
    garden_id: gardenId,
    source: "user",
    title: str(body, "title")!,
    created_at: nowTs(),
    ...(typeof body.detail === "string" ? { detail: body.detail } : {}),
    ...(typeof body.due === "string" ? { due: body.due } : {}),
    ...(typeof body.plant_id === "string" ? { plant_id: body.plant_id } : {}),
    ...(typeof body.scan_id === "string" ? { scan_id: body.scan_id } : {}),
    ...(body.repeat && typeof body.repeat === "object" ? { repeat: body.repeat as S["Repeat"] } : {}),
  };
  c.store.tasks.push(task);
  return jsonResponse(201, task);
}, true);

route("PATCH", "/tasks/:id", async (c) => {
  const task = c.store.tasks.find((t) => t.id === c.params.id);
  if (!task) return notFound();
  const body = await jsonObject(c);
  const now = nowTs();
  if (body.title !== undefined) task.title = str(body, "title")!;
  if (typeof body.detail === "string") task.detail = body.detail;
  if (typeof body.due === "string") task.due = body.due;
  if (body.repeat && typeof body.repeat === "object") task.repeat = body.repeat as S["Repeat"];
  if (body.done === true && !task.done_at) {
    task.done_at = now;
    // A repeating task opens its next occurrence.
    if (task.repeat && !task.dismissed_at) {
      const d = c.store.dates;
      const next = d.date(-task.repeat.every_days);
      if (!task.repeat.until || next <= task.repeat.until) {
        const { done_at: _done, ...open } = task;
        c.store.tasks.push({ ...open, id: c.store.nextId("task"), due: next, created_at: now });
      }
    }
  } else if (body.done === false) {
    delete task.done_at;
  }
  if (body.dismissed === true && !task.dismissed_at) {
    task.dismissed_at = now;
    task.dismissed_reason = "user";
  } else if (body.dismissed === false) {
    delete task.dismissed_at;
    delete task.dismissed_reason;
  }
  return jsonResponse(200, task);
}, true);

route("DELETE", "/tasks/:id", (c) => {
  const i = c.store.tasks.findIndex((t) => t.id === c.params.id);
  if (i < 0) return notFound();
  c.store.tasks.splice(i, 1);
  return emptyResponse(204);
}, true);

route("GET", "/memories", (c) => {
  const gardenId = c.query.get("garden_id");
  if (!gardenId) return invalid("garden_id", "required");
  if (!c.store.gardens.has(gardenId)) return notFound();
  const plantId = c.query.get("plant_id");
  const items = c.store.memories
    .filter((m) => m.garden_id === gardenId && (!plantId || m.plant_id === plantId))
    .sort((a, b) => b.created_at.localeCompare(a.created_at));
  return jsonResponse(200, paginate(items, c.query));
});

route("POST", "/memories", async (c) => {
  const body = await jsonObject(c);
  const gardenId = str(body, "garden_id")!;
  if (!c.store.gardens.has(gardenId)) return notFound();
  const memory: S["Memory"] = {
    id: c.store.nextId("mem"),
    garden_id: gardenId,
    kind: "fact",
    source: "user",
    shared: true,
    topic: (typeof body.topic === "string" ? body.topic : "other") as S["MemoryTopic"],
    text: str(body, "text")!,
    created_at: nowTs(),
    ...(typeof body.plant_id === "string" ? { plant_id: body.plant_id } : {}),
  };
  c.store.memories.push(memory);
  return jsonResponse(201, memory);
}, true);

// ---- species

route("GET", "/species", (c) => {
  const q = (c.query.get("q") ?? "").trim().toLowerCase();
  const items = c.store.species.filter(
    (s) => !q || s.common_name.toLowerCase().includes(q) || s.scientific_name.toLowerCase().includes(q),
  );
  return jsonResponse(200, paginate(items, c.query));
});

route("GET", "/species/:id", (c) => {
  const s = c.store.species.find((x) => x.id === c.params.id);
  return s ? jsonResponse(200, s) : notFound();
});

// ---- scans

route("POST", "/scans", async (c) => {
  const body = await jsonObject(c);
  const gardenId = str(body, "garden_id")!;
  if (!c.store.gardens.has(gardenId)) return notFound();
  const plantId = str(body, "plant_id", false);
  const np = body.new_plant;
  if ((plantId ? 1 : 0) + (np ? 1 : 0) !== 1) return invalid("plant_id", "give plant_id or new_plant, not both");
  let plant;
  if (plantId) {
    plant = c.store.plants.get(plantId);
    if (!plant || plant.garden_id !== gardenId) return notFound();
  } else {
    const n = np as Record<string, unknown>;
    if (typeof n !== "object" || typeof n.name !== "string" || !n.name) return invalid("new_plant.name", "required");
    const made = createPlant(c.store, gardenId, n.name, typeof n.species_id === "string" ? n.species_id : undefined);
    if (typeof made === "string") return invalid("new_plant.species_id", made);
    plant = made;
  }
  const now = nowTs();
  const scan: S["Scan"] = {
    id: c.store.nextId("scan"),
    garden_id: gardenId,
    plant_id: plant.id,
    created_by: { user_id: c.store.user.id, display_name: c.store.user.display_name },
    status: "pending",
    run: 0,
    last_event_seq: 0,
    created_at: now,
    images: [],
    guided_checks: [],
  };
  c.store.scans.set(scan.id, { scan, observations: [], events: [], running: false });
  return jsonResponse(201, scan);
}, true);

route("GET", "/scans/:id", (c) => {
  const st = c.store.scan(c.params.id);
  return st ? jsonResponse(200, c.store.scanDetail(st)) : notFound();
});

route("DELETE", "/scans/:id", (c) => {
  const st = c.store.scan(c.params.id);
  if (!st) return notFound();
  if (st.scan.status !== "pending") {
    return errorResponse("invalid_state", { status: st.scan.status, allowed: ["pending"] });
  }
  c.store.scans.delete(c.params.id);
  c.store.tasks = c.store.tasks.filter((t) => t.scan_id !== c.params.id);
  c.store.scripts.delete(c.params.id);
  c.store.notify(c.params.id);
  return emptyResponse(204);
}, true);

route("POST", "/scans/:id/images", async (c) => {
  const st = c.store.scan(c.params.id);
  if (!st) return notFound();
  const sc = scenario.get();
  const fault = sc.uploadFault;
  if (fault) {
    scenario.set({ uploadFault: null });
    if (fault === "unsupported_media_type") return errorResponse("unsupported_media_type", { accepted: ACCEPTED });
    const dark = fault === "image_too_dark";
    // The server's message names the problem and its fix (contracts 2, QualityHint; screens 3.2's example).
    return errorResponse(
      "image_quality",
      {
        problem: dark ? "too_dark" : "too_blurry",
        blur_score: dark ? 210.4 : 38.2,
        exposure: dark ? 0.08 : 0.5,
        hint: dark ? "more_light" : "tap_to_focus",
      },
      dark
        ? "This photo is too dark. Move into better light or turn the plant to face it."
        : "This photo is too blurry. Hold the phone still and tap the leaf to focus.",
    );
  }
  const form = await c.form();
  const kind = (form.kind ?? "whole_plant") as S["ImageKind"];
  if (!IMAGE_KINDS.includes(kind)) return invalid("kind", "unknown image kind");
  if (form.fileType && !ACCEPTED.includes(form.fileType.toLowerCase())) {
    return errorResponse("unsupported_media_type", { accepted: ACCEPTED });
  }
  const answering = form.questionId;
  if (answering) {
    const q = st.scan.open_question;
    if (!q || q.id !== answering || q.status !== "open") {
      return errorResponse("invalid_state", { status: st.scan.status, allowed: ["open"] });
    }
  } else if (kind === "squeeze_test" || kind === "jar_test") {
    if (st.scan.status === "failed") {
      return errorResponse("invalid_state", { status: st.scan.status, allowed: ["pending", "queued", "analysing", "awaiting_answer", "diagnosed", "planned"] });
    }
  } else if (st.scan.status !== "pending") {
    return errorResponse("invalid_state", { status: st.scan.status, allowed: ["pending"] });
  }
  // Limits apply to the capture before submit; guided checks after submit have none.
  if (!answering && st.scan.status === "pending" && st.scan.images.filter((i) => i.kind === kind && !i.answers_question_id).length >= KIND_LIMIT[kind]) {
    return errorResponse("too_many_images", { kind, max: KIND_LIMIT[kind] });
  }
  const photo = photoFor(c.store, st.scan.plant_id, kind);
  const image: S["ScanImage"] = {
    id: c.store.nextId("img"),
    kind,
    uploaded_at: nowTs(),
    url: mockImageUrl(photo),
    ...MOCK_IMAGE_SIZE[photo],
    ...(answering ? { answers_question_id: answering } : {}),
    ...(form.hasDepth ? { depth: { has_gravity: true, width: 256, height: 192 } } : {}),
  };
  st.scan.images.push(image);
  // As the API does (contracts 3): a photo for a question makes it answering, and a guided photo is a new check
  // that its own job analyses.
  if (answering && st.scan.open_question) st.scan.open_question.status = "answering";
  if (!answering && (kind === "squeeze_test" || kind === "jar_test")) {
    st.scan.guided_checks.push({ image_id: image.id, kind, status: "analysing" });
    // Its own job analyses it, whatever the scan's state (contracts 7, guided checks and the diagnosis).
    void guidedCheck(c.store, st.scan.id, image.id, kind);
  }
  const body: S["UploadImageResponse"] = {
    image,
    quality: { passed: true, blur_score: 182.4, exposure: 0.47 },
    ...(form.hasDepth ? { depth: { stored: true } } : {}),
  };
  return jsonResponse(201, body);
}, true);

route("DELETE", "/scans/:id/images/:image_id", (c) => {
  const st = c.store.scan(c.params.id);
  if (!st) return notFound();
  if (st.scan.status !== "pending") {
    return errorResponse("invalid_state", { status: st.scan.status, allowed: ["pending"] });
  }
  const i = st.scan.images.findIndex((img) => img.id === c.params.image_id);
  if (i < 0) return notFound();
  st.scan.images.splice(i, 1);
  return emptyResponse(204);
}, true);

route("POST", "/scans/:id/submit", (c) => {
  const st = c.store.scan(c.params.id);
  if (!st) return notFound();
  const failure = st.scan.status === "failed" ? st.scan.failure : undefined;
  if (st.scan.status === "failed" && !failure?.retryable) {
    return errorResponse("invalid_state", { status: st.scan.status, allowed: ["pending"], hint: "new_scan" });
  }
  if (st.scan.status !== "pending" && !failure) return jsonResponse(200, st.scan);
  if (!st.scan.images.some((i) => i.kind === "whole_plant")) {
    return errorResponse("missing_required_shot", { missing: ["whole_plant"] });
  }
  startRun(c.store, st.scan.id, scriptFor(takeOutcome(st.scan.plant_id)));
  return jsonResponse(202, st.scan);
}, true);

route("POST", "/scans/:id/answers", async (c) => {
  const st = c.store.scan(c.params.id);
  if (!st) return notFound();
  const body = await jsonObject(c);
  const questionId = str(body, "question_id")!;
  const option = str(body, "option_id")!;
  if (!["yes", "no", "unsure"].includes(option)) return invalid("option_id", "yes, no or unsure");
  const q = st.scan.open_question;
  if (!q || q.id !== questionId) return errorResponse("invalid_state", { status: st.scan.status, allowed: ["open"] });
  if (q.status !== "open") return errorResponse("invalid_state", { status: st.scan.status, allowed: ["open"] });
  if (!answerRun(c.store, st.scan.id, option as S["Answer"])) {
    return errorResponse("invalid_state", { status: st.scan.status });
  }
  return jsonResponse(202, st.scan);
}, true);

route("POST", "/scans/:id/feedback", async (c) => {
  const st = c.store.scan(c.params.id);
  if (!st) return notFound();
  const body = await jsonObject(c);
  if (!["correct", "wrong", "unsure"].includes(String(body.verdict))) return invalid("verdict", "correct, wrong or unsure");
  if (st.scan.status === "pending") return errorResponse("invalid_state", { status: st.scan.status });
  return jsonResponse(201, { id: c.store.nextId("fb") });
}, "reviewer");

// ---- the event stream

const encoder = new TextEncoder();

function frame(e: S["ScanEvent"]): string {
  return `id: ${e.seq}\nevent: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`;
}

/** `ReadableStream<Uint8Array>` of SSE frames: replays after `lastId`, then follows the run. */
export function eventStream(store: MockStore, scanId: string, lastId: number): ReadableStream<Uint8Array> {
  const gen = store.generation;
  const drop = scenario.get().dropNextStream;
  if (drop) scenario.set({ dropNextStream: false });
  let cancelled = false;

  async function pump(controller: ReadableStreamDefaultController<Uint8Array>): Promise<void> {
    let sent = lastId;
    let count = 0;
    try {
      for (;;) {
        const st = store.scan(scanId);
        if (!st || cancelled || store.generation !== gen) break;
        for (const e of st.events) {
          if (e.seq <= sent) continue;
          controller.enqueue(encoder.encode(frame(e)));
          sent = e.seq;
          count += 1;
          if (drop && count >= 3) {
            await store.sleep(5); // let the reader take the frames already queued
            controller.error(new TypeError("Network request failed"));
            return;
          }
        }
        if (store.atRest(scanId)) break;
        const r = await store.waitFor(scanId, scenario.get().keepaliveMs);
        if (r === "timeout" && !cancelled) controller.enqueue(encoder.encode(": keepalive\n\n"));
      }
      if (!cancelled) controller.close();
    } catch {
      // The reader went away.
    }
  }

  return new ReadableStream<Uint8Array>({
    start(controller) {
      void pump(controller);
    },
    cancel() {
      cancelled = true;
    },
  });
}

/**
 * A Response whose `body` is the stream itself. The stream never goes through the runtime's Response: under Jest,
 * Node's fetch doesn't take this realm's ReadableStream as one and wraps it as an async iterable in a byte stream,
 * which throws "ReadableStream is already closed" from a microtask, into whatever test runs next, when the reader
 * cancels it mid-read.
 */
function streamResponse(stream: ReadableStream<Uint8Array>): Response {
  const res = new Response(null, { status: 200, headers: { "content-type": "text/event-stream", "cache-control": "no-cache" } });
  Object.defineProperty(res, "body", { value: stream, configurable: true });
  return res;
}

route("GET", "/scans/:id/events", (c) => {
  if (!c.store.scan(c.params.id)) return notFound();
  const raw = c.headers.get("last-event-id");
  const last = raw && /^\d+$/.test(raw) ? Number(raw) : 0;
  return streamResponse(eventStream(c.store, c.params.id, last));
});

// ---- chat (chat.ts)

registerChatRoutes(route, { jsonObject, streamResponse });
registerConnectorRoutes(route, { jsonObject });

// ---- members, invites and deletion (members.ts)

registerMemberRoutes(route, { jsonObject });

// ---- the transport

function faultResponse(fault: NonNullable<ReturnType<typeof scenario.get>["callFault"]>): Response {
  switch (fault) {
    case "budget_exceeded":
      return errorResponse("budget_exceeded", {
        budget: "scans",
        resets_at: nowTs(8 * 3_600_000),
      });
    case "invalid_state":
      return errorResponse("invalid_state", { status: "planned", allowed: ["pending"], hint: "new_scan" });
    case "rate_limited":
      return errorResponse("rate_limited");
  }
}

export const mockTransport: Transport = {
  async fetch(input, init) {
    const rawUrl = typeof input === "string" ? input : input.url;
    const url = new URL(rawUrl, MOCK_ORIGIN);
    const method = (init?.method ?? (typeof input === "string" ? "GET" : input.method)).toUpperCase();
    const headers = new Headers(init?.headers ?? (typeof input === "string" ? undefined : input.headers));
    const path = url.pathname.length > 1 ? url.pathname.replace(/\/+$/, "") : url.pathname;

    let matched: { r: Route; params: Record<string, string> } | undefined;
    for (const r of routes) {
      if (r.method !== method) continue;
      const m = r.re.exec(path);
      if (m) {
        matched = { r, params: Object.fromEntries(r.keys.map((k, i) => [k, decodeURIComponent(m[i + 1])])) };
        break;
      }
    }
    if (!matched) return notFound();

    const isStream = matched.r.re.source.endsWith("events\\/?$");
    const sc = scenario.get();
    if (sc.callFault && !isStream) {
      const fault = sc.callFault;
      scenario.set({ callFault: null });
      return faultResponse(fault);
    }
    const need = matched.r.minRole;
    if (need && RANK[sc.role] < RANK[need] && !path.startsWith("/me")) {
      return errorResponse("forbidden", { required_role: need });
    }

    let cached: Promise<unknown> | undefined;
    const ctx: Ctx = {
      method,
      params: matched.params,
      query: url.searchParams,
      headers,
      store: mockStore,
      json: () => {
        cached ??= readText(input, init).then((t) => {
          if (t === undefined || t === "") throw new BadRequest("body", "expected a JSON body");
          try {
            return JSON.parse(t) as unknown;
          } catch {
            throw new BadRequest("body", "malformed JSON");
          }
        });
        return cached;
      },
      form: () => readForm(input, init),
    };
    try {
      return await matched.r.handler(ctx);
    } catch (e) {
      if (e instanceof BadRequest) return invalid(e.field, e.problem);
      throw e;
    }
  },
};
