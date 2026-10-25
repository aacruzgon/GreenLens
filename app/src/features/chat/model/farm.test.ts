import type { FarmCitation } from "@/domain/types";

import { connectorIdOf, isVendorRecord, openFarmCitation } from "./farm";

const citation = (record_id: string): FarmCitation => ({
  record_id,
  system: "slack",
  title: "#field-reports",
  at: "2026-10-20T18:05:00Z",
  link: "https://farm.slack.com/archives/C1/p1",
});

const connectors = [
  { id: "conn_era", endpoint: "era" as const },
  { id: "conn_real", endpoint: "vendor" as const },
];

describe("farm citations (EON E10.3)", () => {
  it("takes the connector from the record id (connectors 4)", () => {
    expect(connectorIdOf("conn_era.1.1729500000.000100")).toBe("conn_era");
  });

  it("opens a vendor record in its app and an era record's text in a sheet", () => {
    expect(openFarmCitation(citation("conn_real.1.x"), connectors)).toEqual({ kind: "external", url: "https://farm.slack.com/archives/C1/p1" });
    expect(openFarmCitation(citation("conn_era.1.x"), connectors).kind).toBe("sheet");
    expect(isVendorRecord(citation("conn_era.1.x"), connectors)).toBe(false);
    expect(isVendorRecord(citation("conn_real.1.x"), connectors)).toBe(true);
  });

  it("never opens a link before the connectors are known", () => {
    expect(openFarmCitation(citation("conn_real.1.x"), undefined).kind).toBe("sheet");
    expect(openFarmCitation(citation("conn_gone.1.x"), connectors).kind).toBe("sheet");
    // The sheet offers Open only for a known vendor connector (found in review).
    expect(isVendorRecord(citation("conn_gone.1.x"), connectors)).toBe(false);
    expect(isVendorRecord(citation("conn_real.1.x"), undefined)).toBe(false);
  });
});
