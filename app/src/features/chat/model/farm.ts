// Farm citations in a chat answer (EON E10.3): a vendor's record opens Slack or Drive; an `era` record's link
// points at Era's simulator and opens nothing, so its text opens in a sheet, labelled as synthetic (EON
// decision 10). A `FarmRecordId` is "<connector>.<n>.<vendor_id>" (connectors 4), so the connector, and with it
// the endpoint, comes from the id.
import type { Connector, FarmCitation } from "@/domain/types";

export function connectorIdOf(recordId: string): string {
  const dot = recordId.indexOf(".");
  return dot === -1 ? recordId : recordId.slice(0, dot);
}

/** A record from a connector the app knows to be a real vendor's: the only kind whose link may be opened. */
export function isVendorRecord(c: Pick<FarmCitation, "record_id">, connectors: readonly Pick<Connector, "id" | "endpoint">[] | undefined): boolean {
  const id = connectorIdOf(c.record_id);
  return connectors?.find((x) => x.id === id)?.endpoint === "vendor";
}

export const SYSTEM_WORD: Record<FarmCitation["system"], string> = { slack: "Slack", google_drive: "Google Drive" };

export type FarmOpen = { kind: "external"; url: string } | { kind: "sheet"; citation: FarmCitation };

/**
 * What a tap on a farm citation does. Until the connectors are known, or when they don't list the record's, the
 * sheet opens, as demo data with no link: a simulator link must never be opened (EON E9.3, E10.3).
 */
export function openFarmCitation(c: FarmCitation, connectors: readonly Pick<Connector, "id" | "endpoint">[] | undefined): FarmOpen {
  return isVendorRecord(c, connectors) ? { kind: "external", url: c.link } : { kind: "sheet", citation: c };
}
