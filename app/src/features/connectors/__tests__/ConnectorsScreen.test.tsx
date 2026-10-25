// The Farm connectors screen over the mock backend (EON T-E10.1): each state (none, pending, active,
// reauth_needed, deleting, era, connectors off), read-only below owner, and the connect flow (E10.2) end to end with
// the fake consent page: the verifier's hash checked for real, cancel, a declined consent and a used state; the
// garden's cap of 2 connectors, a Reconnect in a full garden, a double tap and a failed cancel.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react-native";
import { Alert } from "react-native";

import { setTransport } from "@/api";
import { IDS, mockAuthSession, mockConnectors, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import type { Connector } from "@/domain/types";
import { authSession } from "@/platform";
import { ThemeProvider } from "@/ui";

import { useConnectFlow } from "../hooks/useConnectFlow";
import { ConnectorsScreen } from "../screens/ConnectorsScreen";

// expo-crypto's digest is a stub under Jest; the real SHA-256 makes the mock's device check mean something.
jest.mock("expo-crypto", () => {
  const actual = jest.requireActual("expo-crypto");
  const { createHash, randomBytes } = jest.requireActual("crypto");
  return {
    ...actual,
    getRandomBytes: (n: number) => new Uint8Array(randomBytes(n)),
    digest: async (_alg: string, data: Uint8Array) => {
      const out = createHash("sha256").update(data).digest();
      return out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength);
    },
  };
});

const mockRouter = { push: jest.fn(), back: jest.fn() };
jest.mock("expo-router", () => ({ useRouter: () => mockRouter }));

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  authSession.setFake(mockAuthSession);
  jest.clearAllMocks();
});

afterAll(() => authSession.setFake(null));

async function renderScreen() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        <ConnectorsScreen gardenId={IDS.garden} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

function vendor(patch: Partial<Connector>): Connector {
  return {
    id: "conn_vendor",
    garden_id: IDS.garden,
    kind: "slack",
    status: "active",
    endpoint: "vendor",
    account: { kind: "slack", team_id: "T0MAPLE", team_name: "Maple Row Farm", url: "https://maple-row-farm.slack.com/" },
    sources: [{ id: "C0FIELD", name: "field-reports" }],
    record_count: 34,
    synced_at: "2026-10-23T07:45:00Z",
    created_at: "2026-10-20T09:00:00Z",
    ...patch,
  };
}

const only = (...cs: Connector[]) => {
  mockStore.chat.connectors = cs;
};
const button = (name: string) => screen.findByRole("button", { name });

describe("the Farm connectors screen", () => {
  it("labels the demo garden's era connectors as synthetic and offers no owner actions on them", async () => {
    await renderScreen();
    expect(await screen.findAllByText("Demo data from Era by Eon (synthetic)")).toHaveLength(2);
    expect(screen.getByText("#field-reports, #crew")).toBeTruthy();
    expect(screen.getByText(/91 messages/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Sync now" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Disconnect" })).toBeNull();
    // The two era connectors fill the garden's 2, as the store counts them.
    expect(screen.getByText("A garden holds at most 2 farm connectors, and this one has 2.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Connect Slack" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Connect Google Drive" })).toBeNull();
  });

  it("offers Connect again when one of two connectors is being deleted", async () => {
    only(vendor({ status: "deleting" }), vendor({ id: "conn_drive", kind: "google_drive", account: undefined }));
    await renderScreen();
    expect(await button("Connect Slack")).toBeTruthy();
    expect(screen.queryByText(/holds at most 2/)).toBeNull();
  });

  it("says when connectors are off on the server", async () => {
    mockConnectors.setAvailable(mockStore, false);
    await renderScreen();
    expect(await screen.findByText("Farm connectors are off")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Connect Slack" })).toBeNull();
  });

  it("shows no connector, read-only below owner", async () => {
    only();
    scenario.set({ role: "editor" });
    await renderScreen();
    expect(await screen.findByText("No farm connectors yet")).toBeTruthy();
    expect(screen.getByText(/Only the garden's owners connect/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Connect Slack" })).toBeNull();
  });

  it("shows a pending connect, which the owner can cancel", async () => {
    only(vendor({ status: "pending", account: undefined, sources: [], record_count: 0, synced_at: undefined }));
    await renderScreen();
    expect(await screen.findByText("Waiting for consent")).toBeTruthy();
    await fireEvent.press(await button("Cancel connect"));
    await waitFor(() => expect(mockStore.chat.connectors).toHaveLength(0));
  });

  it("says so when a cancel fails", async () => {
    only(vendor({ status: "pending", account: undefined, sources: [], record_count: 0, synced_at: undefined }));
    await renderScreen();
    scenario.set({ role: "editor" });
    await fireEvent.press(await button("Cancel connect"));
    expect(await screen.findByText("Not cancelled")).toBeTruthy();
    expect(screen.getByText("Only the garden's owners can do that.")).toBeTruthy();
  });

  it("shows an active connector, syncs it, and words the 5-minute limit", async () => {
    only(vendor({}));
    await renderScreen();
    const card = await screen.findByTestId("connector-conn_vendor");
    expect(within(card).getByText("Connected")).toBeTruthy();
    expect(within(card).getByText("Maple Row Farm")).toBeTruthy();
    expect(within(card).getByText(/Last synced .* · 34 messages/)).toBeTruthy();
    await fireEvent.press(within(card).getByRole("button", { name: "Sync now" }));
    expect(await screen.findByText("Syncing")).toBeTruthy();
    await fireEvent.press(within(card).getByRole("button", { name: "Sync now" }));
    expect(await screen.findByText(/less than 5 minutes ago/)).toBeTruthy();
  });

  it("asks before it disconnects, then shows the connector going", async () => {
    only(vendor({}));
    const alert = jest.spyOn(Alert, "alert");
    await renderScreen();
    await fireEvent.press(await button("Disconnect"));
    expect(alert).toHaveBeenCalledWith("Disconnect Slack?", expect.stringMatching(/deletes every message/), expect.any(Array));
    const buttons = alert.mock.calls[0]![2]!;
    await act(async () => buttons.find((b) => b.style === "destructive")!.onPress!());
    await waitFor(() => expect(mockStore.chat.connectors).toHaveLength(0));
  });

  it("shows deleting with no actions", async () => {
    only(vendor({ status: "deleting" }));
    await renderScreen();
    expect(await screen.findByText("Disconnecting")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Disconnect" })).toBeNull();
  });

  it("offers Reconnect on reauth_needed, which connects again and replaces the old connector", async () => {
    only(vendor({ status: "reauth_needed" }));
    await renderScreen();
    expect(await screen.findByText("Needs reconnecting")).toBeTruthy();
    await fireEvent.press(await button("Reconnect"));
    expect(await screen.findByTestId("source-picker")).toBeTruthy();
    await waitFor(() => expect(mockStore.chat.connectors.map((c) => c.status)).toEqual(["active"]));
    expect(mockStore.chat.connectors[0]!.id).not.toBe("conn_vendor");
  });

  it("reconnects in a garden that already has 2 connectors, deleting the old one first", async () => {
    only(vendor({ status: "reauth_needed" }), vendor({ id: "conn_drive", kind: "google_drive", account: undefined }));
    await renderScreen();
    expect(await screen.findByText(/holds at most 2/)).toBeTruthy();
    await fireEvent.press(await button("Reconnect"));
    expect(await screen.findByTestId("source-picker")).toBeTruthy();
    expect(screen.queryByText("Not connected")).toBeNull();
    await waitFor(() => expect(mockStore.chat.connectors.map((c) => `${c.kind}:${c.status}`).sort()).toEqual(["google_drive:active", "slack:active"]));
    expect(mockStore.chat.connectors.map((c) => c.id)).not.toContain("conn_vendor");
  });
});

describe("the connect flow (EON E10.2)", () => {
  it("connects Slack with the device's verifier, then saves the chosen channels", async () => {
    only();
    await renderScreen();
    await fireEvent.press(await button("Connect Slack"));
    const picker = await screen.findByTestId("source-picker");
    expect(within(picker).getByText(/Every member of this garden can ask about what's in these channels/)).toBeTruthy();
    await fireEvent.press(await within(picker).findByTestId("source-C0IRRIG"));
    await fireEvent.press(within(picker).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByTestId("source-picker")).toBeNull());
    const [c] = mockStore.chat.connectors;
    expect(c).toMatchObject({ kind: "slack", status: "active", endpoint: "vendor", sources: [{ id: "C0IRRIG", name: "irrigation" }] });
    expect(await screen.findByText("#irrigation")).toBeTruthy();
  });

  it("drops the pending connector when the owner closes the consent page", async () => {
    only();
    mockConnectors.setNextConsent("cancel");
    await renderScreen();
    await fireEvent.press(await button("Connect Google Drive"));
    await waitFor(() => expect(mockStore.chat.connectors).toHaveLength(0));
    expect(screen.queryByText("Not connected")).toBeNull();
  });

  it("says so when the owner declines in the vendor's page", async () => {
    only();
    mockConnectors.setNextConsent("access_denied");
    await renderScreen();
    await fireEvent.press(await button("Connect Slack"));
    expect(await screen.findByText("You didn't allow PDM in Slack. Nothing was connected.")).toBeTruthy();
    await waitFor(() => expect(mockStore.chat.connectors).toHaveLength(0));
  });

  it("refuses a redirect whose state isn't the one the API signed", async () => {
    only();
    authSession.setFake(async (url, redirect) => {
      const real = await mockAuthSession(url, redirect);
      return real.type === "success" ? { type: "success", url: real.url.replace(/state=[^&]+/, "state=tampered") } : real;
    });
    await renderScreen();
    await fireEvent.press(await button("Connect Slack"));
    expect(await screen.findByText(/expired or was already used/)).toBeTruthy();
    await waitFor(() => expect(mockStore.chat.connectors).toHaveLength(0));
  });

  it("sends one create for a quick double tap", async () => {
    only();
    let release: (r: { type: "cancel" }) => void = () => undefined;
    authSession.setFake(() => new Promise((resolve) => (release = resolve)));
    const qc = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
    const { result } = await renderHook(() => useConnectFlow(IDS.garden, () => undefined), {
      wrapper: ({ children }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>,
    });
    // Both taps run in the same render, so both see the idle phase.
    const { start } = result.current;
    let flows: Promise<void>[] = [];
    await act(async () => {
      flows = [start("slack"), start("slack")];
    });
    await waitFor(() => expect(mockStore.chat.connectors).toHaveLength(1));
    await act(async () => {
      release({ type: "cancel" });
      await Promise.all(flows);
    });
    await waitFor(() => expect(mockStore.chat.connectors).toHaveLength(0));
  });

  it("refuses a connect for anyone below owner (403)", async () => {
    only();
    await renderScreen();
    scenario.set({ role: "editor" });
    await fireEvent.press(await button("Connect Slack"));
    expect(await screen.findByText("Only the garden's owners can do that.")).toBeTruthy();
  });
});
