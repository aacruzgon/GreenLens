// F6's mock exit checks in Jest (roadmap 12), over the mock with no delays: a scripted turn streams with a tool
// chip, a block and a card; the frost planner's choice becomes tasks; a dropped stream offers Resend; an era record
// opens its text in a sheet; a viewer gets no card. And the review's cases (PR #151): leaving mid-answer, a
// refetch during a turn, `409 request_in_progress`, an `error` event, a stream that goes quiet, a plan card.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";

import { chat, newKey, qk, setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { links } from "@/platform";
import { ThemeProvider } from "@/ui";

import { ConversationListScreen } from "../screens/ConversationListScreen";
import { chatTiming } from "../hooks/useConversationScreen";
import { ConversationScreen } from "../screens/ConversationScreen";

jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));
const mockRouter = { back: jest.fn(), push: jest.fn(), replace: jest.fn() };
let mockParams: Record<string, string> = {};
jest.mock("expo-router", () => ({
  useLocalSearchParams: () => mockParams,
  useRouter: () => mockRouter,
  useFocusEffect: () => undefined,
}));
const mockStart = jest.fn(async () => undefined);
jest.mock("@/features/capture", () => ({
  ...jest.requireActual<object>("@/features/capture"),
  useStartCapture: () => ({ start: mockStart, isPending: false, error: null }),
}));

jest.setTimeout(30_000);

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});
afterEach(async () => {
  await new Promise((r) => setTimeout(r, 50));
  jest.restoreAllMocks();
  chatTiming.idleMs = 45_000;
});

let sharedClient: QueryClient;
const newClient = () => new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });

function wrap(ui: React.ReactElement, client?: QueryClient) {
  const qc = client ?? (sharedClient = newClient());
  return (
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="light" reducedMotion>
        {ui}
      </ThemeProvider>
    </QueryClientProvider>
  );
}

async function openNew(anchor: { type: "garden" | "plant"; id: string } = { type: "garden", id: IDS.garden }) {
  const c = await chat.create({ garden_id: IDS.garden, anchor }, newKey());
  mockParams = { conversationId: c.id };
  await render(wrap(<ConversationScreen />));
  await screen.findByText(/Ask about/);
  return c.id;
}

/** The idempotency keys of every send, read off the mock transport. */
function sendKeys(): () => string[] {
  const keys: string[] = [];
  const real = mockTransport.fetch.bind(mockTransport);
  jest.spyOn(mockTransport, "fetch").mockImplementation((input, init) => {
    const url = typeof input === "string" ? input : input.url;
    const method = init?.method ?? (typeof input === "string" ? "GET" : input.method);
    if (method === "POST" && /\/messages$/.test(url)) {
      const headers = new Headers(init?.headers ?? (typeof input === "string" ? undefined : input.headers));
      keys.push(headers.get("idempotency-key") ?? "");
    }
    return real(input, init);
  });
  return () => keys;
}

const userCount = (conversationId: string, text: string) =>
  mockStore.chat.conversations.get(conversationId)!.messages.filter((m) => m.role === "user" && m.text === text).length;

/** Waits until the cached conversation holds a message matching `pred`, then for the screen to draw it. */
async function cached(id: string, pred: (m: { role: string; interrupted: boolean }) => boolean) {
  await waitFor(() => {
    const data = sharedClient.getQueryData<{ pages: { messages: { items: { role: string; interrupted: boolean }[] } }[] }>(qk.conversation(id));
    expect(data?.pages[0]?.messages.items.some(pred)).toBe(true);
  });
  await new Promise((r) => setTimeout(r, 100));
}

async function ask(text: string) {
  await fireEvent.changeText(screen.getByTestId("composer-input"), text);
  await fireEvent.press(screen.getByTestId("composer-send"));
}

describe("the conversation (screens 3.6)", () => {
  it("keeps Try again on a repeat of an earlier question that never reached the server", async () => {
    // Before: a failed turn gave way to any stored question with its text, so asking again lost Try again (review
    // of #205). Here the earlier copy is answered, and the repeat fails on the network before the server.
    await openNew();
    await ask("Should I water today?");
    expect(await screen.findByTestId("block-weather_hours")).toBeTruthy();
    const real = mockTransport.fetch.bind(mockTransport);
    jest.spyOn(mockTransport, "fetch").mockImplementation((input, init) => {
      const url = typeof input === "string" ? input : input.url;
      const method = init?.method ?? (typeof input === "string" ? "GET" : input.method);
      if (method === "POST" && /\/messages$/.test(url)) return Promise.reject(new TypeError("Network request failed"));
      return real(input, init);
    });
    await ask("Should I water today?");
    expect(await screen.findByRole("button", { name: "Try again" }, { timeout: 20_000 })).toBeTruthy();
  });

  it("streams a scripted turn with a tool chip, a block and a card (F6 exit check)", async () => {
    await openNew();
    await ask("Should I water today?");
    // The answer, its weather block and its card, after the turn is stored.
    expect(await screen.findByTestId("block-weather_hours")).toBeTruthy();
    expect(await screen.findByText("Check the soil a finger deep before watering")).toBeTruthy();
    expect(screen.getByText("Sources")).toBeTruthy();
    expect(screen.getByText("Should I water today?")).toBeTruthy();
    expect(screen.getByTestId("feedback-up")).toBeTruthy();
  });

  it("shows tool chips while the agent works", async () => {
    scenario.set({ delayScale: 1 });
    await openNew();
    await ask("Should I cover anything this week?");
    expect(await screen.findByText("Checking the forecast", {}, { timeout: 3000 })).toBeTruthy();
    expect(await screen.findByTestId("block-frost_planner", {}, { timeout: 8000 })).toBeTruthy();
  });

  it("turns the frost planner's choice into tasks, one per plant, with no card (F6 live check, S5's rule)", async () => {
    await openNew();
    await ask("Should I cover anything this week?");
    const planner = await screen.findByTestId("block-frost_planner");
    expect(within(planner).getByTestId("frost-summary").props.children).toMatch(/^Cover 4 plants/);
    // Lettuce's species has no frost record: not known, never hardy, and not chosen.
    expect(within(within(planner).getByTestId(`frost-plant-${IDS.lettuce}`)).getByText("Not known")).toBeTruthy();
    // Basil off: the summary follows.
    await fireEvent.press(within(planner).getByTestId(`frost-plant-${IDS.basil}`));
    expect(within(planner).getByTestId("frost-summary").props.children).toMatch(/^Cover 3 plants.*Basil stays uncovered/);
    const before = mockStore.tasks.length;
    await fireEvent.press(within(planner).getByTestId("frost-add"));
    expect(await within(planner).findByTestId("frost-added")).toBeTruthy();
    const added = mockStore.tasks.slice(before);
    expect(added.map((t) => t.plant_id).sort()).toEqual([IDS.tomato1, IDS.tomato2, IDS.pepper].sort());
    expect(added.every((t) => t.due === added[0]!.due && /^Cover .* before \w+day's frost$/.test(t.title))).toBe(true);
    expect(screen.queryByTestId("pending-confirm")).toBeNull();
    // Basil can still be added on its own; the three already added aren't added again.
    await fireEvent.press(within(planner).getByTestId(`frost-plant-${IDS.basil}`));
    await fireEvent.press(within(planner).getByTestId("frost-add"));
    await waitFor(() => expect(mockStore.tasks.length).toBe(before + 4));
  });

  it("gives a viewer the frost planner without Add", async () => {
    scenario.set({ role: "viewer" });
    await openNew();
    await ask("Should I cover anything this week?");
    const planner = await screen.findByTestId("block-frost_planner");
    expect(within(planner).queryByTestId("frost-add")).toBeNull();
  });

  it("offers Resend when the stream drops (flows 18)", async () => {
    await openNew();
    scenario.set({ dropNextStream: true });
    await ask("Should I water today?");
    expect(await screen.findByTestId("resend")).toBeTruthy();
    await fireEvent.press(screen.getByTestId("resend"));
    expect(await screen.findByTestId("block-weather_hours")).toBeTruthy();
    // The interrupted answer stays, but no longer offers a second Resend (found on the simulator).
    await waitFor(() => expect(screen.queryByTestId("resend")).toBeNull());
    expect(screen.getByText("Interrupted")).toBeTruthy();
  });

  it("shows a confirmed card as added after the screen remounts (found on the simulator)", async () => {
    const id = await openNew();
    await ask("Should I water today?");
    await fireEvent.press(await screen.findByTestId("pending-confirm"));
    expect(await screen.findByText("Added to your tasks")).toBeTruthy();
    await screen.unmount();
    mockParams = { conversationId: id };
    await render(wrap(<ConversationScreen />, sharedClient));
    expect(await screen.findByText("Task added")).toBeTruthy();
    expect(screen.queryByTestId("pending-confirm")).toBeNull();
  });

  it("says a deleted conversation is gone, with no composer", async () => {
    mockParams = { conversationId: "conv_deleted" };
    await render(wrap(<ConversationScreen />));
    expect(await screen.findByText("This conversation is gone")).toBeTruthy();
    expect(screen.queryByTestId("composer-input")).toBeNull();
  });

  it("says when the chat budget is used up, before any stream", async () => {
    await openNew();
    scenario.set({ chatFault: "budget_exceeded" });
    await ask("Should I water today?");
    expect(await screen.findByText(/Today's chat limit is used up/)).toBeTruthy();
  });

  it("remembers a fact with Undo and Share with garden", async () => {
    await openNew({ type: "plant", id: IDS.tomato1 });
    await ask("It's in a container");
    expect(await screen.findByText(/Tomato 1 is in a container/)).toBeTruthy();
    await fireEvent.press(screen.getByText("Share with garden"));
    expect(await screen.findByText("Shared with the garden")).toBeTruthy();
    expect(mockStore.memories.find((m) => m.text === "Tomato 1 is in a container")?.shared).toBe(true);
  });

  it("gives a viewer the answer but no card", async () => {
    scenario.set({ role: "viewer" });
    await openNew();
    await ask("Should I water today?");
    expect(await screen.findByTestId("block-weather_hours")).toBeTruthy();
    expect(screen.queryByTestId("pending-confirm")).toBeNull();
  });

  it("opens an era record's text in a sheet, labelled as synthetic (EON E10.3)", async () => {
    mockParams = { conversationId: "conv_garden_spray" };
    await render(wrap(<ConversationScreen />));
    const records = await screen.findByTestId("farm-citations");
    await waitFor(() => expect(within(records).getAllByText("DEMO")).toHaveLength(2));
    const open = jest.spyOn(links, "openExternal");
    await fireEvent.press(within(records).getByText("Spray log 2026"));
    const sheet = await screen.findByTestId("farm-record-sheet");
    expect(within(sheet).getByText("Demo data from Era by Eon (synthetic)")).toBeTruthy();
    expect(within(sheet).getByText(/Aug 2: east bed again/)).toBeTruthy();
    expect(within(sheet).queryByText(/^Open in/)).toBeNull();
    expect(open).not.toHaveBeenCalled();
  });

  it("never offers a record's link when its connector isn't known (found in review)", async () => {
    mockStore.chat.connectors = mockStore.chat.connectors.filter((c) => !/slack/.test(c.id));
    mockParams = { conversationId: "conv_garden_spray" };
    await render(wrap(<ConversationScreen />));
    const records = await screen.findByTestId("farm-citations");
    await waitFor(() => expect(within(records).getAllByText("DEMO")).toHaveLength(2));
    const open = jest.spyOn(links, "openExternal");
    await fireEvent.press(within(records).getByText(/#field-reports/));
    const sheet = await screen.findByTestId("farm-record-sheet");
    expect(within(sheet).getByText("Demo data from Era by Eon (synthetic)")).toBeTruthy();
    expect(within(sheet).queryByText(/^Open in/)).toBeNull();
    expect(open).not.toHaveBeenCalled();
  });

  it("shows the question, Interrupted and Resend after leaving mid-answer (review must-fix)", async () => {
    scenario.set({ delayScale: 1 });
    const id = await openNew();
    await ask("Should I water today?");
    await screen.findByText("Checking the forecast", {}, { timeout: 3000 });
    await screen.unmount();
    await waitFor(() => expect(mockStore.chat.conversations.get(id)!.messages.at(-1)?.interrupted).toBe(true));
    scenario.set({ delayScale: 0 });
    await render(wrap(<ConversationScreen />, sharedClient));
    expect(await screen.findByText("Should I water today?")).toBeTruthy();
    expect(await screen.findByText("Interrupted")).toBeTruthy();
    expect(screen.getByTestId("resend")).toBeTruthy();
  });

  it("draws the question once when the conversation refetches during a turn (review)", async () => {
    chatTiming.idleMs = 60_000;
    scenario.set({ chatFault: "stall" });
    const id = await openNew();
    await ask("Should I water today?");
    await waitFor(() => expect(userCount(id, "Should I water today?")).toBe(1));
    await sharedClient.invalidateQueries({ queryKey: qk.conversation(id) });
    // The refetch holds the stored question while the turn still draws its own.
    await cached(id, (m) => m.role === "user");
    expect(screen.getAllByText("Should I water today?")).toHaveLength(1);
    await screen.unmount();
    chatTiming.idleMs = 45_000;
  });

  it("waits out 409 request_in_progress with the same key, and asks only once (review)", async () => {
    const keys = sendKeys();
    const id = await openNew();
    scenario.set({ chatFault: "in_progress" });
    await ask("Should I water today?");
    expect(await screen.findByTestId("block-weather_hours", {}, { timeout: 8000 })).toBeTruthy();
    expect(new Set(keys()).size).toBe(1);
    expect(keys().length).toBe(3);
    expect(userCount(id, "Should I water today?")).toBe(1);
    expect(screen.getAllByText("Should I water today?")).toHaveLength(1);
  });

  it("keeps an error event's text after the stored message arrives, with Resend (review)", async () => {
    const id = await openNew();
    scenario.set({ chatFault: "error_event" });
    await ask("Should I water today?");
    // The stored message has landed (it says only `interrupted`), and the error text is still the one shown.
    await cached(id, (m) => m.role === "assistant" && m.interrupted);
    expect(screen.getByText("The assistant stopped partway. Try again in a minute.")).toBeTruthy();
    expect(screen.getByTestId("resend")).toBeTruthy();
    expect(screen.getByText("Didn't finish")).toBeTruthy();
    expect(screen.queryByText("Interrupted")).toBeNull();
  });

  it("takes a stream that goes quiet as dropped, with Resend on a new key (review)", async () => {
    chatTiming.idleMs = 300;
    const keys = sendKeys();
    await openNew();
    scenario.set({ chatFault: "stall" });
    await ask("Should I water today?");
    await fireEvent.press(await screen.findByTestId("resend", {}, { timeout: 5000 }));
    expect(await screen.findByTestId("block-weather_hours")).toBeTruthy();
    expect(new Set(keys()).size).toBe(2);
    chatTiming.idleMs = 45_000;
  });

  it("confirms a plan card into a new plan version (review)", async () => {
    await openNew({ type: "plant", id: IDS.tomato1 });
    await ask("No copper, please.");
    expect(await screen.findByText("This changes the plan for everyone in the garden.")).toBeTruthy();
    await fireEvent.press(screen.getByTestId("pending-confirm"));
    expect(await screen.findByText("Plan updated")).toBeTruthy();
    const plans = [...mockStore.scans.values()].flatMap((st) => (st.plan ? [st.plan] : []));
    expect(plans.some((p) => p.constraints.some((c) => c.text === "No copper products"))).toBe(true);
  });

  it("starts the rescan on the shot the agent asked for (flows 9)", async () => {
    await openNew({ type: "plant", id: IDS.tomato2 });
    await ask("Can you take a closer look?");
    await fireEvent.press(await screen.findByText("Take a new photo"));
    expect(mockStart).toHaveBeenCalledWith({ plantId: IDS.tomato2, photoKind: "leaf_closeup" }, { push: true });
  });

  it("sends a suggested follow-up", async () => {
    await openNew();
    const chips = await screen.findByTestId("suggestions");
    await fireEvent.press(within(chips).getByText("Should I water today?"));
    expect(await screen.findByTestId("block-weather_hours")).toBeTruthy();
  });
});

describe("the Chat tab", () => {
  it("groups the conversations by anchor and starts a garden chat", async () => {
    await render(wrap(<ConversationListScreen gardenId={IDS.garden} />));
    expect(await screen.findByText("About the garden")).toBeTruthy();
    expect(screen.getByText("About a plant")).toBeTruthy();
    expect(screen.getByText("When did we last spray?")).toBeTruthy();
    await fireEvent.press(screen.getByTestId("chat-new"));
    await waitFor(() => expect(mockRouter.push).toHaveBeenCalledWith(expect.objectContaining({ pathname: "/chat/[conversationId]" })));
  });

  it("leaves out a conversation nobody wrote in, and New chat reuses it (found on the simulator)", async () => {
    const empty = await chat.create({ garden_id: IDS.garden, anchor: { type: "garden", id: IDS.garden } }, newKey());
    await render(wrap(<ConversationListScreen gardenId={IDS.garden} />));
    await screen.findByText("When did we last spray?");
    expect(screen.queryByTestId(`conversation-${empty.id}`)).toBeNull();
    await fireEvent.press(screen.getByTestId("chat-new"));
    await waitFor(() => expect(mockRouter.push).toHaveBeenCalledWith({ pathname: "/chat/[conversationId]", params: { conversationId: empty.id } }));
    expect(mockStore.chat.conversations.size).toBe(3);
  });
});
