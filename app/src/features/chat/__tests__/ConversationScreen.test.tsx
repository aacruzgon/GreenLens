// F6's mock exit checks in Jest (roadmap 12), over the mock with no delays: a scripted turn streams with a tool
// chip, a block and a card; the frost planner's choice becomes a task card that a confirm adds to the tasks; a
// dropped stream offers Resend; an era record opens its text in a sheet; a viewer gets no card.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";

import { chat, newKey, setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { ThemeProvider } from "@/ui";

import { ConversationListScreen } from "../screens/ConversationListScreen";
import { ConversationScreen } from "../screens/ConversationScreen";

jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));
const mockRouter = { back: jest.fn(), push: jest.fn(), replace: jest.fn() };
let mockParams: Record<string, string> = {};
jest.mock("expo-router", () => ({
  useLocalSearchParams: () => mockParams,
  useRouter: () => mockRouter,
  useFocusEffect: () => undefined,
}));
jest.mock("@/features/capture", () => ({
  ...jest.requireActual<object>("@/features/capture"),
  useStartCapture: () => ({ start: jest.fn(async () => undefined), isPending: false, error: null }),
}));

jest.setTimeout(30_000);

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});
afterEach(() => new Promise((r) => setTimeout(r, 50)));

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

async function ask(text: string) {
  await fireEvent.changeText(screen.getByTestId("composer-input"), text);
  await fireEvent.press(screen.getByTestId("composer-send"));
}

describe("the conversation (screens 3.6)", () => {
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

  it("turns the frost planner's choice into a confirmed task (F6 live check, on the mock)", async () => {
    await openNew();
    await ask("Should I cover anything this week?");
    const planner = await screen.findByTestId("block-frost_planner");
    expect(within(planner).getByTestId("frost-summary").props.children).toMatch(/^Cover 4 plants/);
    // Basil off: the summary follows.
    await fireEvent.press(within(planner).getByTestId(`frost-plant-${IDS.basil}`));
    expect(within(planner).getByTestId("frost-summary").props.children).toMatch(/^Cover 3 plants.*Basil stays uncovered/);
    await fireEvent.press(within(planner).getByTestId("frost-ask"));
    const confirm = await screen.findByTestId("pending-confirm");
    await fireEvent.press(confirm);
    await waitFor(() => expect(mockStore.tasks.some((t) => t.source === "chat" && /Tomato 1, Tomato 2 and Pepper/.test(t.title) && !/Basil/.test(t.title))).toBe(true));
    expect(await screen.findByText("Added to your tasks")).toBeTruthy();
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
    scenario.set({ chatBudgetUsed: true });
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
    await fireEvent.press(within(records).getByText("Spray log 2026"));
    const sheet = await screen.findByTestId("farm-record-sheet");
    expect(within(sheet).getByText("Demo data from Era by Eon (synthetic)")).toBeTruthy();
    expect(within(sheet).getByText(/Aug 2: east bed again/)).toBeTruthy();
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
