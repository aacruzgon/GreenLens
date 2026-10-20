// The role matrix of contracts 1 as data (docs/frontend/README.md §9, "Roles in the UI").
// The server enforces every rule; this only hides actions that would fail.
import type { Role } from "./types";

export const ROLES = ["viewer", "reviewer", "editor", "owner"] as const satisfies readonly Role[];

/** Rank for "at least this role". Reviewer sits above viewer and below editor. */
const RANK: Record<Role, number> = { viewer: 0, reviewer: 1, editor: 2, owner: 3 };

/** The lowest role allowed to do each action (contracts 1). */
export const REQUIRED_ROLE = {
  // Every role
  "garden.read": "viewer",
  "chat.read": "viewer",
  "memory.remember": "viewer",
  "connectors.read": "viewer",
  // Reviewer and up
  "scan.feedback": "reviewer",
  // Editor and up
  "scan.create": "editor",
  "scan.upload": "editor",
  "scan.delete_pending": "editor",
  "scan.answer": "editor",
  "species.confirm": "editor",
  "scan.guided_check": "editor",
  "map.capture": "editor",
  "chat.write_tools": "editor",
  "task.write": "editor",
  "plant.write": "editor",
  "memory.share": "editor",
  "memory.note": "editor",
  // Owner only
  "garden.manage": "owner",
  "plant.delete": "owner",
  "memory.delete_shared": "owner",
  "invites.manage": "owner",
  "members.manage": "owner",
  "connectors.manage": "owner",
} as const satisfies Record<string, Role>;

export type Action = keyof typeof REQUIRED_ROLE;

export const ACTIONS = Object.keys(REQUIRED_ROLE) as Action[];

/** Whether `role` may do `action`. An unknown role (not a member) may do nothing. */
export function can(role: Role | undefined, action: Action): boolean {
  if (role === undefined) return false;
  return RANK[role] >= RANK[REQUIRED_ROLE[action]];
}
