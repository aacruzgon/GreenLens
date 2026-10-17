import { createContext, useCallback, useContext, useEffect, useMemo, type ReactNode } from "react";
import { ClerkLoaded, ClerkProvider, useAuth, useUser } from "@clerk/expo";
import { tokenCache } from "@clerk/expo/token-cache";
import { useQueryClient } from "@tanstack/react-query";
import { create } from "zustand";
import { persist } from "zustand/middleware";

import { setTokenProvider } from "@/api";
import { env } from "@/config/env";
import { persistStorage } from "@/platform";

// docs/frontend 7 and 8. Screens ask useSession(); only this file knows whether Clerk or the
// demo sign-in is behind it. The demo sign-in exists only in mock mode without a Clerk key.

export interface Session {
  mode: "clerk" | "demo";
  isLoaded: boolean;
  isSignedIn: boolean;
  /** A name to suggest for PATCH /me: Clerk's full name or username. */
  suggestedName?: string;
  email?: string;
  signOut: () => Promise<void>;
}

const SessionContext = createContext<Session | null>(null);

export function useSession(): Session {
  const s = useContext(SessionContext);
  if (!s) throw new Error("useSession outside SessionProvider");
  return s;
}

/** Sign-out clears every device-local cache (docs/frontend 8, session lifecycle). */
const signOutHooks = new Set<() => void>();
export function onSignOut(fn: () => void) {
  signOutHooks.add(fn);
  return () => signOutHooks.delete(fn);
}

export function SessionProvider({ children }: { children: ReactNode }) {
  if (env.demoAuth) return <DemoSession>{children}</DemoSession>;
  return (
    <ClerkProvider publishableKey={env.clerkPublishableKey} tokenCache={tokenCache}>
      <ClerkLoaded>
        <ClerkSession>{children}</ClerkSession>
      </ClerkLoaded>
    </ClerkProvider>
  );
}

function useClearOnSignOut() {
  const queryClient = useQueryClient();
  return useCallback(() => {
    queryClient.clear();
    signOutHooks.forEach((fn) => fn());
  }, [queryClient]);
}

function ClerkSession({ children }: { children: ReactNode }) {
  const { isLoaded, isSignedIn, getToken, signOut } = useAuth();
  const { user } = useUser();
  const clear = useClearOnSignOut();

  const doSignOut = useCallback(async () => {
    clear();
    await signOut();
  }, [clear, signOut]);

  // AuthBridge: hands getToken to the api layer, and clears it on sign-out.
  useEffect(() => {
    if (isSignedIn) setTokenProvider((opts) => getToken(opts), () => void doSignOut());
    else setTokenProvider(null);
  }, [isSignedIn, getToken, doSignOut]);

  const value = useMemo<Session>(
    () => ({
      mode: "clerk",
      isLoaded,
      isSignedIn: !!isSignedIn,
      suggestedName: user?.fullName ?? user?.username ?? undefined,
      email: user?.primaryEmailAddress?.emailAddress,
      signOut: doSignOut,
    }),
    [isLoaded, isSignedIn, user, doSignOut],
  );
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

const useDemoAuth = create<{ signedIn: boolean; name: string; set: (signedIn: boolean, name?: string) => void }>()(
  persist((set) => ({ signedIn: false, name: "", set: (signedIn, name = "") => set({ signedIn, name }) }), {
    name: "pdm-demo-auth",
    storage: persistStorage,
  }),
);

export function useDemoSignIn() {
  return useDemoAuth((s) => s.set);
}

function DemoSession({ children }: { children: ReactNode }) {
  const { signedIn, name, set } = useDemoAuth();
  const hydrated = useDemoAuth.persist.hasHydrated();
  const clear = useClearOnSignOut();

  useEffect(() => {
    setTokenProvider(signedIn ? async () => "demo-token" : null);
  }, [signedIn]);

  const value = useMemo<Session>(
    () => ({
      mode: "demo",
      isLoaded: hydrated,
      isSignedIn: signedIn,
      suggestedName: name || undefined,
      email: "demo@pdm.garden",
      signOut: async () => {
        clear();
        set(false);
      },
    }),
    [hydrated, signedIn, name, clear, set],
  );
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}
