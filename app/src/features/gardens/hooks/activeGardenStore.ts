import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import type { Id } from "@/domain/types";
import { storage } from "@/platform";

// Client state that isn't on the server (docs/frontend/README.md §12): the garden the user chose last.
interface ActiveGardenState {
  gardenId: Id | null;
  setGardenId: (id: Id | null) => void;
}

export const useActiveGardenStore = create<ActiveGardenState>()(
  persist((set) => ({ gardenId: null, setGardenId: (gardenId) => set({ gardenId }) }), {
    name: "pdm.active-garden",
    storage: createJSONStorage(() => ({ getItem: storage.get, setItem: storage.set, removeItem: storage.remove })),
  }),
);
