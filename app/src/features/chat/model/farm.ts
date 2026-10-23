// Farm citations in a chat answer (EON E10.3): a vendor's record opens Slack or Drive; an `era` record's link
// points at Era's simulator and opens nothing, so its text opens in a sheet, labelled as synthetic (EON
// decision 10). A `FarmRecordId` is "<connector>.<n>.<vendor_id>" (connectors 4), so the connector, and with it
// the endpoint, comes from the id.
import type { FarmCitationWithText } from "@/domain/proposed";
import type { Connector } from "@/domain/types";

export function connectorIdOf(recordId: string): string {
  const dot = recordId.indexOf(".");
  return dot === -1 ? recordId : recordId.slice(0, dot);
}

export function isEraRecord(c: Pick<FarmCitationWithText, "record_id">, connectors: readonly Pick<Connector, "id" | "endpoint">[]): boolean {
  const id = connectorIdOf(c.record_id);
  return connectors.some((x) => x.id === id && x.endpoint === "era");
}

export const SYSTEM_WORD: Record<FarmCitationWithText["system"], string> = { slack: "Slack", google_drive: "Google Drive" };

export type FarmOpen = { kind: "external"; url: string } | { kind: "sheet"; citation: FarmCitationWithText };

/**
 * What a tap on a farm citation does. Until the connectors are known (or when they don't list the record's), the
 * sheet opens: a simulator link must never be opened, and the sheet offers a vendor record's link anyway.
 */
export function openFarmCitation(c: FarmCitationWithText, connectors: readonly Pick<Connector, "id" | "endpoint">[] | undefined): FarmOpen {
  const id = connectorIdOf(c.record_id);
  const connector = connectors?.find((x) => x.id === id);
  return connector?.endpoint === "vendor" ? { kind: "external", url: c.link } : { kind: "sheet", citation: c };
}
