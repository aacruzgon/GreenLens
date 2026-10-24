import { useRouter } from "expo-router";
import { useEffect } from "react";
import { create } from "zustand";

import { storage } from "@/platform";

// An invite opened while signed out (flows 3: "sign-in, then back here"): kept across sign-in and sign-up, which can
// leave the app for an email code, and accepted by the account that signs in next. In memory only, so a code is never
// accepted at a later launch, perhaps by another account; sign-out (shell, clearSignedOutState) and any final answer
// of the accept (useAcceptInviteScreen) clear it.
interface PendingInviteState {
  code: string | null;
  setCode: (code: string | null) => void;
}

export const usePendingInviteStore = create<PendingInviteState>()((set) => ({ code: null, setCode: (code) => set({ code }) }));

// Builds before this one persisted the code under this key; drop what they left.
void Promise.resolve()
  .then(() => storage.remove("pdm.pending-invite"))
  .catch(() => undefined);

/** For sign-out: no code stays for the next account. */
export function clearPendingInvite(): void {
  usePendingInviteStore.getState().setCode(null);
}

/** In the signed-in layout: opens a kept invite once the user is signed in, before the session gate decides. */
export function usePendingInvite(signedIn: boolean): void {
  const router = useRouter();
  const code = usePendingInviteStore((s) => s.code);
  useEffect(() => {
    if (signedIn && code) router.push({ pathname: "/invite/[code]", params: { code } });
  }, [signedIn, code, router]);
}
