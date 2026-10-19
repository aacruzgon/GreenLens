import { ClerkLoaded, ClerkProvider, useAuth, useUser } from "@clerk/expo";
import { tokenCache } from "@clerk/expo/token-cache";
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import { clearTokenProvider, setOnSignedOut, setTokenProvider } from "@/api";

// The app's view of the session (docs/frontend/README.md §8). With a Clerk key it is Clerk's session;
// in mock mode with no key it is a local demo user, so the app can be shown with no backend and no account.

export type SessionMode = "clerk" | "demo";

export interface Session {
  mode: SessionMode;
  isLoaded: boolean;
  isSignedIn: boolean;
  /** Clerk's full name or username, for the first PATCH /me; null in demo mode. */
  profileName: string | null;
  signInDemo: () => void;
  signOut: () => Promise<void>;
}

const SessionContext = createContext<Session | null>(null);

export function useSession(): Session {
  const session = useContext(SessionContext);
  if (!session) throw new Error("useSession is used outside SessionProvider");
  return session;
}

export function SessionProvider({ publishableKey, children }: { publishableKey: string; children: ReactNode }) {
  if (!publishableKey) return <DemoSession>{children}</DemoSession>;
  return (
    <ClerkProvider publishableKey={publishableKey} tokenCache={tokenCache}>
      <ClerkLoaded>
        <ClerkSession>{children}</ClerkSession>
      </ClerkLoaded>
    </ClerkProvider>
  );
}

function ClerkSession({ children }: { children: ReactNode }) {
  const { isLoaded, isSignedIn, getToken, signOut } = useAuth();
  const { user } = useUser();

  // AuthBridge: the api layer never imports Clerk, so the session hands it a token function.
  useEffect(() => {
    if (!isSignedIn) {
      clearTokenProvider();
      return;
    }
    setTokenProvider((opts) => getToken(opts?.skipCache ? { skipCache: true } : undefined));
    setOnSignedOut(() => void signOut());
    return () => clearTokenProvider();
  }, [isSignedIn, getToken, signOut]);

  const value = useMemo<Session>(
    () => ({
      mode: "clerk",
      isLoaded,
      isSignedIn: Boolean(isSignedIn),
      profileName: user?.fullName || user?.username || null,
      signInDemo: () => {},
      signOut: async () => {
        await signOut();
      },
    }),
    [isLoaded, isSignedIn, user?.fullName, user?.username, signOut],
  );
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

function DemoSession({ children }: { children: ReactNode }) {
  const [isSignedIn, setSignedIn] = useState(false);

  useEffect(() => {
    if (!isSignedIn) {
      clearTokenProvider();
      return;
    }
    setTokenProvider(async () => "demo-token");
    setOnSignedOut(() => setSignedIn(false));
  }, [isSignedIn]);

  const value = useMemo<Session>(
    () => ({
      mode: "demo",
      isLoaded: true,
      isSignedIn,
      profileName: null,
      signInDemo: () => setSignedIn(true),
      signOut: async () => setSignedIn(false),
    }),
    [isSignedIn],
  );
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}
