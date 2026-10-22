import { render, screen } from "@testing-library/react-native";
import { useEffect, useState } from "react";
import { Text } from "react-native";

import { apiFetch, setTransport } from "@/api";
import { resetAuthForTests } from "@/api/auth";
import { SessionProvider } from "@/features/auth";

// Like @clerk/expo 4.x: useAuth returns a new getToken on every render, and fetching a token updates
// Clerk's state, which re-renders the components that read it (#97).
const mockClerk = { version: 0, listeners: new Set<() => void>() };
const mockSignOut = jest.fn(async () => {});
jest.mock("@clerk/expo", () => {
  const { useSyncExternalStore } = jest.requireActual("react");
  const passthrough = ({ children }: { children: unknown }) => children;
  const useClerkVersion = () =>
    useSyncExternalStore(
      (l: () => void) => {
        mockClerk.listeners.add(l);
        return () => mockClerk.listeners.delete(l);
      },
      () => mockClerk.version,
    );
  return {
    ClerkProvider: passthrough,
    ClerkLoaded: passthrough,
    useAuth: () => {
      useClerkVersion();
      return {
        isLoaded: true,
        isSignedIn: true,
        getToken: async () => {
          await Promise.resolve();
          mockClerk.version += 1;
          mockClerk.listeners.forEach((l) => l());
          return "tok";
        },
        signOut: mockSignOut,
      };
    },
    useUser: () => ({ user: null }),
  };
});

const sent: (string | null)[] = [];
beforeEach(() => {
  resetAuthForTests();
  sent.length = 0;
  setTransport({
    fetch: async (input) => {
      const auth = typeof input === "string" ? null : input.headers.get("Authorization");
      sent.push(auth);
      return new Response("{}", { status: auth ? 200 : 401 });
    },
  });
});

// Stands in for SessionGate: a child that sends GET /me as soon as it mounts.
function Gate() {
  const [status, setStatus] = useState<number>();
  useEffect(() => {
    void apiFetch(new Request("https://api.test/me")).then((r) => setStatus(r.status));
  }, []);
  return <Text>{status ? `status ${status}` : "loading"}</Text>;
}

it("sends the Clerk token on a child's first request, and keeps the session when Clerk re-renders", async () => {
  await render(
    <SessionProvider publishableKey="pk_test_x" onSignedOut={() => {}}>
      <Gate />
    </SessionProvider>,
  );
  expect(await screen.findByText("status 200")).toBeTruthy();
  expect(sent).toEqual(["Bearer tok"]);
  expect(mockSignOut).not.toHaveBeenCalled();
});
