// Settings over the mock (screens 3.9; flows 13): the gardens with Manage or Members by role, switching the active
// garden, the location toggle asking first, and deleting the account with the typed word, then signing out; a
// sign-out that throws after the deletion still says it is deleted and clears this phone.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { Alert } from "react-native";

import { setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { useActiveGardenStore } from "@/features/gardens";
import { ThemeProvider } from "@/ui";

import { DeleteAccountSheet } from "../screens/DeleteAccountSheet";
import { SettingsScreen } from "../screens/SettingsScreen";

const mockRouter = { push: jest.fn(), back: jest.fn(), navigate: jest.fn(), dismissTo: jest.fn() };
jest.mock("expo-router", () => ({ useRouter: () => mockRouter }));

const mockSignOut = jest.fn(async () => {});
jest.mock("@/features/auth", () => ({ useSession: () => ({ mode: "demo", isSignedIn: true, signOut: mockSignOut }) }));

type AlertButton = { text?: string; onPress?: () => void };

beforeEach(() => {
  resetMock();
  setTransport(mockTransport);
  jest.clearAllMocks();
  useActiveGardenStore.setState({ gardenId: IDS.garden });
});

function wrap(node: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        {node}
      </ThemeProvider>
    </QueryClientProvider>
  );
}

describe("Settings", () => {
  it("lists the gardens with Manage for an owner, and switches the active one", async () => {
    mockStore.gardens.set("garden_allot", { ...mockStore.gardens.get(IDS.garden)!, id: "garden_allot", name: "Allotment" });
    mockStore.people.members.set("garden_allot", [{ user_id: IDS.user, display_name: "Alex Doe", role: "editor", since: "2026-10-16T12:00:00Z" }]);
    await render(wrap(<SettingsScreen />));
    expect(await screen.findByTestId(`garden-manage-${IDS.garden}`)).toBeTruthy();
    expect(screen.getByTestId("garden-members-garden_allot")).toBeTruthy();
    await fireEvent.press(screen.getByText("Allotment"));
    expect(useActiveGardenStore.getState().gardenId).toBe("garden_allot");
    expect(mockRouter.navigate).toHaveBeenCalledWith("/");
  });

  it("asks before it stops sharing the location, then rounds owned gardens", async () => {
    const alert = jest.spyOn(Alert, "alert");
    await render(wrap(<SettingsScreen />));
    await waitFor(() => expect(screen.getByTestId("settings-consent").props.value).toBe(true));
    await fireEvent(screen.getByTestId("settings-consent"), "valueChange", false);
    expect(alert.mock.calls[0]![1]).toMatch(/about 10 km/);
    (alert.mock.calls[0]![2] as AlertButton[]).find((b) => b.text === "Turn off")!.onPress!();
    await waitFor(() => expect(mockStore.user.location_consent).toBe(false));
  });

  it("is not reached by a viewer's Manage", async () => {
    scenario.set({ role: "viewer" });
    await render(wrap(<SettingsScreen />));
    expect(await screen.findByTestId(`garden-members-${IDS.garden}`)).toBeTruthy();
    expect(screen.queryByTestId(`garden-manage-${IDS.garden}`)).toBeNull();
  });
});

describe("deleting the account", () => {
  it("needs the word, deletes, signs out and names the garden's new owner", async () => {
    const alert = jest.spyOn(Alert, "alert");
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
    // The members list was seen before, so the summary can name the heir.
    qc.setQueryData(["members", IDS.garden], mockStore.people.members.get(IDS.garden));
    await render(
      <QueryClientProvider client={qc}>
        <ThemeProvider scheme="light" reducedMotion>
          <DeleteAccountSheet />
        </ThemeProvider>
      </QueryClientProvider>,
    );
    expect(screen.getByTestId("account-delete-confirm").props.accessibilityState?.disabled).toBe(true);
    await fireEvent.changeText(screen.getByTestId("account-delete-word"), "delete");
    await waitFor(() => expect(screen.getByTestId("account-delete-confirm").props.accessibilityState?.disabled).toBe(false));
    await fireEvent.press(screen.getByTestId("account-delete-confirm"));
    await waitFor(() => expect(mockSignOut).toHaveBeenCalled());
    expect(alert).toHaveBeenCalledWith("Your account is deleted", "Back garden is now owned by Sam Lee. The rest finishes in the background.");
  });

  it("says the account is deleted and clears this phone when sign-out throws after DELETE /me", async () => {
    mockSignOut.mockRejectedValueOnce(new Error("Clerk is offline"));
    const alert = jest.spyOn(Alert, "alert");
    await render(wrap(<DeleteAccountSheet />));
    await fireEvent.changeText(screen.getByTestId("account-delete-word"), "delete");
    await fireEvent.press(screen.getByTestId("account-delete-confirm"));
    await waitFor(() => expect(alert).toHaveBeenCalledWith("Your account is deleted", expect.stringMatching(/finishes in the background/)));
    expect(screen.queryByText(/wasn't deleted/)).toBeNull();
    expect(useActiveGardenStore.getState().gardenId).toBeNull();
  });
});
