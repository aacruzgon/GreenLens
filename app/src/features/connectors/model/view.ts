// What the Farm connectors screen shows for a connector (EON E10.1; docs/connectors/README.md 2, 4 and 5): its
// status, account, sources, last sync and record count, and which owner actions it offers. An `era` connector is
// the demo garden's synthetic data and offers none (EON decision 11).
import { can } from "@/domain/permissions";
import type { Connector, ConnectorKind, Role } from "@/domain/types";

export const VENDOR: Record<ConnectorKind, string> = { slack: "Slack", google_drive: "Google Drive" };

export const ERA_LABEL = "Demo data from Era by Eon (synthetic)";

/** Connectors 2, item 2, as the screen says it. Drive's says "folder" where Slack's says "channels". */
export const CONSENT: Record<ConnectorKind, string> = {
  slack:
    "Every member of this garden can ask about what's in these channels. PDM keeps a copy of their text for 180 days, and deletes it when you disconnect.",
  google_drive:
    "Every member of this garden can ask about what's in this folder. PDM keeps a copy of its text for 180 days, and deletes it when you disconnect.",
};

/** Connectors 5: what the picker says about what it lists. */
export const PICKER_NOTE: Record<ConnectorKind, string> = {
  slack: "Only channels the PDM bot has been invited to are listed. To add one, type /invite @PDM in it. Choose up to 10.",
  google_drive: "Choose one folder. Google's permission covers your whole Drive, but PDM reads only this folder.",
};

export const STATUS_WORD: Record<Connector["status"], string> = {
  pending: "Waiting for consent",
  active: "Connected",
  reauth_needed: "Needs reconnecting",
  deleting: "Disconnecting",
};

/** Connectors per garden, `deleting` ones left out and `era` ones counted, as the store counts them (contracts 9). */
export const MAX_CONNECTORS_PER_GARDEN = 2;

/** The garden can't take another connector: the create would be `409 invalid_state`. */
export function atConnectorCap(connectors: readonly Pick<Connector, "status">[]): boolean {
  return connectors.filter((c) => c.status !== "deleting").length >= MAX_CONNECTORS_PER_GARDEN;
}

/** Said instead of the Connect buttons; the demo garden's `era` connectors can't be disconnected, so no advice. */
export const CAP_NOTE = `A garden holds at most ${MAX_CONNECTORS_PER_GARDEN} farm connectors, and this one has ${MAX_CONNECTORS_PER_GARDEN}.`;

export type ConnectorAction = "choose" | "sync" | "reconnect" | "disconnect" | "cancel";

export interface ConnectorView {
  title: string;
  status: string;
  era: boolean;
  account?: string;
  /** "#field-reports, #crew", or the folder's name. */
  sources?: string;
  /** "91 messages", "5 documents". */
  records: string;
  actions: ConnectorAction[];
  /** The label of `choose`. */
  chooseLabel: string;
  /** A line under the status for the states that need one. */
  note?: string;
}

export function sourceName(kind: ConnectorKind, name: string): string {
  return kind === "slack" && !name.startsWith("#") ? `#${name}` : name;
}

function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString("en-US")} ${n === 1 ? one : many}`;
}

/** The owner actions per status; none for a demo connector or a member below owner. */
export function actionsFor(c: Pick<Connector, "status" | "endpoint">, role: Role | undefined): ConnectorAction[] {
  if (c.endpoint === "era" || !can(role, "connectors.manage")) return [];
  switch (c.status) {
    case "pending":
      return ["cancel"];
    case "active":
      return ["choose", "sync", "disconnect"];
    case "reauth_needed":
      return ["reconnect", "disconnect"];
    case "deleting":
      return [];
  }
}

export function connectorView(c: Connector, role: Role | undefined): ConnectorView {
  const account = c.account ? (c.account.kind === "slack" ? c.account.team_name : c.account.email) : undefined;
  const sources = c.sources.length > 0 ? c.sources.map((s) => sourceName(c.kind, s.name)).join(", ") : undefined;
  const note =
    c.status === "pending"
      ? `Finish in ${VENDOR[c.kind]}'s page. A connect that isn't finished is dropped after 10 minutes.`
      : c.status === "reauth_needed"
        ? `${VENDOR[c.kind]} stopped accepting PDM's access. What was synced stays searchable until you reconnect or disconnect.`
        : c.status === "deleting"
          ? "PDM is revoking its access and deleting the synced records."
          : c.status === "active" && c.sources.length === 0
            ? `Nothing is synced until ${c.kind === "slack" ? "channels are" : "a folder is"} chosen.`
            : undefined;
  return {
    title: VENDOR[c.kind],
    status: STATUS_WORD[c.status],
    era: c.endpoint === "era",
    account,
    sources,
    records: c.kind === "slack" ? plural(c.record_count, "message", "messages") : plural(c.record_count, "document", "documents"),
    actions: actionsFor(c, role),
    chooseLabel: c.kind === "slack" ? "Choose channels" : "Choose folder",
    note,
  };
}

/** The slice of an ApiError the wording needs; this model can't import the api layer. */
export interface ErrorLike {
  code?: string;
  status?: number;
  retryAfterS?: number;
}

/** What a failed connector call says (contracts 2; connectors 5, as built). */
export function errorText(e: ErrorLike | undefined, kind?: ConnectorKind): string {
  const vendor = kind ? VENDOR[kind] : "The vendor";
  switch (e?.code) {
    case "forbidden":
      return "Only the garden's owners can do that.";
    case "invalid_state":
      return "That isn't possible right now. Farm connectors may be off on this server, or this connector changed.";
    case "upstream_unavailable":
      return `${vendor} isn't answering. Try again in a few minutes.`;
    case "rate_limited":
      return "PDM synced this less than 5 minutes ago. Try again in a few minutes.";
    case "invalid_request":
      return "The connect link expired or was already used. Start again.";
    case "not_found":
      return "That connect didn't start on this phone, or it timed out. Start again.";
    default:
      return "Something went wrong. Check your network and try again.";
  }
}

/** The redirect said the owner declined, or the vendor stopped. */
export function declinedText(error: string, kind: ConnectorKind): string {
  return error === "access_denied"
    ? `You didn't allow PDM in ${VENDOR[kind]}. Nothing was connected.`
    : `${VENDOR[kind]} didn't finish connecting. Nothing was connected.`;
}
