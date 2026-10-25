import type { Member } from "@/domain/types";

import { acceptProblem, canLeave, inviteLink, inviteMessage, inviteSubtitle, isLastOwner, locationLine, memberRows, parseInviteCode } from "./members";

const member = (user_id: string, role: Member["role"]): Member => ({ user_id, display_name: user_id, role, since: "2026-10-16T14:00:00Z" });
const team = [member("alex", "owner"), member("sam", "editor"), member("jo", "viewer")];
const CODE = "BACKGARDENEDITORINVITE2345";

describe("the garden tools' model (screens 3.9; flows 3)", () => {
  it("shares the app's scheme and route", () => {
    expect(inviteLink(CODE)).toBe(`pdm://invite/${CODE}`);
    expect(inviteMessage("Back garden", { code: CODE, role: "editor" })).toBe(`Join "Back garden" on PDM as an editor: pdm://invite/${CODE}`);
  });

  it("reads a code from a link or the code, in any case", () => {
    expect(parseInviteCode(`pdm://invite/${CODE.toLowerCase()}`)).toBe(CODE);
    expect(parseInviteCode(CODE)).toBe(CODE);
    expect(parseInviteCode("short")).toBeNull();
  });

  it("gives owners controls over others, never over themselves or the last owner", () => {
    const rows = memberRows(team, "alex", "owner", "America/Toronto");
    expect(rows.map((r) => r.canManage)).toEqual([false, true, true]);
    expect(rows[0]!.subtitle).toBe("owner · since Oct 16");
    expect(memberRows(team, "jo", "viewer", "America/Toronto").every((r) => !r.canManage)).toBe(true);
    const twoOwners = [member("alex", "owner"), member("sam", "owner")];
    expect(memberRows(twoOwners, "sam", "owner", "UTC")[0]!.canManage).toBe(true);
  });

  it("lets anyone but the last owner leave", () => {
    expect(isLastOwner(team, "alex")).toBe(true);
    expect(canLeave(team, "alex")).toBe(false);
    expect(canLeave(team, "jo")).toBe(true);
    expect(canLeave([member("alex", "owner"), member("sam", "owner")], "alex")).toBe(true);
  });

  it("words invites, locations and failed accepts", () => {
    expect(inviteSubtitle({ role: "editor", expires_at: "2026-10-23T15:00:00Z" }, "America/Toronto")).toBe("editor · expires Oct 23");
    expect(locationLine({ location_label: "Toronto, ON", location_precision: "about_1_km" })).toBe("Toronto, ON · about 1 km");
    expect(locationLine({ location_label: "Toronto, ON", location_precision: "exact" })).toBe("Toronto, ON");
    expect(acceptProblem(404)).toBe("This invite isn't valid any more. Ask for a new one.");
    expect(acceptProblem(429)).toBe("Too many attempts. Try again in a minute.");
  });
});
