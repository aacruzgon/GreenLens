import { IDS, mockStore, mockTransport, resetMock } from "./index";

const BASE = "https://mock.pdm.local";

async function call(method: string, path: string, body?: unknown) {
  const res = await mockTransport.fetch(`${BASE}${path}`, {
    method,
    headers: { "content-type": "application/json", "Idempotency-Key": crypto.randomUUID() },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  return { status: res.status, json: text ? JSON.parse(text) : undefined };
}

beforeEach(() => resetMock());

describe("PUT /gardens/{id}/map (contracts 9)", () => {
  const plant = () => [...mockStore.plants.values()].find((p) => p.garden_id === IDS.garden)!;

  it("stores a new version with the beds' areas and moves the plants to it", async () => {
    const before = (await call("GET", `/gardens/${IDS.garden}/map`)).json.map;
    const res = await call("PUT", `/gardens/${IDS.garden}/map`, {
      base_version: before.version,
      beds: [{ id: "b1", name: "New bed", polygon_m: [[0, 0], [2, 0], [2, 1.5], [0, 1.5]] }],
      pins: [
        { plant_id: plant().id, x_m: 1, y_m: 0.5, canopy_radius_m: 0.3, bed_id: "b1" },
        { new_plant: { name: "Chard" }, x_m: 1.5, y_m: 1 },
      ],
      north_deg: 270,
    });
    expect(res.status).toBe(200);
    expect(res.json.version).toBe(before.version + 1);
    expect(res.json.beds[0].area_m2).toBe(3);
    expect(res.json.pins).toHaveLength(2);
    expect(mockStore.plants.get(plant().id)?.position).toEqual({ map_version: before.version + 1, x_m: 1, y_m: 0.5 });
    expect([...mockStore.plants.values()].some((p) => p.name === "Chard")).toBe(true);
  });

  it("refuses an upload from a session that started on an older version", async () => {
    const res = await call("PUT", `/gardens/${IDS.garden}/map`, { base_version: 0, beds: [], pins: [] });
    expect(res.status).toBe(409);
    expect(res.json.error.code).toBe("invalid_state");
    expect(res.json.error.details.current_map_version).toBe(3);
  });

  it("lists every pin of a plant that isn't this garden's, and creates nothing", async () => {
    const before = (await call("GET", `/gardens/${IDS.garden}/map`)).json.map;
    const plants = mockStore.plants.size;
    const res = await call("PUT", `/gardens/${IDS.garden}/map`, {
      base_version: before.version,
      beds: [],
      pins: [
        { plant_id: "plt_gone1", x_m: 0, y_m: 0 },
        { new_plant: { name: "Chard" }, x_m: 1, y_m: 1 },
        { plant_id: "plt_gone2", x_m: 1, y_m: 0 },
      ],
    });
    expect(res.status).toBe(400);
    expect(res.json.error.code).toBe("invalid_request");
    expect(res.json.error.details).toEqual({ field: "pins", problem: "unknown_plant", plant_ids: ["plt_gone1", "plt_gone2"] });
    expect(mockStore.plants.size).toBe(plants);
  });
});
