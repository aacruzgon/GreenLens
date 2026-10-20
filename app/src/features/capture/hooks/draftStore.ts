import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import { files, storage } from "@/platform";

import { relocateDraft, resumeDraft, type CaptureDraft } from "../model/draft";

// Capture drafts (docs/frontend/README.md 12: zustand, persisted), one per pending scan, so a capture survives
// the app closing (flows 15, suspended). Rehydrated drafts are resumed: an upload that was running is sent
// again under its key, and each photo's URI is rebuilt from its name, since the app's folder can move between
// launches. Signing out clears them with their photos.
interface DraftState {
  drafts: Record<string, CaptureDraft>;
  put: (draft: CaptureDraft) => void;
  update: (scanId: string, change: (draft: CaptureDraft) => CaptureDraft) => void;
  /** Drops a draft and deletes its photos: after submit or discard. */
  drop: (scanId: string) => void;
  clear: () => void;
}

export const useDraftStore = create<DraftState>()(
  persist(
    (set) => ({
      drafts: {},
      put: (draft) => set((s) => ({ drafts: { ...s.drafts, [draft.scanId]: draft } })),
      update: (scanId, change) =>
        set((s) => {
          const draft = s.drafts[scanId];
          return draft ? { drafts: { ...s.drafts, [scanId]: change(draft) } } : s;
        }),
      drop: (scanId) => {
        // Every kept photo is in the scan's folder (platform/files keep).
        files.drop(scanId);
        set((s) => {
          const { [scanId]: _gone, ...rest } = s.drafts;
          return { drafts: rest };
        });
      },
      clear: () => {
        files.dropAll();
        set({ drafts: {} });
      },
    }),
    {
      name: "pdm.capture-drafts",
      storage: createJSONStorage(() => ({ getItem: storage.get, setItem: storage.set, removeItem: storage.remove })),
      partialize: (s) => ({ drafts: s.drafts }),
      merge: (persisted, current) => {
        const drafts = (persisted as Partial<DraftState> | undefined)?.drafts ?? {};
        return { ...current, drafts: Object.fromEntries(Object.entries(drafts).map(([id, d]) => [id, relocateDraft(resumeDraft(d), (name) => files.locate(d.scanId, name))])) };
      },
    },
  ),
);
