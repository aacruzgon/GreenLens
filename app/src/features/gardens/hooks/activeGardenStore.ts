import { create } from "zustand";
import { persist } from "zustand/middleware";

import { onSignOut } from "@/features/auth";
import { persistStorage } from "@/platform";

export const useActiveGardenStore = create<{ gardenId: string | null; setGardenId: (id: string | null) => void }>()(
  persist((set) => ({ gardenId: null, setGardenId: (gardenId) => set({ gardenId }) }), { name: "pdm-active-garden", storage: persistStorage }),
);

onSignOut(() => useActiveGardenStore.getState().setGardenId(null));
