// invite/[code] (flows 3): signed out the code is kept for after sign-in; signed in it is accepted and the garden
// becomes active, the exit check's "an invite accepted on a second account lands on the garden"; a dead code says so.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react-native";

import { setTransport } from "@/api";
import { chooseAccount, DEMO_INVITE_CODE, IDS, mockStore, mockTransport, resetMock } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { useActiveGardenStore } from "../hooks/activeGardenStore";
import { usePendingInviteStore } from "../hooks/pendingInviteStore";
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

  it("says when a link has no code", async () => {
    await renderScreen("pdm://invite/");
    expect(screen.getByTestId("invite-problem")).toBeTruthy();
  });
});
