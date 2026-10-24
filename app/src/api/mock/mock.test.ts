import { TZDate } from "@date-fns/tz";
import { format } from "date-fns";

import { SseParser } from "../sse";
import type { S } from "./util";
import { chooseAccount, IDS, mockStore, mockTransport, resetMock, scenario } from "./index";
import { buildSeed } from "./fixtures";
import { bandFor, DateKit, MOCK_TIME_ZONE } from "./util";

const BASE = "https://mock.pdm.local";

async function call(method: string, path: string, body?: unknown, headers?: Record<string, string>) {
  const res = await mockTransport.fetch(`${BASE}${path}`, {
    method,
    headers: { "content-type": "application/json", ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = res.status === 204 ? "" : await res.text();
  return { status: res.status, json: text ? JSON.parse(text) : undefined };
}

async function upload(scanId: string, kind: string) {
  const form = new FormData();
  form.append("kind", kind);
  form.append("file", new Blob(["x"], { type: "image/jpeg" }), "photo.jpg");
  const res = await mockTransport.fetch(`${BASE}/scans/${scanId}/images`, { method: "POST", body: form });
  return { status: res.status, json: JSON.parse(await res.text()) };
}

/** Reads an SSE response; `until` stops early and leaves the reader open for `rest()`. */
class Events {
  readonly seen: S["ScanEvent"][] = [];
  private readonly reader: ReadableStreamDefaultReader<Uint8Array>;
  private readonly parser = new SseParser();
  closed = false;
  raw = "";
  private readonly decoder = new TextDecoder();

  constructor(res: Response) {
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toBe("text/event-stream");
    this.reader = res.body!.getReader();
  }

  async until(pred: (e: S["ScanEvent"]) => boolean): Promise<void> {
    while (!this.closed) {
      if (this.seen.some(pred)) return;
      const { done, value } = await this.reader.read();
      if (done) {
        this.closed = true;
        break;
      }
      this.raw += this.decoder.decode(value, { stream: true });
      for (const m of this.parser.push(value)) {
        const e = JSON.parse(m.data) as S["ScanEvent"];
        expect(m.id).toBe(String(e.seq));
        expect(m.event).toBe(e.type);
        this.seen.push(e);
      }
    }
  }

  async rest(): Promise<S["ScanEvent"][]> {
    await this.until(() => false);
    return this.seen;
  }

  cancel() {
    return this.reader.cancel();
  }
}

function open(scanId: string, lastEventId?: number) {
  return mockTransport
    .fetch(`${BASE}/scans/${scanId}/events`, {
      headers: lastEventId === undefined ? {} : { "Last-Event-ID": String(lastEventId) },
    })
    .then((r) => new Events(r));
}

async function newScan(plantId: string, withPhoto = true): Promise<string> {
  const created = await call("POST", "/scans", { garden_id: IDS.garden, plant_id: plantId });
  expect(created.status).toBe(201);
  if (withPhoto) expect((await upload(created.json.id, "whole_plant")).status).toBe(201);
  return created.json.id;
}

function expectOrdered(events: S["ScanEvent"][]) {
  expect(events.length).toBeGreaterThan(0);
  events.forEach((e, i) => {
    expect(e.seq).toBe(i + 1);
    expect(e.v).toBe(1);
  });
}

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0, keepaliveMs: 50 });
});

afterAll(() => {
  resetMock();
});

describe("fixtures over REST", () => {
  it("GET /gardens returns the Back garden", async () => {
    const r = await call("GET", "/gardens");
    expect(r.status).toBe(200);
    expect(r.json.items).toHaveLength(1);
    expect(r.json.items[0]).toMatchObject({
      id: IDS.garden,
      name: "Back garden",
      time_zone: "America/Toronto",
      location_label: "Toronto, ON",
      my_role: "owner",
    });
    expect((await call("GET", "/me")).json.display_name).toBe("Alex Doe");
  });

  it("has the six plants with their statuses and Tomato 1's growth series", async () => {
    const plants = (await call("GET", `/plants?garden_id=${IDS.garden}`)).json.items as S["Plant"][];
    expect(plants.map((p) => p.name).sort()).toEqual(["Basil", "Kale", "Lettuce", "Pepper", "Tomato 1", "Tomato 2"]);
    const status = Object.fromEntries(plants.map((p) => [p.name, p.status]));
    expect(status).toMatchObject({ "Tomato 1": "sick", "Tomato 2": "healthy", Pepper: "watch", Basil: "healthy" });

    const pts = (await call("GET", `/plants/${IDS.tomato1}/measurements?quantity=plant_height_cm`)).json.items as S["MeasurementPoint"][];
    expect(pts.map((p) => p.value)).toEqual([32, 41, 50, 58, 60, 62]);
    const widths = (await call("GET", `/plants/${IDS.tomato1}/measurements?quantity=canopy_width_cm`)).json.items as S["MeasurementPoint"][];
    expect(widths.at(-1)?.value).toBe(48);
  });

  it("dates the scans relative to today", async () => {
    const tl = (await call("GET", `/plants/${IDS.tomato1}/timeline`)).json.items as S["TimelineEntry"][];
    expect(tl).toHaveLength(6);
    const days = tl.map((e) => Math.round((Date.now() - Date.parse(e.created_at)) / 86_400_000));
    expect(days[0]).toBeLessThanOrEqual(1);
    expect(days.at(-1)).toBeGreaterThanOrEqual(26);
    expect(tl[2].primary?.name).toBe("Early blight");
  });

  it("dates nothing seeded after now, early in the morning too, and keeps today's events in order", () => {
    for (const [hour, minute] of [[0, 3], [7, 30], [11, 0], [15, 0]]) {
      const now = new TZDate(2026, 9, 24, hour, minute, MOCK_TIME_ZONE);
      const seed = buildSeed(new DateKit(now));
      const times = [
        ...seed.scans.flatMap((s) => [s.scan.created_at, ...s.events.map((e) => e.ts)]),
        ...seed.tasks.flatMap((t) => [t.created_at, t.done_at ?? t.created_at]),
        seed.weather.fetched_at,
      ];
      for (const t of times) expect(Date.parse(t)).toBeLessThanOrEqual(now.getTime());
      const today = seed.scans.filter((s) => new TZDate(Date.parse(s.scan.created_at), MOCK_TIME_ZONE).getDate() === 24);
      expect(today.length).toBeGreaterThan(0);
      for (const s of today) {
        const ts = s.events.map((e) => Date.parse(e.ts));
        expect(ts).toEqual([...ts].sort((a, b) => a - b));
      }
    }
  });

  it("forecasts the frost on the coming Thursday within the home's 6 days, and on Friday on a Thursday", () => {
    // Monday Oct 19, 2026 to Sunday Oct 25: the strip shows the 6 days after today.
    for (let day = 19; day <= 25; day++) {
      const seed = buildSeed(new DateKit(new TZDate(2026, 9, day, 9, 0, MOCK_TIME_ZONE)));
      const frost = seed.weather.forecast.findIndex((d) => d.frost_risk) + 1;
      const weekday = new Date(2026, 9, day + frost).getDay();
      expect(frost).toBeGreaterThanOrEqual(1);
      expect(frost).toBeLessThanOrEqual(6);
      expect(weekday).toBe(day === 22 ? 5 : 4);
    }
  });

  it("holds Tomato 1's plan, the tasks, the frost alert and the garden note", async () => {
    const open = (await call("GET", `/tasks?garden_id=${IDS.garden}&state=open`)).json.items as S["Task"][];
    expect(open.find((t) => t.plant_id === IDS.tomato2 && t.due === new DateKit().date(0))?.title).toBe("Check Tomato 2's lower leaves");
    const alerts = (await call("GET", `/tasks?garden_id=${IDS.garden}&source=forecast_alert`)).json.items as S["Task"][];
    expect(alerts).toHaveLength(2);
    const upcoming = alerts.find((t) => !t.done_at)!;
    expect(upcoming.trigger?.kind).toBe("frost");

    const weather = (await call("GET", `/gardens/${IDS.garden}/weather`)).json.weather as S["WeatherSummary"];
    expect(weather.forecast).toHaveLength(7);
    const frost = weather.forecast.find((d) => d.frost_risk)!;
    expect(frost.t_min_c).toBe(-1);
    expect(upcoming.trigger?.date).toBe(frost.date);
    expect(upcoming.title).toBe(`Frost on ${format(new Date(`${frost.date}T12:00:00`), "EEEE")}: cover tender plants`);

    const memories = (await call("GET", `/memories?garden_id=${IDS.garden}`)).json.items as S["Memory"][];
    expect(memories.some((m) => m.text === "Everything is in containers")).toBe(true);

    const scan = (await call("GET", "/scans/scan_tomato1_6")).json as S["ScanDetail"];
    expect(scan.plan?.version).toBe(1);
    expect(scan.diagnosis?.risks[0]).toMatchObject({ risk: "fungal_spread", level: "medium", drivers: ["3 humid days in a row"] });
    const area = scan.observations.find((o) => o.detail?.measurement?.quantity === "affected_area_pct")!;
    expect(area.detail?.measurement).toMatchObject({ value: 14, min: 9, max: 21, n: 3 });
    expect(scan.weather?.past_7_days).toMatchObject({ hours_rh_over_90: 46, longest_humid_run_days: 3 });
  });

  it("answers with ErrorBody: 404, 400, 403 for a viewer", async () => {
    const nf = await call("GET", "/nope");
    expect(nf.status).toBe(404);
    expect(nf.json.error).toMatchObject({ code: "not_found", retryable: false });
    expect(typeof nf.json.error.request_id).toBe("string");
    expect((await call("GET", "/plants")).json.error.code).toBe("invalid_request");
    expect((await call("GET", "/plants/missing")).status).toBe(404);
    scenario.set({ role: "viewer" });
    const denied = await call("POST", "/scans", { garden_id: IDS.garden, plant_id: IDS.kale });
    expect(denied.status).toBe(403);
    expect(denied.json.error).toMatchObject({ code: "forbidden", details: { required_role: "editor" } });
    expect((await call("GET", "/gardens")).json.items[0].my_role).toBe("viewer");
  });

  it("completing a repeating task opens the next one", async () => {
    const before = (await call("GET", `/tasks?garden_id=${IDS.garden}&state=open`)).json.items as S["Task"][];
    const check = before.find((t) => t.plant_id === IDS.tomato2)!;
    const done = await call("PATCH", `/tasks/${check.id}`, { done: true });
    expect(done.status).toBe(200);
    expect(done.json.done_at).toBeDefined();
    const after = (await call("GET", `/tasks?garden_id=${IDS.garden}&state=open`)).json.items as S["Task"][];
    expect(after.some((t) => t.id === check.id)).toBe(false);
    expect(after.some((t) => t.plant_id === IDS.tomato2 && t.due && t.due > check.due!)).toBe(true);
  });
});

describe("uploads and faults", () => {
  it("a fault makes the next upload return 422 image_quality, then clears", async () => {
    const scanId = (await call("POST", "/scans", { garden_id: IDS.garden, plant_id: IDS.tomato2 })).json.id;
    scenario.set({ uploadFault: "image_quality" });
    const bad = await upload(scanId, "whole_plant");
    expect(bad.status).toBe(422);
    expect(bad.json.error).toMatchObject({ code: "image_quality", details: { problem: "too_blurry", hint: "tap_to_focus" } });
    const good = await upload(scanId, "whole_plant");
    expect(good.status).toBe(201);
    expect(good.json.quality.passed).toBe(true);
    expect(good.json.image.url).toMatch(/^mock:\/\/images\//);
    expect((await upload(scanId, "whole_plant")).json.error.code).toBe("too_many_images");
  });

  it("reads an upload's fields from React Native's Request, whose formData() can't read a form", async () => {
    const scanId = (await call("POST", "/scans", { garden_id: IDS.garden, plant_id: IDS.tomato2 })).json.id;
    const form = new FormData();
    form.append("kind", "leaf_closeup");
    form.append("file", new Blob(["x"], { type: "image/jpeg" }), "photo.jpg");
    // As openapi-fetch hands it over on a phone: a Request and no init, with whatwg-fetch's failing formData().
    const request = Object.assign(new Request(`${BASE}/scans/${scanId}/images`, { method: "POST", body: form }), {
      _bodyFormData: form,
      clone: () => ({ formData: () => Promise.reject(new Error("could not read FormData body as text")) }),
    });
    const res = await mockTransport.fetch(request as unknown as Request);
    expect(res.status).toBe(201);
    expect(mockStore.scan(scanId)!.scan.images.map((i) => i.kind)).toEqual(["leaf_closeup"]);
  });

  it("refuses submit without a whole_plant shot, and other faults hit the next call", async () => {
    const scanId = await newScan(IDS.kale, false);
    const r = await call("POST", `/scans/${scanId}/submit`);
    expect(r.status).toBe(422);
    expect(r.json.error).toMatchObject({ code: "missing_required_shot", details: { missing: ["whole_plant"] } });
    scenario.set({ callFault: "budget_exceeded" });
    const b = await call("GET", "/me");
    expect(b.status).toBe(429);
    expect(b.json.error.details.budget).toBe("scans");
    expect((await call("GET", "/me")).status).toBe(200);
  });
});

describe("scripted runs", () => {
  async function runToEnd(plantId: string, nextScan: "auto" | "healthy" | "low_evidence" | "no_plant_found" | "early_blight" = "auto") {
    scenario.set({ nextScan });
    const scanId = await newScan(plantId);
    const submit = await call("POST", `/scans/${scanId}/submit`);
    expect(submit.status).toBe(202);
    expect(submit.json.status).toBe("queued");
    const events = await open(scanId);
    return { scanId, events };
  }

  it("a healthy plant ends planned with every event in seq order", async () => {
    const { scanId, events } = await runToEnd(IDS.kale);
    const all = await events.rest();
    expectOrdered(all);
    expect(all[0]).toMatchObject({ type: "run_started", reason: "submit" });
    expect(all.at(-1)).toMatchObject({ type: "plan_ready", provisional: false });
    const detail = (await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"];
    expect(detail.status).toBe("planned");
    expect(detail.diagnosis?.status).toBe("healthy");
    expect(detail.last_event_seq).toBe(all.length);
  });

  it("low_evidence gives an uncertain, low-evidence result with a provisional plan", async () => {
    const { scanId, events } = await runToEnd(IDS.basil, "low_evidence");
    expectOrdered(await events.rest());
    const d = (await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"];
    expect(d.diagnosis).toMatchObject({ status: "uncertain", low_evidence: true });
    expect(d.plan?.provisional).toBe(true);
    expect((await call("GET", `/plants/${IDS.basil}`)).json.status).toBe("watch");
  });

  it("no_plant_found fails the scan and closes the stream", async () => {
    const { scanId, events } = await runToEnd(IDS.lettuce, "no_plant_found");
    const all = await events.rest();
    expectOrdered(all);
    expect(all.at(-1)).toMatchObject({ type: "failed", failure: { code: "no_plant_found", retryable: false } });
    expect((await call("GET", `/scans/${scanId}`)).json.status).toBe("failed");
  });

  it("Tomato 2's live scan: uncertain until the rings question, then early blight after Yes", async () => {
    const { scanId, events } = await runToEnd(IDS.tomato2);
    await events.until((e) => e.type === "question");
    const q = events.seen.find((e) => e.type === "question") as Extract<S["ScanEvent"], { type: "question" }>;
    expect(q.question).toMatchObject({ kind: "indicator", status: "open" });
    expect(q.question.options?.map((o) => o.label)).toEqual(["Yes", "No", "Not sure"]);
    // The classifier is torn between early blight and Septoria leaf spot.
    const names = events.seen.flatMap((e) => (e.type === "finding" && e.observation.stage === "disease" ? [e.observation.label] : []));
    expect(names).toEqual(["Early blight", "Septoria leaf spot"]);
    expect(events.seen.some((e) => e.type === "finding" && e.observation.role === "beneficial")).toBe(true);

    await events.until((e) => e.type === "status_changed" && e.to === "awaiting_answer");
    const mid = (await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"];
    expect(mid.status).toBe("awaiting_answer");
    expect(mid.open_question?.id).toBe(q.question.id);
    expect(mid.diagnosis).toBeUndefined();

    const ans = await call("POST", `/scans/${scanId}/answers`, { question_id: q.question.id, option_id: "yes" });
    expect(ans.status).toBe(202);
    expect(ans.json.open_question.status).toBe("answering");
    expect((await call("POST", `/scans/${scanId}/answers`, { question_id: q.question.id, option_id: "yes" })).status).toBe(409);

    const all = await events.rest();
    expectOrdered(all);
    expect(all.at(-1)).toMatchObject({ type: "plan_ready", provisional: false });
    const after = (await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"];
    expect(after.status).toBe("planned");
    expect(after.open_question).toBeUndefined();
    expect(after.diagnosis).toMatchObject({ status: "confirmed", primary: { problem_id: "tomato_early_blight" } });
    expect(after.diagnosis?.risks[0]).toMatchObject({ level: "high" });
    expect(after.diagnosis?.risks[0].drivers).toEqual(["4 humid days in the forecast", "Crowded: 40 cm from Tomato 1"]);
    expect(after.diagnosis?.organisms[0]).toMatchObject({ name: "Ladybug larva", role: "beneficial", preys_on: [{ name: "Aphids" }] });
    expect(after.plan?.task_ids.length).toBeGreaterThan(0);
    expect(after.plant.status).toBe("sick");
    // Mock uploads have no depth, so the run measures nothing (contracts 4).
    expect(after.observations.some((o) => o.detail?.measurement?.quantity === "plant_height_cm" && o.scan_id === scanId)).toBe(false);
  });

  it("Not sure gives an uncertain diagnosis and a provisional plan", async () => {
    const { scanId, events } = await runToEnd(IDS.tomato2);
    await events.until((e) => e.type === "status_changed" && e.to === "awaiting_answer");
    const qid = (await call("GET", `/scans/${scanId}`)).json.open_question.id;
    expect((await call("POST", `/scans/${scanId}/answers`, { question_id: qid, option_id: "unsure" })).status).toBe(202);
    const all = await events.rest();
    expectOrdered(all);
    expect(all.at(-1)).toMatchObject({ type: "plan_ready", provisional: true });
    const d = (await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"];
    expect(d.diagnosis?.status).toBe("uncertain");
    expect(d.diagnosis?.primary).toBeUndefined();
    expect(d.plan?.provisional).toBe(true);
    expect(d.plant.status).toBe("watch");
  });

  it("replay with Last-Event-ID resumes after it and the run is unaffected", async () => {
    const { scanId, events } = await runToEnd(IDS.kale);
    const all = await events.rest();
    const cut = 7;
    const replay = await (await open(scanId, cut)).rest();
    expect(replay.map((e) => e.seq)).toEqual(all.slice(cut).map((e) => e.seq));
    expect(replay[0].seq).toBe(cut + 1);
    expect(await (await open(scanId, all.length)).rest()).toEqual([]);
    // Without Last-Event-ID the whole history is replayed.
    expect((await (await open(scanId)).rest()).length).toBe(all.length);
  });

  it("a dropped stream resumes where it stopped", async () => {
    const { scanId, events } = await runToEnd(IDS.kale);
    await events.rest();
    scenario.set({ dropNextStream: true });
    const dropped = await open(scanId);
    await expect(dropped.rest()).rejects.toThrow();
    expect(dropped.seen.map((e) => e.seq)).toEqual([1, 2, 3]);
    const resumed = await (await open(scanId, 3)).rest();
    expect(resumed[0].seq).toBe(4);
  });

  it("sends a keepalive comment while a question waits", async () => {
    const { events } = await runToEnd(IDS.tomato2);
    await events.until((e) => e.type === "status_changed" && e.to === "awaiting_answer");
    for (let i = 0; i < 40 && !events.raw.includes(": keepalive"); i++) {
      await new Promise((r) => setTimeout(r, 20));
      await Promise.race([events.until(() => false), new Promise((r) => setTimeout(r, 20))]);
    }
    expect(events.raw).toContain(": keepalive\n\n");
    await events.cancel();
  });
});

describe("review fixes", () => {
  it("every seeded scan is at rest: planned with its plan, or failed", async () => {
    const plants = (await call("GET", `/plants?garden_id=${IDS.garden}`)).json.items as S["Plant"][];
    let n = 0;
    for (const p of plants) {
      const tl = (await call("GET", `/plants/${p.id}/timeline?limit=100`)).json.items as S["TimelineEntry"][];
      for (const e of tl) {
        n += 1;
        expect(["planned", "failed"]).toContain(e.status);
        if (e.status === "planned") {
          const d = (await call("GET", `/scans/${e.scan_id}`)).json as S["ScanDetail"];
          expect(d.plan?.id).toBe(d.plan_id);
        }
      }
    }
    expect(n).toBeGreaterThan(20);
  });

  it("confidence bands: high from 0.8, medium from 0.5", async () => {
    expect([0.8, 0.79, 0.5, 0.49].map(bandFor)).toEqual(["high", "medium", "medium", "low"]);
  });

  it("submit is idempotent, and DELETE only takes a pending scan", async () => {
    const scanId = await newScan(IDS.kale);
    expect((await call("POST", `/scans/${scanId}/submit`)).status).toBe(202);
    const again = await call("POST", `/scans/${scanId}/submit`);
    expect(again.status).toBe(200);
    expect(again.json.id).toBe(scanId);
    expect((await call("DELETE", `/scans/${scanId}`)).json.error).toMatchObject({ code: "invalid_state", details: { status: expect.any(String) } });
    const pending = await newScan(IDS.basil);
    expect((await call("DELETE", `/scans/${pending}`)).status).toBe(204);
    expect((await call("GET", `/scans/${pending}`)).status).toBe(404);
  });

  it("a non-retryable failed scan refuses submit; a retryable one starts a new run", async () => {
    scenario.set({ nextScan: "no_plant_found" });
    const scanId = await newScan(IDS.lettuce);
    await call("POST", `/scans/${scanId}/submit`);
    await (await open(scanId)).rest();
    const refused = await call("POST", `/scans/${scanId}/submit`);
    expect(refused.status).toBe(409);
    expect(refused.json.error.details.hint).toBe("new_scan");
    mockStore.scan(scanId)!.scan.failure!.retryable = true;
    const retry = await call("POST", `/scans/${scanId}/submit`);
    expect(retry.status).toBe(202);
    expect(retry.json.run).toBe(2);
  });

  it("kind limits apply before submit; guided checks after submit are unlimited", async () => {
    const scanId = await newScan(IDS.kale);
    expect((await upload(scanId, "jar_test")).status).toBe(201);
    expect((await upload(scanId, "jar_test")).json.error.code).toBe("too_many_images");
    await call("POST", `/scans/${scanId}/submit`);
    expect((await upload(scanId, "jar_test")).status).toBe(201);
    expect((await upload(scanId, "jar_test")).status).toBe(201);
  });

  it("a guided check's job finishes it with a texture, and re-plans a planned scan", async () => {
    scenario.set({ nextScan: "healthy" });
    const scanId = await newScan(IDS.kale);
    await call("POST", `/scans/${scanId}/submit`);
    await (await open(scanId)).rest();
    const planBefore = mockStore.scan(scanId)!.scan.plan_id;

    const image = (await upload(scanId, "jar_test")).json.image as S["ScanImage"];
    expect(((await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"]).guided_checks).toEqual([{ image_id: image.id, kind: "jar_test", status: "analysing" }]);
    await (await open(scanId)).rest();

    const detail = (await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"];
    const check = detail.guided_checks[0]!;
    expect(check.status).toBe("done");
    expect(check.result?.detail).toMatchObject({ aspect: "texture", value: "loamy", guided_kind: "jar_test" });
    expect(detail.diagnosis?.soil.some((o) => o.detail?.aspect === "texture")).toBe(true);
    const events = mockStore.scan(scanId)!.events;
    expect(events.find((e) => e.type === "plan_superseded")).toMatchObject({ old_plan_id: planBefore, reason: "guided_check" });
    expect(detail.plan?.id).not.toBe(planBefore);

    // A second jar of the same texture writes a diagnosis but keeps the plan.
    await upload(scanId, "jar_test");
    await (await open(scanId)).rest();
    expect(mockStore.scan(scanId)!.events.filter((e) => e.type === "plan_superseded")).toHaveLength(1);
  });

  it("a second species change keeps the first run's plant check, and confirms under the new run", async () => {
    scenario.set({ nextScan: "healthy" });
    const scanId = await newScan(IDS.kale);
    await call("POST", `/scans/${scanId}/submit`);
    await (await open(scanId)).rest();
    const runsOf = () => mockStore.scan(scanId)!.events.filter((e) => e.type === "run_started");

    for (const species_id of ["basil", "tomato"]) {
      expect((await call("POST", `/plants/${IDS.kale}/species`, { species_id })).status).toBe(200);
      await (await open(scanId)).rest();
      const started = runsOf().at(-1)!;
      expect(started.type === "run_started" && started.rerun_stages).not.toContain("plant_check");
      const confirmed = mockStore.scan(scanId)!.events.filter((e) => e.type === "species_confirmed").at(-1)!;
      expect(confirmed.run).toBe(started.run);
    }
    expect(runsOf().map((e) => e.run)).toEqual([1, 2, 3]);
    // The same species again changes nothing.
    await call("POST", `/plants/${IDS.kale}/species`, { species_id: "tomato" });
    expect(runsOf()).toHaveLength(3);
  });

  it("feedback needs reviewer, a submitted scan, and returns 201 { id }", async () => {
    const scanId = await newScan(IDS.kale);
    expect((await call("POST", `/scans/${scanId}/feedback`, { verdict: "correct" })).json.error.code).toBe("invalid_state");
    await call("POST", `/scans/${scanId}/submit`);
    const ok = await call("POST", `/scans/${scanId}/feedback`, { verdict: "correct" });
    expect(ok.status).toBe(201);
    expect(typeof ok.json.id).toBe("string");
    scenario.set({ role: "viewer" });
    const denied = await call("POST", `/scans/${scanId}/feedback`, { verdict: "correct" });
    expect(denied.status).toBe(403);
    expect(denied.json.error.details.required_role).toBe("reviewer");
    scenario.set({ role: "reviewer" });
    expect((await call("POST", `/scans/${scanId}/feedback`, { verdict: "wrong" })).status).toBe(201);
  });

  it("renaming a garden is owner only", async () => {
    scenario.set({ role: "editor" });
    const r = await call("PATCH", `/gardens/${IDS.garden}`, { name: "Front" });
    expect(r.status).toBe(403);
    expect(r.json.error.details.required_role).toBe("owner");
    scenario.set({ role: "owner" });
    expect((await call("PATCH", `/gardens/${IDS.garden}`, { name: "Front" })).json.name).toBe("Front");
  });

  it("timestamps have whole seconds", async () => {
    const re = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
    const scanId = await newScan(IDS.kale);
    const detail = (await call("GET", `/scans/${scanId}`)).json as S["ScanDetail"];
    expect(detail.created_at).toMatch(re);
    expect(detail.images[0].uploaded_at).toMatch(re);
    await call("POST", `/scans/${scanId}/submit`);
    const all = await (await open(scanId)).rest();
    for (const e of all) expect(e.ts).toMatch(re);
    expect((await call("GET", `/plants/${IDS.tomato1}/timeline`)).json.items[0].created_at).toMatch(re);
  });

  it("the measurement stage runs only when a photo has depth", async () => {
    const stages = (all: S["ScanEvent"][]) =>
      all.filter((e) => e.type === "stage_started").map((e) => (e as { stage: string }).stage);
    const a = await newScan(IDS.kale);
    const sub = await call("POST", `/scans/${a}/submit`);
    expect(sub.status).toBe(202);
    const evs = await (await open(a)).rest();
    expect(stages(evs)).not.toContain("measurement");
    const started = evs[0] as Extract<S["ScanEvent"], { type: "run_started" }>;
    expect(started.rerun_stages).not.toContain("measurement");

    const b = (await call("POST", "/scans", { garden_id: IDS.garden, plant_id: IDS.basil })).json.id as string;
    const form = new FormData();
    form.append("kind", "whole_plant");
    form.append("file", new Blob(["x"], { type: "image/jpeg" }), "p.jpg");
    form.append("depth", new Blob(["d"]), "d.bin");
    expect((await mockTransport.fetch(`${BASE}/scans/${b}/images`, { method: "POST", body: form })).status).toBe(201);
    await call("POST", `/scans/${b}/submit`);
    expect(stages(await (await open(b)).rest())).toContain("measurement");
  });

  it("the empty-account control hides the demo until a garden is created", async () => {
    scenario.get().setEmptyAccount(true);
    expect((await call("GET", "/me")).json.display_name).toBe("");
    expect((await call("GET", "/gardens")).json.items).toEqual([]);
    const made = await call("POST", "/gardens", { name: "Mine", location: { city: "Ottawa" } });
    expect(made.status).toBe(201);
    expect(scenario.get().emptyAccount).toBe(false);
    const list = (await call("GET", "/gardens")).json.items as S["Garden"][];
    expect(list.map((g) => g.name)).toEqual(["Mine"]);
    expect((await call("GET", "/me")).json.display_name).toBe("");
    expect((await call("PATCH", "/me", { display_name: "Sam" })).json.display_name).toBe("Sam");
    scenario.get().setEmptyAccount(true);
    resetMock();
    expect(scenario.get().emptyAccount).toBe(false);
  });

  it("chooseAccount keeps a new account chosen after its first garden, and brings the demo back", async () => {
    chooseAccount("new");
    expect(scenario.get()).toMatchObject({ account: "new", emptyAccount: true });
    await call("POST", "/gardens", { name: "Mine", location: { city: "Ottawa" } });
    // The transport clears emptyAccount with the first garden; the controls' account stays "new".
    expect(scenario.get()).toMatchObject({ account: "new", emptyAccount: false });

    chooseAccount("demo");
    expect(scenario.get()).toMatchObject({ account: "demo", emptyAccount: false });
    const names = ((await call("GET", "/gardens")).json.items as S["Garden"][]).map((g) => g.name);
    expect(names).toEqual(["Back garden"]);
    expect((await call("GET", `/gardens/${IDS.garden}`)).status).toBe(200);
  });

  it("chooseAccount leaves the demo alone when it is already chosen", async () => {
    const scan = (await call("POST", "/scans", { garden_id: IDS.garden, plant_id: IDS.basil })).json.id as string;
    chooseAccount("demo");
    expect(mockStore.scans.has(scan)).toBe(true);
  });
});
