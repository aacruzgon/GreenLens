import { formatMonthDay } from "@/domain/format";
import type { Garden, Invite, LocationPrecision, Member, Role } from "@/domain/types";

// Members, invites and the garden's location as the garden tools show them (screens 3.9; flows 3; contracts 9).

/** The app's scheme (app.config.ts) and route; flows 3 shares `pdm://invite/<code>`. */
export function inviteLink(code: string): string {
  return `pdm://invite/${code}`;
}

/** The share sheet's text: one line and the link. */
export function inviteMessage(gardenName: string, invite: Pick<Invite, "code" | "role">): string {
  return `Join "${gardenName}" on PDM as ${articled(invite.role)}: ${inviteLink(invite.code)}`;
}

function articled(role: Role): string {
  return role === "owner" || role === "editor" ? `an ${role}` : `a ${role}`;
}

/** What each role may do, for the invite's role choice. */
export const ROLE_HINT: Record<Role, string> = {
  owner: "Manages the garden, members and invites",
  editor: "Scans, changes plants and tasks",
  reviewer: "Reads everything and rates results",
  viewer: "Reads everything",
};

/** Most useful first, for the invite choice and the role menu. */
export const ROLE_ORDER: readonly Role[] = ["editor", "viewer", "reviewer", "owner"];

export function roleName(role: Role): string {
  return role[0]!.toUpperCase() + role.slice(1);
}

/** The asking-first text of screens 3.9, for removing a member or leaving. */
export const REMOVE_WARNING = "Their private chats in this garden will be deleted. Their scans stay with the garden.";
export const LEAVE_WARNING = "Your private chats in this garden will be deleted. Your scans stay with the garden.";

export function ownerCount(members: readonly Pick<Member, "role">[]): number {
  return members.filter((m) => m.role === "owner").length;
}

/** The last owner can't leave or be demoted (contracts 9); the screen hides those actions (flows 3). */
export function isLastOwner(members: readonly Pick<Member, "user_id" | "role">[], userId: string): boolean {
  const m = members.find((x) => x.user_id === userId);
  return m?.role === "owner" && ownerCount(members) === 1;
}

export interface MemberRow {
  member: Member;
  isMe: boolean;
  /** "owner · since Oct 16". */
  subtitle: string;
  /** Owner controls: the role menu and remove, never on yourself (you leave instead) or on the last owner. */
  canManage: boolean;
}

export function memberRows(members: readonly Member[], meId: string, myRole: Role | undefined, timeZone: string): MemberRow[] {
  return members.map((member) => {
    const isMe = member.user_id === meId;
    return {
      member,
      isMe,
      subtitle: `${member.role} · since ${formatMonthDay(member.since, timeZone)}`,
      canManage: myRole === "owner" && !isMe && !isLastOwner(members, member.user_id),
    };
  });
}

export function canLeave(members: readonly Pick<Member, "user_id" | "role">[], meId: string): boolean {
  return members.some((m) => m.user_id === meId) && !isLastOwner(members, meId);
}

/** "editor · expires Oct 23". */
export function inviteSubtitle(invite: Pick<Invite, "role" | "expires_at">, timeZone: string): string {
  return `${invite.role} · expires ${formatMonthDay(invite.expires_at, timeZone)}`;
}

const PRECISION: Record<LocationPrecision, string | null> = {
  exact: null,
  about_1_km: "about 1 km",
  about_10_km: "about 10 km",
  postal_code: "by postal code",
  city: "by city",
};

/** "Toronto, ON · about 1 km" (screens 3.9). */
export function locationLine(garden: Pick<Garden, "location_label" | "location_precision">): string {
  const p = PRECISION[garden.location_precision];
  return p ? `${garden.location_label} · ${p}` : garden.location_label;
}

/** What the invite screen says for an accept that failed (flows 3). */
export function acceptProblem(status: number | undefined): string {
  if (status === 404) return "This invite isn't valid any more. Ask for a new one.";
  if (status === 429) return "Too many attempts. Try again in a minute.";
  return "The invite didn't go through. Check your connection and try again.";
}

/** An invite code from a pasted link or the code itself; base32, so case doesn't matter. */
export function parseInviteCode(input: string): string | null {
  const m = /([A-Za-z2-7]{26})\s*$/.exec(input.trim());
  return m ? m[1]!.toUpperCase() : null;
}
