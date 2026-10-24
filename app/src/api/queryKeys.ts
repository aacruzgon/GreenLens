import type { AnchorType, TaskSource, TaskState } from "@/domain/types";

// Every query key, named as the table in docs/frontend/README.md §9 (query keys and invalidation).
// Optional trailing parts are left off when absent, so `qk.tasks(gardenId)` is a prefix of
// `qk.tasks(gardenId, filter)` and invalidates every filtered list of that garden.

export interface TaskFilter {
  source?: TaskSource;
  state?: TaskState;
}

/** The `anchor_type` and `anchor_id` query parameters of `GET /conversations`. */
export interface ConversationAnchor {
  type: AnchorType;
  id: string;
}

function tail<T>(v: T | undefined): [] | [T] {
  return v === undefined ? [] : [v];
}

export const qk = {
  me: () => ["me"] as const,
  gardens: () => ["gardens"] as const,
  garden: (id: string) => ["gardens", id] as const,
  weather: (gardenId: string) => ["weather", gardenId] as const,
  members: (gardenId: string) => ["members", gardenId] as const,
  invites: (gardenId: string) => ["invites", gardenId] as const,
  plants: (gardenId: string) => ["plants", "list", gardenId] as const,
  plant: (id: string) => ["plants", "item", id] as const,
  timeline: (plantId: string) => ["timeline", plantId] as const,
  measurements: (plantId: string) => ["measurements", plantId] as const,
  scan: (id: string) => ["scans", id] as const,
  tasks: (gardenId: string, filter?: TaskFilter) => ["tasks", gardenId, ...tail(filter)] as const,
  species: (q?: string) => ["species", "list", q ?? ""] as const,
  speciesItem: (id: string) => ["species", "item", id] as const,
  conversations: (gardenId: string, anchor?: ConversationAnchor) =>
    ["conversations", "list", gardenId, ...tail(anchor)] as const,
  conversation: (id: string) => ["conversations", "item", id] as const,
  memories: (gardenId: string, plantId?: string) => ["memories", gardenId, ...tail(plantId)] as const,
  map: (gardenId: string) => ["map", gardenId] as const,
  connectors: (gardenId: string) => ["connectors", gardenId] as const,
  connectorSources: (connectorId: string) => ["connectorSources", connectorId] as const,
};
