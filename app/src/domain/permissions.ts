import type { Role } from "./types";

// The role matrix of contracts 1 as data (docs/frontend 9, roles in the UI).
// The server enforces every rule; this only hides actions a role can't use.
export type Action =
  | "scan.create"
  | "scan.delete_pending"
  | "scan.answer"
  | "scan.feedback"
  | "species.confirm"
  | "map.capture"
  | "task.write"
  | "plant.write"
  | "plant.delete"
  | "memory.note"
  | "memory.share"
  | "memory.delete_shared"
  | "invites.manage"
  | "members.manage"
  | "garden.manage";

const rank: Record<Role, number> = { viewer: 0, reviewer: 1, editor: 2, owner: 3 };

const minimum: Record<Action, Role> = {
  "scan.feedback": "reviewer",
  "scan.create": "editor",
  "scan.delete_pending": "editor",
  "scan.answer": "editor",
  "species.confirm": "editor",
  "map.capture": "editor",
  "task.write": "editor",
  "plant.write": "editor",
  "memory.note": "editor",
  "memory.share": "editor",
  "plant.delete": "owner",
  "memory.delete_shared": "owner",
  "invites.manage": "owner",
  "members.manage": "owner",
  "garden.manage": "owner",
};

export function can(role: Role | undefined, action: Action): boolean {
  if (!role) return false;
  return rank[role] >= rank[minimum[action]];
}

export function atLeast(role: Role, other: Role): boolean {
  return rank[role] >= rank[other];
}
