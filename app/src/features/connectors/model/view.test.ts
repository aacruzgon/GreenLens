// What a connector shows and offers, per status, role and endpoint (EON E10.1).
import type { Connector } from "@/domain/types";

import { actionsFor, connectorView, errorText } from "./view";

const base: Connector = {
  id: "c1",
  garden_id: "g1",
  kind: "slack",
  status: "active",
  endpoint: "vendor",
  sources: [{ id: "C1", name: "field-reports" }],
  record_count: 1,
  created_at: "2026-10-20T09:00:00Z",
  account: { kind: "slack", team_id: "T1", team_name: "Maple Row Farm", url: "https://maple.slack.com/" },
};

describe("the connector view", () => {
  it("offers owners the actions of each status", () => {
    expect(actionsFor({ ...base, status: "pending" }, "owner")).toEqual(["cancel"]);
    expect(actionsFor(base, "owner")).toEqual(["choose", "sync", "disconnect"]);
    expect(actionsFor({ ...base, status: "reauth_needed" }, "owner")).toEqual(["reconnect", "disconnect"]);
    expect(actionsFor({ ...base, status: "deleting" }, "owner")).toEqual([]);
  });

  it("offers nothing on an era connector, or to anyone below owner", () => {
    expect(actionsFor({ ...base, endpoint: "era" }, "owner")).toEqual([]);
    expect(actionsFor(base, "editor")).toEqual([]);
    expect(actionsFor(base, undefined)).toEqual([]);
  });

  it("names the account, the channels with their #, and the record count", () => {
    const v = connectorView(base, "owner");
    expect(v.account).toBe("Maple Row Farm");
    expect(v.sources).toBe("#field-reports");
    expect(v.records).toBe("1 message");
    const drive = connectorView({ ...base, kind: "google_drive", account: { kind: "google_drive", email: "a@b.c" }, sources: [{ id: "f", name: "Farm records" }], record_count: 5 }, "owner");
    expect([drive.account, drive.sources, drive.records, drive.chooseLabel]).toEqual(["a@b.c", "Farm records", "5 documents", "Choose folder"]);
  });

  it("words the errors of connectors 5", () => {
    expect(errorText({ code: "rate_limited" })).toMatch(/5 minutes/);
    expect(errorText({ code: "upstream_unavailable" }, "slack")).toMatch(/^Slack isn't answering/);
    expect(errorText(undefined)).toMatch(/network/);
  });
});
