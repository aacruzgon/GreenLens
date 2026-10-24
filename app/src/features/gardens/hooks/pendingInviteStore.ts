import { useRouter } from "expo-router";
import { useEffect } from "react";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import { storage } from "@/platform";

// An invite opened while signed out (flows 3): kept across sign-in and sign-up, which can leave the app for an
// email code, and accepted once the user is signed in. Persisted, so it survives the app being closed meanwhile.
interface PendingInviteState {
  code: string | null;
  setCode: (code: string | null) => void;
}

export const usePendingInviteStore = create<PendingInviteState>()(
  persist((set) => ({ code: null, setCode: (code) => set({ code }) }), {
    name: "pdm.pending-invite",
    storage: createJSONStorage(() => ({ getItem: storage.get, setItem: storage.set, removeItem: storage.remove })),
  }),
);

/** In the signed-in layout: opens a kept invite once the user is signed in, before the session gate decides. */
export function usePendingInvite(signedIn: boolean): void {
  const router = useRouter();
  const code = usePendingInviteStore((s) => s.code);
  useEffect(() => {
    if (signedIn && code) router.push({ pathname: "/invite/[code]", params: { code } });
  }, [signedIn, code, router]);
}
