// Mock members, invites and deletion against contracts 9: roles, the last owner, invite codes, accepting by code
// as the demo controls' second account, a garden's deletion and the account's.
import { randomUUID } from "crypto";

import { chooseAccount, DEMO_INVITE_CODE, GUEST, IDS, mockStore, mockTransport, resetMock, scenario } from "./index";

const BASE = "https://mock.pdm.local";

async function call(method: string, path: string, body?: unknown) {
  const res = await mockTransport.fetch(`${BASE}${path}`, {
    method,
    headers: { "content-type": "application/json", "Idempotency-Key": randomUUID() },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  return { status: res.status, json: text ? JSON.parse(text) : undefined };
}

const g = IDS.garden;

beforeEach(() => resetMock());

describe("members (contracts 9)", () => {
  it("lists the demo garden's owner, editor and viewer by since", async () => {
    const res = await call("GET", `/gardens/${g}/members`);
    expect(res.status).toBe(200);
    expect(res.json.items.map((m: { display_name: string; role: string }) => `${m.display_name}:${m.role}`)).toEqual([
      "Alex Doe:owner",
      "Sam Lee:editor",
      "Jo Park:viewer",
    ]);
    expect((await call("GET", `/gardens/${g}`)).json.member_count).toBe(3);
  });

  it("refuses to demote or remove the last owner, and lets an owner be added", async () => {
    expect((await call("PATCH", `/gardens/${g}/members/${IDS.user}`, { role: "editor" })).status).toBe(409);
    expect((await call("DELETE", `/gardens/${g}/members/${IDS.user}`)).status).toBe(409);
    expect((await call("PATCH", `/gardens/${g}/members/user_sam`, { role: "owner" })).json.role).toBe("owner");
    expect((await call("DELETE", `/gardens/${g}/members/${IDS.user}`)).status).toBe(204);
    expect((await call("GET", "/gardens")).json.items).toEqual([]);
  });

  it("keeps owner actions for owners", async () => {
    scenario.set({ role: "viewer" });
    expect((await call("PATCH", `/gardens/${g}/members/user_jo`, { role: "editor" })).status).toBe(403);
    expect((await call("GET", `/gardens/${g}/invites`)).status).toBe(403);
    expect((await call("DELETE", `/gardens/${g}`)).status).toBe(403);
  });
});

describe("invites (contracts 9; flows 3)", () => {
  it("makes a 26-character base32 code valid for 7 days, lists it first and revokes it", async () => {
    const made = await call("POST", `/gardens/${g}/invites`, { role: "viewer" });
    expect(made.status).toBe(201);
    expect(made.json.code).toMatch(/^[A-Z2-7]{26}$/);
    expect(Date.parse(made.json.expires_at) - Date.now()).toBeGreaterThan(6.9 * 86_400_000);
    const list = await call("GET", `/gardens/${g}/invites`);
    expect(list.json.items[0].id).toBe(made.json.id);
    expect((await call("DELETE", `/gardens/${g}/invites/${made.json.id}`)).status).toBe(204);
    expect((await call("POST", `/invites/${made.json.code}/accept`)).status).toBe(404); // a revoked code stops working
  });

  it("lets the second account join with the seeded code, once", async () => {
    chooseAccount("guest");
    expect((await call("GET", "/me")).json.id).toBe(GUEST.id);
    expect((await call("GET", "/gardens")).json.items).toEqual([]);
    const joined = await call("POST", `/invites/${DEMO_INVITE_CODE}/accept`);
    expect(joined.status).toBe(200);
    expect(joined.json).toMatchObject({ id: g, my_role: "editor", member_count: 4 });
    expect((await call("GET", "/gardens")).json.items.map((x: { id: string }) => x.id)).toEqual([g]);
    // A used code is unknown to everyone else.
    mockStore.user = { ...GUEST, id: "user_other" };
    expect((await call("POST", `/invites/${DEMO_INVITE_CODE}/accept`)).status).toBe(404);
  });

  it("answers 404 for an unknown code and 429 after 10 tries a minute", async () => {
    chooseAccount("guest");
    for (let i = 0; i < 10; i++) expect((await call("POST", "/invites/AAAAAAAAAAAAAAAAAAAAAAAAAA/accept")).status).toBe(404);
    expect((await call("POST", "/invites/AAAAAAAAAAAAAAAAAAAAAAAAAA/accept")).status).toBe(429);
  });
});

describe("gardens and the account (contracts 9)", () => {
  it("changes a garden's location and rounds owned gardens when consent is withdrawn", async () => {
    const moved = await call("PATCH", `/gardens/${g}`, { location: { lat: 43.6, lon: -79.4, precision: "about_1_km" } });
    expect(moved.json.location_precision).toBe("about_1_km");
    await call("PATCH", "/me", { location_consent: false });
    expect((await call("GET", `/gardens/${g}`)).json.location_precision).toBe("about_10_km");
  });

  it("deletes a garden with its plants", async () => {
    expect((await call("DELETE", `/gardens/${g}`)).status).toBe(202);
    expect((await call("GET", "/gardens")).json.items).toEqual([]);
    expect([...mockStore.plants.values()].some((p) => p.garden_id === g)).toBe(false);
  });

  it("hands a shared garden to its editor when its only owner deletes the account, then starts over", async () => {
    const res = await call("DELETE", "/me");
    expect(res.status).toBe(202);
    expect(res.json).toEqual({ deleted_gardens: [], transferred_gardens: [{ garden_id: g, new_owner_user_id: "user_sam" }] });
    expect(scenario.get().emptyAccount).toBe(true);
  });

  it("deletes a garden only the user is in", async () => {
    const made = await call("POST", "/gardens", { name: "Allotment", location: { city: "Guelph, ON" } });
    expect(made.json.my_role).toBe("owner");
    const res = await call("DELETE", "/me");
    expect(res.json.deleted_gardens).toEqual([made.json.id]);
  });
});
