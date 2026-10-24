// Members and invites over the mock's demo garden (screens 3.9; flows 3): owner controls against a viewer's view,
// an invite made and shared through the system share sheet, and the last owner kept from leaving.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { Alert, Share } from "react-native";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { MembersScreen } from "../screens/MembersScreen";

const mockRouter = { push: jest.fn(), back: jest.fn(), dismissTo: jest.fn() };
jest.mock("expo-router", () => ({ useRouter: () => mockRouter }));

type AlertButton = { text?: string; onPress?: () => void };
const press = (alert: jest.SpyInstance, text: string) => {
  const buttons = alert.mock.calls.at(-1)![2] as AlertButton[];
  buttons.find((b) => b.text === text)!.onPress!();
};

beforeEach(() => {
  resetMock();
  setTransport(mockTransport);
  jest.clearAllMocks();
});

async function renderScreen() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <MembersScreen gardenId={IDS.garden} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByText("Sam Lee");
}

describe("the members screen", () => {
  it("gives an owner the role menu, remove and the invites, and no Leave as the last owner", async () => {
    await renderScreen();
    expect(screen.getByText("Alex Doe (you)")).toBeTruthy();
    expect(screen.getByTestId("role-user_sam")).toBeTruthy();
    expect(screen.getByTestId("remove-user_jo")).toBeTruthy();
    expect(screen.queryByTestId("role-user_alex")).toBeNull();
    expect(await screen.findByText(/editor · expires/)).toBeTruthy();
    expect(screen.queryByTestId("leave-garden")).toBeNull();
    expect(screen.getByText(/You're the only owner/)).toBeTruthy();
  });

  it("shows a viewer the members only, and lets them leave after asking", async () => {
    scenario.set({ role: "viewer" });
    // A second owner, so the demo user (now a viewer) isn't holding the garden alone.
    mockStore.people.members.get(IDS.garden)![1]!.role = "owner";
    await renderScreen();
    expect(screen.queryByTestId("role-user_sam")).toBeNull();
    expect(screen.queryByTestId("invite-add")).toBeNull();
    expect(screen.queryByText("Invites")).toBeNull();
    const alert = jest.spyOn(Alert, "alert");
    await fireEvent.press(screen.getByTestId("leave-garden"));
    expect(alert.mock.calls[0]![1]).toBe("Your private chats in this garden will be deleted. Your scans stay with the garden.");
    press(alert, "Leave");
    await waitFor(() => expect(mockRouter.dismissTo).toHaveBeenCalledWith("/"));
    expect(mockStore.people.members.get(IDS.garden)!.some((m) => m.user_id === IDS.user)).toBe(false);
  });

  it("makes an invite for the chosen role and opens the share sheet with its link", async () => {
    const share = jest.spyOn(Share, "share").mockResolvedValue({ action: "sharedAction" });
    const alert = jest.spyOn(Alert, "alert");
    await renderScreen();
    await fireEvent.press(screen.getByTestId("invite-add"));
    press(alert, "Viewer");
    await waitFor(() => expect(share).toHaveBeenCalled());
    const made = mockStore.people.invites.at(-1)!.invite;
    expect(made.role).toBe("viewer");
    expect(share.mock.calls[0]![0]).toEqual({ message: `Join "Back garden" on PDM as a viewer: pdm://invite/${made.code}` });
    expect(await screen.findAllByText(/viewer · expires/)).toHaveLength(1);
  });

  it("asks before removing a member, with screens 3.9's warning", async () => {
    const alert = jest.spyOn(Alert, "alert");
    await renderScreen();
    await fireEvent.press(screen.getByTestId("remove-user_jo"));
    expect(alert.mock.calls[0]![1]).toBe("Their private chats in this garden will be deleted. Their scans stay with the garden.");
    press(alert, "Remove");
    await waitFor(() => expect(screen.queryByText("Jo Park")).toBeNull());
  });
});
