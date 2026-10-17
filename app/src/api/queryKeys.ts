// docs/frontend 9, query keys: every key comes from here so invalidation rules can name them.
export const qk = {
  me: () => ["me"] as const,
  gardens: () => ["gardens"] as const,
  garden: (id: string) => ["gardens", id] as const,
  weather: (gardenId: string) => ["weather", gardenId] as const,
  plants: (gardenId: string) => ["plants", gardenId] as const,
  plant: (id: string) => ["plant", id] as const,
  timeline: (plantId: string) => ["timeline", plantId] as const,
  measurements: (plantId: string) => ["measurements", plantId] as const,
  scan: (id: string) => ["scan", id] as const,
  tasks: (gardenId: string, filter?: Record<string, string>) =>
    (filter ? (["tasks", gardenId, filter] as const) : (["tasks", gardenId] as const)),
  memories: (gardenId: string, plantId?: string) =>
    (plantId ? (["memories", gardenId, plantId] as const) : (["memories", gardenId] as const)),
  species: (q: string) => ["species", q] as const,
  speciesItem: (id: string) => ["species-item", id] as const,
  members: (gardenId: string) => ["members", gardenId] as const,
  map: (gardenId: string) => ["map", gardenId] as const,
};
