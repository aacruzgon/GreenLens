// The connect flow (EON E10.2; docs/connectors/README.md 5): a device_verifier made here, its hash sent with the
// create, the vendor's consent page in an auth session, then `complete` with the redirect's code and state and the
// verifier, which is kept in this module's memory only, by connector, and dropped when the flow ends. A Reconnect
// deletes the `reauth_needed` connector first: a garden holds at most 2 connectors (contracts 9), so a create beside
// it would be refused in a full garden, and its synced records go with a reconnect anyway.
import { useRef, useState } from "react";

import { isApiError, useCompleteConnector, useCreateConnector, useDeleteConnector } from "@/api";
import type { Connector, ConnectorKind } from "@/domain/types";
import { authSession, deviceCrypto } from "@/platform";

import { deviceProof, parseRedirect, RETURN_URL } from "../model/connect";
import { declinedText, errorText } from "../model/view";

/** Connector id → its verifier, for the one flow in progress. Never stored, never sent before `complete`. */
const verifiers = new Map<string, string>();

export type ConnectPhase =
  | { step: "idle"; error?: string }
  | { step: "starting" | "consent" | "completing"; kind: ConnectorKind };

export interface ConnectFlow {
  phase: ConnectPhase;
  busy: boolean;
  /** Connect a vendor; `replaces` is the `reauth_needed` connector a Reconnect deletes before it connects. */
  start(kind: ConnectorKind, replaces?: Connector): Promise<void>;
  clearError(): void;
}

export function useConnectFlow(gardenId: string, onConnected: (c: Connector) => void): ConnectFlow {
  const [phase, setPhase] = useState<ConnectPhase>({ step: "idle" });
  // `phase` is the last render's: a second tap before it re-renders would start a second create.
  const running = useRef(false);
  const create = useCreateConnector(gardenId);
  const complete = useCompleteConnector();
  const remove = useDeleteConnector();

  // A connect that went nowhere: its pending connector would otherwise wait 10 minutes for the cleanup.
  const drop = (c: Connector) => {
    verifiers.delete(c.id);
    remove.mutate(c, { onError: () => undefined });
  };

  async function start(kind: ConnectorKind, replaces?: Connector) {
    if (running.current) return;
    running.current = true;
    try {
      await run(kind, replaces);
    } finally {
      running.current = false;
    }
  }

  async function run(kind: ConnectorKind, replaces?: Connector) {
    setPhase({ step: "starting", kind });
    let pending: Connector;
    let authorizeUrl: string;
    try {
      if (replaces) {
        verifiers.delete(replaces.id);
        await remove.mutateAsync(replaces);
      }
      const proof = await deviceProof(deviceCrypto.randomBytes(32), (d) => deviceCrypto.sha256(d));
      const res = await create.mutateAsync({ kind, device_hash: proof.hash });
      pending = res.connector;
      authorizeUrl = res.authorize_url;
      verifiers.set(pending.id, proof.verifier);
    } catch (e) {
      setPhase({ step: "idle", error: errorText(isApiError(e) ? e : undefined, kind) });
      return;
    }

    setPhase({ step: "consent", kind });
    const result = await authSession.open(authorizeUrl, RETURN_URL).catch(() => ({ type: "cancel" as const }));
    if (result.type !== "success") {
      drop(pending);
      setPhase({ step: "idle" });
      return;
    }
    const redirect = parseRedirect(result.url, pending.id);
    if (redirect.kind !== "code") {
      drop(pending);
      setPhase({ step: "idle", error: redirect.kind === "declined" ? declinedText(redirect.error, kind) : errorText({ code: "invalid_request" }, kind) });
      return;
    }

    setPhase({ step: "completing", kind });
    const verifier = verifiers.get(pending.id);
    try {
      if (!verifier) throw new Error("the verifier is gone");
      const connected = await complete.mutateAsync({
        id: pending.id,
        body: { code: redirect.code, state: redirect.state, device_verifier: verifier },
      });
      verifiers.delete(pending.id);
      setPhase({ step: "idle" });
      onConnected(connected);
    } catch (e) {
      // A 404 already deleted it on the server; anything else leaves a pending connector nothing can finish.
      drop(pending);
      setPhase({ step: "idle", error: errorText(isApiError(e) ? e : undefined, kind) });
    }
  }

  return {
    phase,
    busy: phase.step !== "idle",
    start,
    clearError: () => setPhase({ step: "idle" }),
  };
}
