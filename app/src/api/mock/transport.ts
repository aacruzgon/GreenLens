// The mock Transport (docs/frontend/README.md 15): a router on method and path that answers the
// REST endpoints from the fixture store with the real status codes and ErrorBody errors, and
// streams scan events as a Server-Sent Events byte stream.

import type { Transport } from "../transport";
import { answerRun, scriptFor, startRun, takeOutcome } from "./runs";
import { scenario } from "./scenario";
import { mockStore, type MockStore } from "./store";
import { MOCK_IMAGE_SIZE, mockImageUrl, type MockImageName } from "./images";
import { emptyResponse, errorResponse, jsonResponse, MOCK_ORIGIN, paginate, type S } from "./util";

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
  /** Needs the editor role. */
  write: boolean;
  handler: Handler;
}

const routes: Route[] = [];

function route(method: string, pattern: string, handler: Handler, write = false): void {
  const keys: string[] = [];
  const re = new RegExp(
    "^" + pattern.replace(/:([a-z_]+)/g, (_m, k: string) => (keys.push(k), "([^/]+)")) + "/?$",
  );
  routes.push({ method, re, keys, write, handler });
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
  else if (init?.body == null && typeof input !== "string") {
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

function isGrownUp(role: S["Role"]): boolean {
  return role === "editor" || role === "owner";
}

function gardenView(store: MockStore, g: S["Garden"]): S["Garden"] {
  return { ...g, my_role: scenario.get().role };
}

// ---- /me

route("GET", "/me", (c) => jsonResponse(200, c.store.user));

route("PATCH", "/me", async (c) => {
  const body = await jsonObject(c);
  const user = c.store.user;
  if (body.display_name !== undefined) {
    if (typeof body.display_name !== "string" || !body.display_name.trim()) {
      return invalid("display_name", "must be a non-empty string");
    }
    user.display_name = body.display_name.trim();
  }
  if (body.location_consent !== undefined) {
    if (typeof body.location_consent !== "boolean") return invalid("location_consent", "must be a boolean");
    user.location_consent = body.location_consent;
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
  jsonResponse(200, paginate([...c.store.gardens.values()].map((g) => gardenView(c.store, g)), c.query)),
);

route("POST", "/gardens", async (c) => {
  const body = await jsonObject(c);
  const name = str(body, "name")!;
  const loc = body.location;
  if (typeof loc !== "object" || loc === null) return invalid("location", "required");
  const l = loc as Record<string, unknown>;
  const label =
    (typeof l.city === "string" && l.city) || (typeof l.postal_code === "string" && l.postal_code) || "";
  if (!label && !(typeof l.lat === "number" && typeof l.lon === "number")) {
    return invalid("location.city", "give a city, a postal code or lat and lon");
  }
  const garden: S["Garden"] = {
    id: c.store.nextId("garden"),
    name,
    location_label: label || "Near you",
    location_precision: label ? (typeof l.city === "string" ? "city" : "postal_code") : l.precision === "exact" ? "exact" : "about_1_km",
    time_zone: "America/Toronto",
    member_count: 1,
    my_role: "owner",
    soil_profile_status: "pending",
    created_at: new Date().toISOString(),
  };
  c.store.gardens.set(garden.id, garden);
  c.store.maps.set(garden.id, null);
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
  return jsonResponse(200, gardenView(c.store, g));
}, true);

route("GET", "/gardens/:id/weather", (c) =>
  c.store.gardens.has(c.params.id) ? jsonResponse(200, { weather: c.store.weather }) : notFound(),
);

route("GET", "/gardens/:id/map", (c) =>
  c.store.gardens.has(c.params.id)
    ? jsonResponse(200, { map: c.store.maps.get(c.params.id) ?? null })
    : notFound(),
);

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
    created_at: new Date().toISOString(),
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

route("GET", "/plants/:id/timeline", (c) =>
  c.store.plants.has(c.params.id)
    ? jsonResponse(200, paginate(c.store.timelineOf(c.params.id), c.query))
    : notFound(),
);

route("GET", "/plants/:id/measurements", (c) => {
  if (!c.store.plants.has(c.params.id)) return notFound();
  const q = c.query.get("quantity");
  if (q && q !== "plant_height_cm" && q !== "canopy_width_cm") return invalid("quantity", "unknown quantity");
  return jsonResponse(200, paginate(c.store.pointsOf(c.params.id, (q as S["GrowthQuantity"] | null) ?? undefined), c.query));
});

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
    created_at: new Date().toISOString(),
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
  const now = new Date().toISOString();
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
    created_at: new Date().toISOString(),
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
  const now = new Date().toISOString();
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
  if (!c.store.scans.delete(c.params.id)) return notFound();
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
    return errorResponse("image_quality", {
      problem: dark ? "too_dark" : "too_blurry",
      blur_score: dark ? 210.4 : 38.2,
      exposure: dark ? 0.08 : 0.5,
      hint: dark ? "more_light" : "tap_to_focus",
    });
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
  } else if (kind !== "squeeze_test" && kind !== "jar_test" && st.scan.status !== "pending") {
    return errorResponse("invalid_state", { status: st.scan.status, allowed: ["pending"] });
  }
  if (!answering && st.scan.images.filter((i) => i.kind === kind && !i.answers_question_id).length >= KIND_LIMIT[kind]) {
    return errorResponse("too_many_images", { kind, max: KIND_LIMIT[kind] });
  }
  const photo = photoFor(c.store, st.scan.plant_id, kind);
  const image: S["ScanImage"] = {
    id: c.store.nextId("img"),
    kind,
    uploaded_at: new Date().toISOString(),
    url: mockImageUrl(photo),
    ...MOCK_IMAGE_SIZE[photo],
    ...(answering ? { answers_question_id: answering } : {}),
    ...(form.hasDepth ? { depth: { has_gravity: true, width: 256, height: 192 } } : {}),
  };
  st.scan.images.push(image);
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
  if (st.scan.status !== "pending") {
    return errorResponse("invalid_state", { status: st.scan.status, allowed: ["pending"], hint: "new_scan" });
  }
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
  if (!c.store.scan(c.params.id)) return notFound();
  const body = await jsonObject(c);
  if (!["correct", "wrong", "unsure"].includes(String(body.verdict))) return invalid("verdict", "correct, wrong or unsure");
  return emptyResponse(204);
});

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

/** A Response whose `body` is the stream, also where the runtime's Response can't take one. */
function streamResponse(stream: ReadableStream<Uint8Array>): Response {
  const init = { status: 200, headers: { "content-type": "text/event-stream", "cache-control": "no-cache" } };
  let res: Response;
  try {
    res = new Response(stream as unknown as BodyInit, init);
  } catch {
    res = new Response(null, init);
  }
  if ((res.body as unknown) !== stream && typeof (res.body as { getReader?: unknown } | null)?.getReader !== "function") {
    Object.defineProperty(res, "body", { value: stream, configurable: true });
  }
  return res;
}

route("GET", "/scans/:id/events", (c) => {
  if (!c.store.scan(c.params.id)) return notFound();
  const raw = c.headers.get("last-event-id");
  const last = raw && /^\d+$/.test(raw) ? Number(raw) : 0;
  return streamResponse(eventStream(c.store, c.params.id, last));
});

// ---- the transport

function faultResponse(fault: NonNullable<ReturnType<typeof scenario.get>["callFault"]>): Response {
  switch (fault) {
    case "budget_exceeded":
      return errorResponse("budget_exceeded", {
        budget: "scans",
        resets_at: new Date(Date.now() + 8 * 3_600_000).toISOString(),
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
    if (matched.r.write && !isGrownUp(sc.role) && !path.startsWith("/me")) {
      return errorResponse("forbidden", { required_role: "editor" });
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
