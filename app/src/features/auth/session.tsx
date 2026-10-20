import { ClerkLoaded, ClerkProvider, useAuth, useUser } from "@clerk/expo";
import { tokenCache } from "@clerk/expo/token-cache";
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

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

interface SessionProviderProps {
  publishableKey: string;
  /** Runs whenever a signed-in session ends, by any path: Settings, a second 401, or Clerk ending it (README 8). */
  onSignedOut: () => void;
  children: ReactNode;
}

export function SessionProvider({ publishableKey, onSignedOut, children }: SessionProviderProps) {
  if (!publishableKey) return <DemoSession onSignedOut={onSignedOut}>{children}</DemoSession>;
  return (
    <ClerkProvider publishableKey={publishableKey} tokenCache={tokenCache}>
      <ClerkLoaded>
        <ClerkSession onSignedOut={onSignedOut}>{children}</ClerkSession>
      </ClerkLoaded>
    </ClerkProvider>
  );
}

// Calls onSignedOut when isSignedIn goes from true to false.
function useSignOutCleanup(isSignedIn: boolean, onSignedOut: () => void) {
  const was = useRef(isSignedIn);
  useEffect(() => {
    if (was.current && !isSignedIn) onSignedOut();
    was.current = isSignedIn;
  }, [isSignedIn, onSignedOut]);
}

function ClerkSession({ onSignedOut, children }: { onSignedOut: () => void; children: ReactNode }) {
  const { isLoaded, isSignedIn, getToken, signOut } = useAuth();
  const { user } = useUser();
  useSignOutCleanup(Boolean(isSignedIn), onSignedOut);

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

  // Two-step sign-in by text: the phone verified at sign-up becomes the account's second factor,
  // so every later sign-in asks for a code by SMS (Clerk's phone_code second factor).
  const reserving = useRef(false);
  useEffect(() => {
    if (!user || user.twoFactorEnabled || reserving.current) return;
    const phone = user.phoneNumbers.find((p) => p.verification?.status === "verified");
    if (!phone || phone.reservedForSecondFactor) return;
    reserving.current = true;
    phone
      .setReservedForSecondFactor({ reserved: true })
      .catch(() => {})
      .finally(() => {
        reserving.current = false;
      });
  }, [user]);

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

function DemoSession({ onSignedOut, children }: { onSignedOut: () => void; children: ReactNode }) {
  const [isSignedIn, setSignedIn] = useState(false);
  useSignOutCleanup(isSignedIn, onSignedOut);

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
