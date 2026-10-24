// Mock members, invites and deletion (docs/frontend/README.md 15; contracts 9): the demo garden's members, invite
// codes of 26 base32 characters, accepting one by code, a garden's deletion and the account's. The demo controls'
// second account (`becomeGuest`) is someone else on the same phone, holding the newest invite to the demo garden,
// so an invite accepted on a second account can be shown with no second phone (roadmap F7 exit checks).

import { IDS } from "./fixtures";
import { scenario } from "./scenario";
import type { MockStore } from "./store";
import { emptyResponse, errorResponse, jsonResponse, nowTs, paginate, type DateKit, type S } from "./util";

const DAY_MS = 86_400_000;
const BASE32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/** The seeded editor invite to the demo garden, which the second account accepts unless the owner made a newer one. */
export const DEMO_INVITE_CODE = "BACKGARDENEDITORINVITE2345";

export const GUEST: S["Me"] = { id: "user_riley", display_name: "Riley Chen", location_consent: false, created_at: "2026-10-01T12:00:00Z" };

export interface StoredInvite {
  invite: S["Invite"];
  gardenId: string;
  used: boolean;
}

/** The members half of the store, reset with it. */
export class MemberState {
  /** Garden → its members, by `since`. */
  members = new Map<string, S["Member"][]>();
  invites: StoredInvite[] = [];
  /** Accept attempts per user, for the 10-a-minute limit. */
  attempts = new Map<string, number[]>();
}

export function seedMembers(state: MemberState, k: DateKit, owner: S["Me"]): void {
  state.members.set(IDS.garden, [
    { user_id: owner.id, display_name: owner.display_name, role: "owner", since: k.ts(40) },
    { user_id: "user_sam", display_name: "Sam Lee", role: "editor", since: k.ts(25) },
    { user_id: "user_jo", display_name: "Jo Park", role: "viewer", since: k.ts(12) },
  ]);
  state.invites = [
    {
      gardenId: IDS.garden,
      used: false,
      invite: { id: "inv_demo", code: DEMO_INVITE_CODE, role: "editor", created_by: owner.id, expires_at: k.ts(-5) },
    },
  ];
}

export function newInviteCode(): string {
  let code = "";
  for (let i = 0; i < 26; i++) code += BASE32[Math.floor(Math.random() * 32)];
  return code;
}

/**
 * The caller's role in a garden, or undefined when they aren't a member. In the demo garden the demo controls'
 * role chip decides, as it did before there were members.
 */
export function myRole(store: MockStore, gardenId: string): S["Role"] | undefined {
  const me = store.people.members.get(gardenId)?.find((m) => m.user_id === store.user.id);
  if (!me) return undefined;
  return gardenId === IDS.garden ? scenario.get().role : me.role;
}

/** The members as the API lists them, with the caller's role as `myRole` sees it. */
function membersOf(store: MockStore, gardenId: string): S["Member"][] {
  return (store.people.members.get(gardenId) ?? []).map((m) =>
    m.user_id === store.user.id ? { ...m, role: myRole(store, gardenId) ?? m.role } : m,
  );
}

export function addMember(store: MockStore, gardenId: string, user: S["Me"], role: S["Role"]): void {
  const list = store.people.members.get(gardenId) ?? [];
  list.push({ user_id: user.id, display_name: user.display_name, role, since: nowTs() });
  store.people.members.set(gardenId, list);
  const g = store.gardens.get(gardenId);
  if (g) g.member_count = list.length;
}

function setRole(store: MockStore, gardenId: string, userId: string, role: S["Role"]): S["Member"] | undefined {
  const m = store.people.members.get(gardenId)?.find((x) => x.user_id === userId);
  if (!m) return undefined;
  m.role = role;
  if (userId === store.user.id && gardenId === IDS.garden) scenario.set({ role });
  return m;
}

function owners(store: MockStore, gardenId: string): S["Member"][] {
  return membersOf(store, gardenId).filter((m) => m.role === "owner");
}

function dropMember(store: MockStore, gardenId: string, userId: string): void {
  const list = (store.people.members.get(gardenId) ?? []).filter((m) => m.user_id !== userId);
  store.people.members.set(gardenId, list);
  const g = store.gardens.get(gardenId);
  if (g) g.member_count = list.length;
}

/** A garden and everything in it (contracts 9, deletion cascades). */
export function deleteGarden(store: MockStore, gardenId: string): void {
  const plantIds = new Set([...store.plants.values()].filter((p) => p.garden_id === gardenId).map((p) => p.id));
  for (const [id, st] of store.scans) if (plantIds.has(st.scan.plant_id)) store.scans.delete(id);
  for (const id of plantIds) store.plants.delete(id);
  store.tasks = store.tasks.filter((t) => t.garden_id !== gardenId);
  store.memories = store.memories.filter((m) => m.garden_id !== gardenId);
  for (const [id, c] of store.chat.conversations) if (c.conversation.garden_id === gardenId) store.chat.conversations.delete(id);
  store.chat.connectors = store.chat.connectors.filter((c) => c.garden_id !== gardenId);
  store.gardens.delete(gardenId);
  store.maps.delete(gardenId);
  store.people.members.delete(gardenId);
  store.people.invites = store.people.invites.filter((i) => i.gardenId !== gardenId);
}

/** Switches the mock to the demo controls' second account: someone with a name and no gardens. */
export function becomeGuest(store: MockStore): void {
  // After the new-account demo the demo garden is gone; the second account needs it to join.
  if (!store.gardens.has(IDS.garden)) store.reset();
  store.user = { ...GUEST };
  store.nameSet = true;
}

/** The code the second account opens: the owner's newest open invite to the demo garden. */
export function guestInviteCode(store: MockStore): string {
  const open = store.people.invites.filter((i) => i.gardenId === IDS.garden && !i.used && Date.parse(i.invite.expires_at) > Date.now());
  return open.at(-1)?.invite.code ?? DEMO_INVITE_CODE;
}

interface MemberRouteCtx {
  params: Record<string, string>;
  query: URLSearchParams;
  headers: Headers;
  store: MockStore;
  json(): Promise<unknown>;
}

type Register = (method: string, pattern: string, handler: (c: MemberRouteCtx) => Response | Promise<Response>) => void;

const ROLES: readonly S["Role"][] = ["owner", "editor", "reviewer", "viewer"];

export function registerMemberRoutes(route: Register, h: { jsonObject(c: MemberRouteCtx): Promise<Record<string, unknown>> }): void {
  const notFound = () => errorResponse("not_found");
  const forbidden = (need: S["Role"]) => errorResponse("forbidden", { required_role: need });
  const invalid = (field: string, problem: string) => errorResponse("invalid_request", { field, problem });
  /** The garden, when the caller is a member with at least `need`; otherwise the error to send. */
  const guard = (c: MemberRouteCtx, need: "member" | "owner"): Response | null => {
    if (!c.store.gardens.has(c.params.id)) return notFound();
    const role = myRole(c.store, c.params.id);
    if (!role) return notFound();
    if (need === "owner" && role !== "owner") return forbidden("owner");
    return null;
  };

  route("DELETE", "/gardens/:id", (c) => {
    const refused = guard(c, "owner");
    if (refused) return refused;
    deleteGarden(c.store, c.params.id);
    return emptyResponse(202);
  });

  route("GET", "/gardens/:id/members", (c) => {
    const refused = guard(c, "member");
    if (refused) return refused;
    return jsonResponse(200, paginate(membersOf(c.store, c.params.id), c.query));
  });

  route("PATCH", "/gardens/:id/members/:user_id", async (c) => {
    const refused = guard(c, "owner");
    if (refused) return refused;
    const body = await h.jsonObject(c);
    const role = body.role as S["Role"];
    if (!ROLES.includes(role)) return invalid("role", "owner, editor, reviewer or viewer");
    const target = membersOf(c.store, c.params.id).find((m) => m.user_id === c.params.user_id);
    if (!target) return notFound();
    if (target.role === "owner" && role !== "owner" && owners(c.store, c.params.id).length === 1) {
      return errorResponse("invalid_state", { hint: "last_owner" }, "A garden needs at least one owner.");
    }
    const member = setRole(c.store, c.params.id, c.params.user_id, role);
    return member ? jsonResponse(200, membersOf(c.store, c.params.id).find((m) => m.user_id === member.user_id)) : notFound();
  });

  route("DELETE", "/gardens/:id/members/:user_id", (c) => {
    const self = c.params.user_id === c.store.user.id;
    const refused = guard(c, self ? "member" : "owner");
    if (refused) return refused;
    const target = membersOf(c.store, c.params.id).find((m) => m.user_id === c.params.user_id);
    if (!target) return notFound();
    if (target.role === "owner" && owners(c.store, c.params.id).length === 1) {
      return errorResponse("invalid_state", { hint: "last_owner" }, "The last owner can't leave.");
    }
    dropMember(c.store, c.params.id, c.params.user_id);
    return emptyResponse(204);
  });

  route("GET", "/gardens/:id/invites", (c) => {
    const refused = guard(c, "owner");
    if (refused) return refused;
    const now = Date.now();
    const open = c.store.people.invites
      .filter((i) => i.gardenId === c.params.id && !i.used && Date.parse(i.invite.expires_at) > now)
      .map((i) => i.invite)
      .reverse();
    return jsonResponse(200, paginate(open, c.query));
  });

  route("POST", "/gardens/:id/invites", async (c) => {
    const refused = guard(c, "owner");
    if (refused) return refused;
    const body = await h.jsonObject(c);
    const role = body.role as S["Role"];
    if (!ROLES.includes(role)) return invalid("role", "owner, editor, reviewer or viewer");
    // A retried create with the same key answers with the invite it made (contracts 1).
    const key = c.headers.get("idempotency-key");
    const prior = key ? c.store.people.invites.find((i) => i.invite.id === `inv_${key}`) : undefined;
    if (prior) return jsonResponse(201, prior.invite);
    const invite: S["Invite"] = {
      id: key ? `inv_${key}` : c.store.nextId("inv"),
      code: newInviteCode(),
      role,
      created_by: c.store.user.id,
      expires_at: nowTs(7 * DAY_MS),
    };
    c.store.people.invites.push({ invite, gardenId: c.params.id, used: false });
    return jsonResponse(201, invite);
  });

  route("DELETE", "/gardens/:id/invites/:invite_id", (c) => {
    const refused = guard(c, "owner");
    if (refused) return refused;
    const before = c.store.people.invites.length;
    c.store.people.invites = c.store.people.invites.filter((i) => !(i.gardenId === c.params.id && i.invite.id === c.params.invite_id));
    return c.store.people.invites.length < before ? emptyResponse(204) : notFound();
  });

  route("POST", "/invites/:code/accept", (c) => {
    const now = Date.now();
    const recent = (c.store.people.attempts.get(c.store.user.id) ?? []).filter((t) => now - t < 60_000);
    recent.push(now);
    c.store.people.attempts.set(c.store.user.id, recent);
    if (recent.length > 10) return errorResponse("rate_limited");
    const stored = c.store.people.invites.find((i) => i.invite.code === c.params.code.toUpperCase());
    const garden = stored ? c.store.gardens.get(stored.gardenId) : undefined;
    if (!stored || !garden) return notFound();
    // An existing member gets the garden with their role unchanged, and the code stays unused.
    if (!myRole(c.store, garden.id)) {
      if (stored.used || Date.parse(stored.invite.expires_at) <= now) return notFound();
      stored.used = true;
      addMember(c.store, garden.id, c.store.user, stored.invite.role);
      // The demo garden's role is the demo controls' chip; it now shows the invite's.
      if (garden.id === IDS.garden) scenario.set({ role: stored.invite.role });
    }
    return jsonResponse(200, { ...garden, my_role: myRole(c.store, garden.id), member_count: membersOf(c.store, garden.id).length });
  });

  // Account deletion (architecture 5, deletion): sole-member gardens go; a garden the user alone owns passes to the
  // longest-standing editor, or goes when there is none. The mock then starts over as a new, empty account.
  route("DELETE", "/me", (c) => {
    const result: S["AccountDeletion"] = { deleted_gardens: [], transferred_gardens: [] };
    for (const gardenId of [...c.store.gardens.keys()]) {
      const list = membersOf(c.store, gardenId);
      if (!list.some((m) => m.user_id === c.store.user.id)) continue;
      const others = list.filter((m) => m.user_id !== c.store.user.id);
      const soleOwner = myRole(c.store, gardenId) === "owner" && !others.some((m) => m.role === "owner");
      if (others.length === 0) {
        result.deleted_gardens.push(gardenId);
      } else if (soleOwner) {
        const heir = others.find((m) => m.role === "editor");
        if (heir) result.transferred_gardens.push({ garden_id: gardenId, new_owner_user_id: heir.user_id });
        else result.deleted_gardens.push(gardenId);
      }
    }
    c.store.reset();
    scenario.set({ account: "new", emptyAccount: true, role: "owner" });
    return jsonResponse(202, result);
  });
}
