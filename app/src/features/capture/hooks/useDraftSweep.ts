import { useEffect } from "react";

import { isApiError, scans } from "@/api";

import { useDraftStore } from "./draftStore";

// Capture checks only the draft of the scan it opens, so a draft whose scan was submitted or discarded elsewhere,
// or one left by an earlier mock launch, would stay with its photos until sign-out. Once per launch, after
// sign-in, every saved draft's scan is fetched, and a draft whose scan is gone or no longer pending is dropped.
// A scan that can't be fetched for another reason (offline) keeps its draft.
let swept = false;

export function useDraftSweep(enabled: boolean) {
  useEffect(() => {
    if (!enabled || swept) return;
    swept = true;
    void sweepDrafts();
  }, [enabled]);
}

export async function sweepDrafts(): Promise<void> {
  const { persist } = useDraftStore;
  if (!persist.hasHydrated()) await new Promise<void>((resolve) => persist.onFinishHydration(() => resolve()));
  const ids = Object.keys(useDraftStore.getState().drafts);
  await Promise.all(
    ids.map(async (id) => {
      try {
        if ((await scans.get(id)).status === "pending") return;
      } catch (error) {
        if (!(isApiError(error) && error.status === 404)) return;
      }
      if (useDraftStore.getState().drafts[id]) useDraftStore.getState().drop(id);
    }),
  );
}
