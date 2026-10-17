import type { CreateGardenRequest, CreatePlantRequest, CreateScanRequest, ImageKind, ScanDetail, ScanEvent, UpdateMeRequest, UpdateTaskRequest } from "@/domain/types";
import { KIND_LIMIT } from "@/domain/scan";

import type { Transport, UploadPart } from "../transport";
import { answerQuestion, startRun } from "./runs";
import { db } from "./store";
import { ts, ulid } from "./util";

// docs/frontend 15: answers the REST endpoints from the fixture store and streams scripted scan runs,
// so the whole app runs with no backend and no network.

const LATENCY_MS = 250;

function json(status: number, body: unknown): Response {
  return new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function error(status: number, code: string, message: string, details?: Record<string, unknown>): Response {
  return json(status, { error: { code, message, retryable: status >= 500 || status === 429, details, request_id: `req_mock_${ulid().slice(-8)}` } });
}

const notFound = () => error(404, "not_found", "We can't find this.");

function scanOf(detail: ScanDetail) {
  // A Scan is a ScanDetail without the joined records.
  const { plant: _p, observations: _o, weather: _w, diagnosis: _d, plan: _pl, ...scan } = detail;
  return scan;
}

function plantOf(id: string) {
  return db.plants.find((p) => p.id === id);
}

async function bodyOf<T>(req: Request): Promise<T> {
  const text = await req.text();
  return (text ? JSON.parse(text) : {}) as T;
}

async function route(req: Request): Promise<Response> {
  const url = new URL(req.url);
  const parts = url.pathname.split("/").filter(Boolean);
  const method = req.method.toUpperCase();
  const [root, id, sub, subId] = parts;

  if (root === "me") {
    if (method === "GET") return json(200, db.me);
    if (method === "PATCH") {
      Object.assign(db.me, await bodyOf<UpdateMeRequest>(req));
      return json(200, db.me);
    }
  }

  if (root === "gardens") {
    if (!id && method === "GET") return json(200, { items: db.gardens });
    if (!id && method === "POST") {
      const body = await bodyOf<CreateGardenRequest>(req);
      const g = {
        id: ulid(),
        name: body.name,
        my_role: "owner" as const,
        location_label: body.location.city ?? body.location.postal_code ?? "Your location",
        location_precision: body.location.lat !== undefined ? ("about_1_km" as const) : body.location.city ? ("city" as const) : ("postal_code" as const),
        time_zone: "America/Toronto",
        soil_profile_status: "pending" as const,
        member_count: 1,
        created_at: ts(),
      };
      db.gardens.push(g);
      return json(201, g);
    }
    const garden = db.gardens.find((g) => g.id === id);
    if (!garden) return notFound();
    if (!sub && method === "GET") return json(200, garden);
    if (sub === "weather") return json(200, { weather: db.weather });
  }

  if (root === "plants") {
    if (!id && method === "GET") {
      const gardenId = url.searchParams.get("garden_id");
      return json(200, { items: db.plants.filter((p) => p.garden_id === gardenId) });
    }
    if (!id && method === "POST") {
      const body = await bodyOf<CreatePlantRequest>(req);
      const p = { id: ulid(), garden_id: body.garden_id, name: body.name, species_status: "unknown" as const, species_candidates: [], status: "healthy" as const, created_at: ts() };
      db.plants.push(p);
      return json(201, p);
    }
    const plant = plantOf(id!);
    if (!plant) return notFound();
    if (!sub && method === "GET") return json(200, plant);
    if (sub === "timeline") {
      const items = [...db.scans.values()]
        .map((s) => s.detail)
        .filter((d) => d.plant_id === plant.id && d.status !== "pending")
        .sort((a, b) => b.created_at.localeCompare(a.created_at))
        .map((d) => ({
          scan_id: d.id,
          created_at: d.created_at,
          status: d.status,
          diagnosis_status: d.diagnosis?.status,
          primary: d.diagnosis?.primary ? { problem_id: d.diagnosis.primary.problem_id, name: d.diagnosis.primary.name } : undefined,
          thumbnail_image_id: d.images.find((i) => i.kind === "whole_plant")?.id,
          thumbnail_url: d.images.find((i) => i.kind === "whole_plant")?.url,
          plan_id: d.plan_id,
        }));
      return json(200, { items });
    }
  }

  if (root === "tasks") {
    if (!id && method === "GET") {
      const gardenId = url.searchParams.get("garden_id");
      const source = url.searchParams.get("source");
      const state = url.searchParams.get("state");
      const items = db.tasks.filter(
        (t) =>
          t.garden_id === gardenId &&
          (!source || t.source === source) &&
          (!state || (state === "open" ? !t.done_at && !t.dismissed_at : state === "done" ? !!t.done_at : !!t.dismissed_at)),
      );
      return json(200, { items });
    }
    const task = db.tasks.find((t) => t.id === id);
    if (!task) return notFound();
    if (method === "PATCH") {
      const body = await bodyOf<UpdateTaskRequest>(req);
      if (body.done !== undefined) task.done_at = body.done ? ts() : undefined;
      if (body.dismissed !== undefined) task.dismissed_at = body.dismissed ? ts() : undefined;
      return json(200, task);
    }
  }

  if (root === "scans") {
    if (!id && method === "POST") {
      const body = await bodyOf<CreateScanRequest>(req);
      let plant = body.plant_id ? plantOf(body.plant_id) : undefined;
      if (!plant && body.new_plant) {
        plant = { id: ulid(), garden_id: body.garden_id, name: body.new_plant.name, species_status: "unknown", species_candidates: [], status: "healthy", created_at: ts() };
        db.plants.push(plant);
      }
      if (!plant) return error(400, "invalid_request", "Choose a plant.", { field: "plant_id", problem: "missing" });
      const detail: ScanDetail = {
        id: ulid(),
        garden_id: body.garden_id,
        plant_id: plant.id,
        status: "pending",
        run: 0,
        images: [],
        guided_checks: [],
        created_at: ts(),
        created_by: { user_id: db.me.id, display_name: db.me.display_name },
        last_event_seq: 0,
        plant,
        observations: [],
      };
      db.addScan(detail);
      plant.latest_scan = { scan_id: detail.id, created_at: detail.created_at, status: "pending" };
      return json(201, scanOf(detail));
    }
    const entry = db.scan(id!);
    if (!entry) return notFound();
    const d = entry.detail;
    if (!sub && method === "GET") return json(200, { ...d, plant: plantOf(d.plant_id) ?? d.plant });
    if (!sub && method === "DELETE") {
      if (d.status !== "pending") return error(409, "invalid_state", "Only a scan that hasn't been sent can be discarded.", { status: d.status });
      db.scans.delete(d.id);
      const p = plantOf(d.plant_id);
      if (p?.latest_scan?.scan_id === d.id) delete p.latest_scan;
      return new Response(null, { status: 204 });
    }
    if (sub === "images" && method === "DELETE") {
      d.images = d.images.filter((i) => i.id !== subId);
      return new Response(null, { status: 204 });
    }
    if (sub === "submit") {
      if (!d.images.some((i) => i.kind === "whole_plant")) {
        return error(422, "missing_required_shot", "Take a photo of the whole plant first.", { missing: ["whole_plant"] });
      }
      if (d.status !== "pending") return json(200, scanOf(d));
      startRun(d);
      return json(202, scanOf(d));
    }
    if (sub === "answers") {
      const body = await bodyOf<{ question_id: string; option_id: "yes" | "no" | "unsure" }>(req);
      if (d.open_question?.id !== body.question_id) return error(409, "invalid_state", "This question has already been answered.");
      answerQuestion(d, body.question_id, body.option_id);
      return json(202, scanOf(d));
    }
    if (sub === "events") return streamEvents(d.id, Number(req.headers.get("Last-Event-ID") ?? 0));
  }

  return notFound();
}

/** A fake streaming Response: replays after Last-Event-ID, follows live events, closes at rest. */
function streamEvents(scanId: string, after: number): Response {
  const entry = db.scan(scanId)!;
  const encoder = new TextEncoder();
  const queue: Uint8Array[] = [];
  let wake: (() => void) | null = null;
  let closed = false;

  const write = (e: ScanEvent) => {
    queue.push(encoder.encode(`id: ${e.seq}\nevent: ${e.type}\ndata: ${JSON.stringify(e)}\n\n`));
    wake?.();
  };
  const closeIfResting = () => {
    if (db.atRest(scanId)) {
      closed = true;
      wake?.();
    }
  };
  const listener = (e: ScanEvent) => {
    write(e);
    closeIfResting();
  };

  entry.events.filter((e) => e.seq > after).forEach(write);
  entry.listeners.add(listener);
  closeIfResting();

  const reader = {
    async read(): Promise<{ done: boolean; value?: Uint8Array }> {
      for (;;) {
        const next = queue.shift();
        if (next) return { done: false, value: next };
        if (closed) {
          entry.listeners.delete(listener);
          return { done: true };
        }
        await new Promise<void>((r) => (wake = r));
        wake = null;
      }
    },
    async cancel() {
      closed = true;
      entry.listeners.delete(listener);
      wake?.();
    },
    releaseLock() {},
  };

  return {
    ok: true,
    status: 200,
    headers: new Headers({ "Content-Type": "text/event-stream" }),
    body: { getReader: () => reader },
    text: async () => "",
    json: async () => null,
  } as unknown as Response;
}

async function upload(url: string, parts: UploadPart[]): Promise<Response> {
  await delay();
  const id = new URL(url).pathname.split("/")[2]!;
  const entry = db.scan(id);
  if (!entry) return notFound();
  const d = entry.detail;
  const kind = (parts.find((p) => p.name === "kind" && "value" in p) as { value: ImageKind } | undefined)?.value;
  const file = parts.find((p) => "file" in p) as { file: { uri: string } } | undefined;
  if (!kind || !file) return error(400, "invalid_request", "Missing the photo.", { field: "file", problem: "missing" });
  if (d.status !== "pending") return error(409, "invalid_state", "This scan has already been sent.", { status: d.status });
  if (d.images.filter((i) => i.kind === kind).length >= KIND_LIMIT[kind]) {
    return error(422, "too_many_images", "You already have this shot. Delete it to take it again.", { kind, max: KIND_LIMIT[kind] });
  }
  if (db.failNextUpload) {
    db.failNextUpload = false;
    return error(422, "image_quality", "This photo is too blurry. Hold the phone still and tap the leaf to focus.", {
      problem: "too_blurry",
      blur_score: 41.2,
      exposure: 0.52,
      hint: "tap_to_focus",
    });
  }
  const image = { id: ulid(), kind, width: 1200, height: 1600, url: file.file.uri, uploaded_at: ts() };
  d.images.push(image);
  if (kind === "whole_plant") {
    const p = plantOf(d.plant_id);
    if (p?.latest_scan?.scan_id === d.id) {
      p.latest_scan.thumbnail_image_id = image.id;
      p.latest_scan.thumbnail_url = image.url;
    }
  }
  return json(201, { image, quality: { blur_score: 182.4, exposure: 0.51, passed: true } });
}

function delay(ms = LATENCY_MS * db.pace) {
  return new Promise((r) => setTimeout(r, ms));
}

export const mockTransport: Transport = {
  async fetch(input, init) {
    const req = typeof input === "string" ? new Request(input, init) : input;
    if (!req.url.includes("/events")) await delay();
    try {
      return await route(req);
    } catch (e) {
      console.warn("[mock]", e);
      return error(500, "internal", "The mock backend hit a bug.");
    }
  },
  upload: (url, parts) => upload(url, parts),
};

export { db as mockDb } from "./store";
export { useMockScenario, type Scenario } from "./store";
