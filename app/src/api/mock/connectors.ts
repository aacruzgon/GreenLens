// Mock farm connectors (docs/connectors/README.md 5 and 11; docs/frontend/README.md 15): the garden's list, a
// fake vendor connect with the device check of connectors 5, the source picker, Sync now with its 5-minute limit
// and disconnect. The demo garden's two `era` connectors are seeded with the chat (chat.ts, seedChat) and refuse
// every owner route with `409 invalid_state`, as the server's do (EON decision 11).

import * as Crypto from "expo-crypto";

import { scenario } from "./scenario";
import type { MockStore } from "./store";
import { emptyResponse, errorResponse, jsonResponse, MOCK_ORIGIN, nowTs, type S } from "./util";

// ---- state, kept per store generation so a reset clears it

interface PendingConnect {
  deviceHash: string;
  state: string;
}

interface ConnectState {
  generation: number;
  available: boolean;
  pending: Map<string, PendingConnect>;
  /** Connector → when its last Sync now was accepted (ms). */
  lastSync: Map<string, number>;
  /** Idempotency key → the create's answer, the replay of a retried connect (contracts 1). */
  replays: Map<string, S["CreateConnectorResponse"]>;
  counter: number;
}

let current: ConnectState | undefined;

function stateOf(store: MockStore): ConnectState {
  if (current?.generation !== store.generation) {
    current = { generation: store.generation, available: true, pending: new Map(), lastSync: new Map(), replays: new Map(), counter: 0 };
  }
  return current;
}

/** What the fake consent page does next: the owner allows, closes the session, or declines. */
export type MockConsent = "allow" | "cancel" | "access_denied";

let nextConsent: MockConsent = "allow";

/** Test and demo controls. Both clear with the store's next reset (consent: after one use). */
export const mockConnectors = {
  /** `available: false`: a server without `CONNECTOR_KEY` (connectors 5, as built). */
  setAvailable(store: MockStore, available: boolean): void {
    stateOf(store).available = available;
  },
  setNextConsent(c: MockConsent): void {
    nextConsent = c;
  },
};

const SYNC_LIMIT_MS = 5 * 60_000;
/** Per garden, `deleting` ones left out and `era` ones counted (domain rules/limits.rs; store farm.rs, CREATE_CONNECTOR). */
const MAX_CONNECTORS_PER_GARDEN = 2;
const DEVICE = /^[A-Za-z0-9_-]{43}$/;

const AVAILABLE: Record<S["ConnectorKind"], S["ConnectorSource"][]> = {
  slack: [
    { id: "C0FIELD", name: "field-reports" },
    { id: "C0CREW", name: "crew" },
    { id: "C0IRRIG", name: "irrigation" },
    { id: "C0MARKET", name: "market-days" },
  ],
  google_drive: [
    { id: "folder_farm", name: "Farm records" },
    { id: "folder_spray", name: "Spray logs" },
    { id: "folder_seed", name: "Seed orders" },
  ],
};

const ACCOUNT: Record<S["ConnectorKind"], S["ConnectorAccount"]> = {
  slack: { kind: "slack", team_id: "T0MAPLE", team_name: "Maple Row Farm", url: "https://maple-row-farm.slack.com/" },
  google_drive: { kind: "google_drive", email: "owner@maplerow.farm" },
};

/** Records a first sync of these sources gives: a channel's messages, a folder's document chunks. */
const PER_SOURCE: Record<S["ConnectorKind"], number> = { slack: 34, google_drive: 12 };

function b64url(bytes: Uint8Array): string {
  const A = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const n = (bytes[i]! << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    const chars = i + 2 < bytes.length ? 4 : i + 1 < bytes.length ? 3 : 2;
    for (let j = 0; j < chars; j++) out += A[(n >> (18 - 6 * j)) & 63];
  }
  return out;
}

/** `SHA-256(device_verifier) = device_hash`, as the server checks it (api connect.rs, `device_matches`). */
async function hashOf(verifier: string): Promise<string> {
  const ascii = Uint8Array.from(verifier, (ch) => ch.charCodeAt(0));
  return b64url(new Uint8Array(await Crypto.digest(Crypto.CryptoDigestAlgorithm.SHA256, ascii)));
}

/**
 * The fake consent page, for `authSession.setFake` in mock mode: it answers at once with the redirect the API's
 * callback would send, `{CONNECTOR_RETURN_URL}/{id}/callback?code&state` (connectors 5, as built).
 */
export async function mockAuthSession(url: string, redirectUrl: string): Promise<{ type: "success"; url: string } | { type: "cancel" }> {
  const consent = nextConsent;
  nextConsent = "allow";
  const parsed = new URL(url, MOCK_ORIGIN);
  const state = parsed.searchParams.get("state");
  if (!url.startsWith(MOCK_ORIGIN) || !state || consent === "cancel") return { type: "cancel" };
  const id = state.slice(0, state.indexOf("."));
  const tail = consent === "allow" ? `code=${encodeURIComponent(`mock-code-${id}`)}` : `error=${consent}`;
  return { type: "success", url: `${redirectUrl}/${id}/callback?${tail}&state=${encodeURIComponent(state)}` };
}

// ---- routes

export interface ConnectorRouteCtx {
  params: Record<string, string>;
  headers: Headers;
  store: MockStore;
  json(): Promise<unknown>;
}

type Register = (
  method: string,
  pattern: string,
  handler: (c: ConnectorRouteCtx) => Response | Promise<Response>,
  minRole?: S["Role"] | boolean,
) => void;

export interface ConnectorRouteHelpers {
  jsonObject(c: ConnectorRouteCtx): Promise<Record<string, unknown>>;
}

export function registerConnectorRoutes(route: Register, h: ConnectorRouteHelpers): void {
  const invalid = (field: string, problem: string) => errorResponse("invalid_request", { field, problem });
  const notFound = () => errorResponse("not_found");
  const list = (store: MockStore) => store.chat.connectors;
  const find = (store: MockStore, id: string) => list(store).find((x) => x.id === id);

  /** The access checks passed; connectors off, a demo connector, or the wrong status is `409 invalid_state`. */
  function refuse(store: MockStore, c: S["Connector"] | undefined, allowed: S["ConnectorStatus"][]): Response | undefined {
    if (!c) return notFound();
    if (!stateOf(store).available || c.endpoint === "era") return errorResponse("invalid_state", { status: c.status });
    if (!allowed.includes(c.status)) return errorResponse("invalid_state", { status: c.status, allowed });
    return undefined;
  }

  route("GET", "/gardens/:id/connectors", (c) => {
    if (!c.store.gardens.has(c.params.id)) return notFound();
    if (!stateOf(c.store).available) return jsonResponse(200, { available: false, connectors: [] });
    return jsonResponse(200, { available: true, connectors: list(c.store).filter((x) => x.garden_id === c.params.id) });
  });

  route(
    "POST",
    "/gardens/:id/connectors",
    async (c) => {
      if (!c.store.gardens.has(c.params.id)) return notFound();
      const st = stateOf(c.store);
      if (!st.available) return errorResponse("invalid_state", { available: false });
      const key = c.headers.get("idempotency-key");
      const prior = key ? st.replays.get(key) : undefined;
      if (prior) return jsonResponse(201, prior);
      const body = await h.jsonObject(c);
      if (body.kind !== "slack" && body.kind !== "google_drive") return invalid("kind", "slack or google_drive");
      if (typeof body.device_hash !== "string" || !DEVICE.test(body.device_hash)) return invalid("device_hash", "43 base64url characters");
      // The store throws `connector_limit`, which the API answers as `409 invalid_state` with empty details (contracts 9).
      const held = list(c.store).filter((x) => x.garden_id === c.params.id && x.status !== "deleting").length;
      if (held >= MAX_CONNECTORS_PER_GARDEN) return errorResponse("invalid_state", {});
      st.counter += 1;
      const id = `conn_mock_${st.counter}`;
      const state = `${id}.nonce${st.counter}.${Math.floor(Date.now() / 1000) + 600}.mac`;
      const connector: S["Connector"] = {
        id,
        garden_id: c.params.id,
        kind: body.kind,
        status: "pending",
        endpoint: "vendor",
        sources: [],
        record_count: 0,
        created_at: nowTs(),
      };
      list(c.store).push(connector);
      st.pending.set(id, { deviceHash: body.device_hash, state });
      const answer: S["CreateConnectorResponse"] = {
        connector,
        authorize_url: `${MOCK_ORIGIN}/oauth/${body.kind}/authorize?state=${encodeURIComponent(state)}`,
      };
      if (key) st.replays.set(key, answer);
      return jsonResponse(201, answer);
    },
    "owner",
  );

  route(
    "POST",
    "/connectors/:id/complete",
    async (c) => {
      const connector = find(c.store, c.params.id);
      const refused = refuse(c.store, connector, ["pending", "active"]);
      if (refused || !connector) return refused ?? notFound();
      const body = await h.jsonObject(c);
      const pending = stateOf(c.store).pending.get(connector.id);
      // A retried complete whose first answer was lost: the connector is already active.
      if (!pending) return connector.status === "active" ? jsonResponse(200, connector) : invalid("state", "used or expired");
      if (body.state !== pending.state) return invalid("state", "used or expired");
      if (body.code !== `mock-code-${connector.id}`) return invalid("code", "the vendor refused the code");
      if (typeof body.device_verifier !== "string" || !DEVICE.test(body.device_verifier)) return invalid("device_verifier", "43 base64url characters");
      if ((await hashOf(body.device_verifier)) !== pending.deviceHash) {
        // Another device, or another user: the pending connector is deleted (connectors 5).
        c.store.chat.connectors = list(c.store).filter((x) => x.id !== connector.id);
        stateOf(c.store).pending.delete(connector.id);
        return notFound();
      }
      stateOf(c.store).pending.delete(connector.id);
      connector.status = "active";
      connector.account = ACCOUNT[connector.kind];
      return jsonResponse(200, connector);
    },
    "owner",
  );

  route(
    "GET",
    "/connectors/:id/available-sources",
    (c) => {
      const connector = find(c.store, c.params.id);
      const refused = refuse(c.store, connector, ["active"]);
      if (refused || !connector) return refused ?? notFound();
      return jsonResponse(200, { sources: AVAILABLE[connector.kind] });
    },
    "owner",
  );

  route(
    "PUT",
    "/connectors/:id/sources",
    async (c) => {
      const connector = find(c.store, c.params.id);
      const refused = refuse(c.store, connector, ["active"]);
      if (refused || !connector) return refused ?? notFound();
      const body = await h.jsonObject(c);
      if (!Array.isArray(body.sources)) return invalid("sources", "an array of { id }");
      const ids = body.sources.map((s: unknown) => (typeof s === "object" && s !== null ? (s as { id?: unknown }).id : undefined));
      const max = connector.kind === "slack" ? 10 : 1;
      if (ids.length > max) return invalid("sources", `at most ${max}`);
      const chosen: S["ConnectorSource"][] = [];
      for (const id of ids) {
        const source = AVAILABLE[connector.kind].find((s) => s.id === id);
        if (!source) return invalid("sources", "an id from available-sources");
        if (!chosen.some((s) => s.id === source.id)) chosen.push(source);
      }
      connector.sources = chosen;
      // The first sync of the new choice, done at once in the mock.
      connector.record_count = chosen.length * PER_SOURCE[connector.kind];
      if (chosen.length > 0) connector.synced_at = nowTs();
      return jsonResponse(200, connector);
    },
    "owner",
  );

  route(
    "POST",
    "/connectors/:id/sync",
    (c) => {
      const connector = find(c.store, c.params.id);
      const refused = refuse(c.store, connector, ["active"]);
      if (refused || !connector) return refused ?? notFound();
      const st = stateOf(c.store);
      const last = st.lastSync.get(connector.id);
      if (last !== undefined && Date.now() - last < SYNC_LIMIT_MS) return errorResponse("rate_limited");
      st.lastSync.set(connector.id, Date.now());
      connector.synced_at = nowTs();
      if (connector.sources.length > 0) connector.record_count += 1;
      return emptyResponse(202);
    },
    "owner",
  );

  route(
    "DELETE",
    "/connectors/:id",
    (c) => {
      const connector = find(c.store, c.params.id);
      const refused = refuse(c.store, connector, ["pending", "active", "reauth_needed", "deleting"]);
      if (refused || !connector) return refused ?? notFound();
      connector.status = "deleting";
      stateOf(c.store).pending.delete(connector.id);
      // The revoke and the record deletion run in the background; the connector goes when they finish.
      const generation = c.store.generation;
      setTimeout(() => {
        if (c.store.generation !== generation) return;
        c.store.chat.connectors = list(c.store).filter((x) => x.id !== connector.id);
      }, 1_500 * scenario.get().delayScale);
      return emptyResponse(202);
    },
    "owner",
  );
}
