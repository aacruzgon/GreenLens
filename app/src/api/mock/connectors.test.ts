// Mock farm connectors against contracts 9: a garden holds at most 2 connectors, `era` ones counted and
// `deleting` ones left out, and a create past that is `409 invalid_state`, as the store's `connector_limit` answers.
import { randomUUID } from "crypto";

import { IDS, mockStore, mockTransport, resetMock } from "./index";

const BASE = "https://mock.pdm.local";
const HASH = "A".repeat(43);

async function create(kind: string) {
  const res = await mockTransport.fetch(`${BASE}/gardens/${IDS.garden}/connectors`, {
    method: "POST",
    headers: { "content-type": "application/json", "Idempotency-Key": randomUUID() },
    body: JSON.stringify({ kind, device_hash: HASH }),
  });
  return { status: res.status, json: JSON.parse(await res.text()) };
}

beforeEach(() => resetMock());

describe("the connector cap (contracts 9)", () => {
  it("refuses a third connector, counting the demo garden's era ones", async () => {
    expect(mockStore.chat.connectors.filter((c) => c.garden_id === IDS.garden && c.endpoint === "era")).toHaveLength(2);
    const res = await create("slack");
    expect(res.status).toBe(409);
    expect(res.json.error).toMatchObject({ code: "invalid_state", details: {} });
  });

  it("leaves deleting connectors out of the count", async () => {
    mockStore.chat.connectors = mockStore.chat.connectors.map((c, i) => (i === 0 ? { ...c, status: "deleting" as const } : c));
    expect((await create("slack")).status).toBe(201);
    expect((await create("google_drive")).status).toBe(409);
  });
});
