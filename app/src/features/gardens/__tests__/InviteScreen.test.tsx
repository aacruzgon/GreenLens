// invite/[code] (flows 3): signed out the code is kept for after sign-in; signed in it is accepted and the garden
// becomes active, the exit check's "an invite accepted on a second account lands on the garden"; a dead code says so.
// Every final answer drops the kept code, so nothing re-opens the invite at the next sign-in.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, renderHook, screen, waitFor } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { chooseAccount, DEMO_INVITE_CODE, IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { useActiveGardenStore } from "../hooks/activeGardenStore";
import { usePendingInvite, usePendingInviteStore } from "../hooks/pendingInviteStore";
import { InviteScreen } from "../screens/InviteScreen";

const mockRouter = { push: jest.fn(), back: jest.fn(), dismissTo: jest.fn() };
jest.mock("expo-router", () => ({
  useRouter: () => mockRouter,
  Redirect: ({ href }: { href: string }) => {
    const { Text: T } = jest.requireActual<typeof import("react-native")>("react-native");
    return <T>{`redirect ${href}`}</T>;
  },
}));

let mockSignedIn = true;
jest.mock("@/features/auth", () => ({ useSession: () => ({ isSignedIn: mockSignedIn }) }));

beforeEach(() => {
  resetMock();
  setTransport(mockTransport);
  jest.clearAllMocks();
  mockSignedIn = true;
  usePendingInviteStore.setState({ code: null });
  useActiveGardenStore.setState({ gardenId: null });
});

async function renderScreen(code: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <InviteScreen code={code} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

describe("the invite screen", () => {
  it("keeps the code and sends a signed-out user to sign in", async () => {
    mockSignedIn = false;
    await renderScreen(`pdm://invite/${DEMO_INVITE_CODE.toLowerCase()}`);
    expect(screen.getByText("redirect /sign-in")).toBeTruthy();
    expect(usePendingInviteStore.getState().code).toBe(DEMO_INVITE_CODE);
  });

  it("joins a second account to the garden and opens it", async () => {
    chooseAccount("guest");
    usePendingInviteStore.setState({ code: DEMO_INVITE_CODE });
    await renderScreen(DEMO_INVITE_CODE);
    await waitFor(() => expect(mockRouter.dismissTo).toHaveBeenCalledWith("/"));
    expect(useActiveGardenStore.getState().gardenId).toBe(IDS.garden);
    expect(usePendingInviteStore.getState().code).toBeNull();
    expect(mockStore.people.members.get(IDS.garden)!.at(-1)).toMatchObject({ display_name: "Riley Chen", role: "editor" });
  });

  it("says a used or expired invite isn't valid, with no retry", async () => {
    chooseAccount("guest");
    mockStore.people.invites[0]!.used = true;
    await renderScreen(DEMO_INVITE_CODE);
    expect(await screen.findByText("This invite isn't valid any more. Ask for a new one.")).toBeTruthy();
    expect(screen.queryByText("Try again")).toBeNull();
    expect(useActiveGardenStore.getState().gardenId).toBeNull();
  });

  it("drops the kept code after a 5xx, offers a retry here, and doesn't open the invite again", async () => {
    chooseAccount("guest");
    usePendingInviteStore.setState({ code: DEMO_INVITE_CODE });
    setTransport({
      fetch: async (input, init) =>
        String(input instanceof Request ? input.url : input).includes("/accept")
          ? new Response(JSON.stringify({ error: { code: "internal", message: "x", retryable: true, request_id: "r" } }), {
              status: 500,
              headers: { "content-type": "application/json" },
            })
          : mockTransport.fetch(input, init),
    });
    await renderScreen(DEMO_INVITE_CODE);
    expect(await screen.findByText("The invite didn't go through. Check your connection and try again.")).toBeTruthy();
    expect(screen.getByText("Try again")).toBeTruthy();
    expect(usePendingInviteStore.getState().code).toBeNull();
    await renderHook(() => usePendingInvite(true));
    expect(mockRouter.push).not.toHaveBeenCalled();
  });

  it("drops the kept code after a 429", async () => {
    chooseAccount("guest");
    usePendingInviteStore.setState({ code: DEMO_INVITE_CODE });
    scenario.set({ callFault: "rate_limited" });
    await renderScreen(DEMO_INVITE_CODE);
    expect(await screen.findByText("Too many attempts. Try again in a minute.")).toBeTruthy();
    expect(usePendingInviteStore.getState().code).toBeNull();
  });

  it("says when a link has no code", async () => {
    await renderScreen("pdm://invite/");
    expect(screen.getByTestId("invite-problem")).toBeTruthy();
  });
});
