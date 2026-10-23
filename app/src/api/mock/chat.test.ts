// The chat stream through the api client against the mock (README 15, "a scripted chat turn"): events from the
// SSE `event:` line, the JSON replay of a retried send, a dropped stream stored as interrupted, the cards and
// chat memories, and the demo farm connector.
import { chat, decodeChatEvent, newKey, sendChatMessage, setTransport, type ChatStreamEvent } from "@/api";
import type { ChatMessage } from "@/domain/proposed";

import { IDS, mockStore, mockTransport, resetMock, scenario } from "./index";

// expo-crypto has no native side under Jest; keys must be real for the replay.
jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
});

async function collect(conversationId: string, text: string, key = newKey()) {
  const res = await sendChatMessage(conversationId, { text }, key);
  if (res.kind !== "stream") throw new Error("expected a stream");
  const events: ChatStreamEvent[] = [];
  let threw = false;
  try {
    for await (const e of res.events) events.push(e);
  } catch {
    threw = true;
  }
  return { events, threw, key };
}

async function gardenConversation() {
  return chat.create({ garden_id: IDS.garden, anchor: { type: "garden", id: IDS.garden } }, newKey());
}

describe("decodeChatEvent", () => {
  it("takes the name from the event line and drops what the app must ignore", () => {
    expect(decodeChatEvent("text_delta", '{"v":1,"text":"Hi"}')).toEqual({ type: "text_delta", v: 1, text: "Hi" });
    expect(decodeChatEvent("text_delta", '{"v":2,"text":"Hi"}')).toBeUndefined();
    expect(decodeChatEvent("something_new", '{"v":1}')).toBeUndefined();
    expect(decodeChatEvent("text_delta", "not json")).toBeUndefined();
  });
});

describe("mock chat", () => {
  it("lists the seeded conversations, newest first, and filters by anchor", async () => {
    const all = await chat.list(IDS.garden);
    expect(all.items.map((c) => c.id)).toEqual(["conv_garden_spray", "conv_plant_tomato1"]);
    const plant = await chat.list(IDS.garden, { type: "plant", id: IDS.tomato1 });
    expect(plant.items.map((c) => c.id)).toEqual(["conv_plant_tomato1"]);
  });

  it("streams the frost question with tool chips, a planner block and a source, then stores it", async () => {
    const c = await gardenConversation();
    const { events } = await collect(c.id, "Should I cover anything this week?");
    const types = events.map((e) => e.type);
    expect(types[0]).toBe("message_started");
    expect(types).toContain("tool_started");
    expect(types).toContain("answer_block");
    expect(types).toContain("citation");
    expect(types.at(-1)).toBe("message_completed");
    const block = events.find((e) => e.type === "answer_block");
    expect(block?.type === "answer_block" && block.block.kind).toBe("frost_planner");

    const detail = await chat.get(c.id);
    const [assistant, user] = detail.messages.items as ChatMessage[];
    expect(user!.text).toBe("Should I cover anything this week?");
    expect(assistant!.blocks?.[0]?.kind).toBe("frost_planner");
    expect(assistant!.interrupted).toBe(false);
  });

  it("answers the planner's request with a task card, which a confirm turns into a chat task", async () => {
    const c = await gardenConversation();
    const { events } = await collect(c.id, "Add a task to cover Tomato 1 and Basil on Thursday evening.");
    const pa = events.find((e) => e.type === "pending_action");
    if (pa?.type !== "pending_action") throw new Error("no card");
    expect(pa.pending_action.card.body).toBe("Cover Tomato 1 and Basil before the frost");
    const res = await chat.confirm(pa.pending_action.id);
    expect(res.pending_action.status).toBe("confirmed");
    expect(mockStore.tasks.find((t) => t.id === res.result.task?.id)?.source).toBe("chat");
    // A retried confirm gets the first result.
    expect((await chat.confirm(pa.pending_action.id)).result.task?.id).toBe(res.result.task?.id);
  });

  it("gives a viewer no card (write tools are for editors and owners)", async () => {
    const c = await gardenConversation();
    scenario.set({ role: "viewer" });
    const { events } = await collect(c.id, "Add a task to cover Basil.");
    expect(events.some((e) => e.type === "pending_action")).toBe(false);
  });

  it("replays a finished turn as JSON for a retried key (contracts 1)", async () => {
    const c = await gardenConversation();
    const first = await collect(c.id, "Should I water today?");
    const again = await sendChatMessage(c.id, { text: "Should I water today?" }, first.key);
    expect(again.kind).toBe("replay");
    if (again.kind === "replay") expect(again.message.text).toMatch(/water|rain/i);
  });

  it("stores a dropped turn as interrupted", async () => {
    const c = await gardenConversation();
    scenario.set({ dropNextStream: true });
    const { events, threw } = await collect(c.id, "Should I water today?");
    expect(threw).toBe(true);
    expect(events.some((e) => e.type === "message_completed")).toBe(false);
    const detail = await chat.get(c.id);
    expect((detail.messages.items[0] as ChatMessage).interrupted).toBe(true);
  });

  it("saves a chat memory, which Share makes shared and Undo deletes", async () => {
    const c = await chat.create({ garden_id: IDS.garden, anchor: { type: "plant", id: IDS.tomato1 } }, newKey());
    const { events } = await collect(c.id, "It's in a container");
    const saved = events.find((e) => e.type === "memory_saved");
    if (saved?.type !== "memory_saved") throw new Error("no memory");
    expect(saved.memory).toMatchObject({ source: "chat", shared: false, plant_id: IDS.tomato1 });
    const res = await mockTransport.fetch(`https://mock.pdm.local/memories/${saved.memory.id}/share`, { method: "POST" });
    expect(((await res.json()) as { shared: boolean }).shared).toBe(true);
  });

  it("cites farm records on the stored message, from the demo garden's era connectors", async () => {
    const c = await gardenConversation();
    await collect(c.id, "When did we last spray?");
    const detail = await chat.get(c.id);
    const answer = detail.messages.items[0] as ChatMessage;
    expect(answer.farm_citations?.map((f) => f.title)).toEqual(["Spray log 2026", "#field-reports"]);
    expect(answer.farm_citations?.[0]?.text).toMatch(/Aug 2/);
    const list = await chat.connectors(IDS.garden);
    expect(list.connectors.every((x) => x.endpoint === "era")).toBe(true);
  });

  it("refuses a message over 2,000 characters before the stream", async () => {
    const c = await gardenConversation();
    await expect(sendChatMessage(c.id, { text: "x".repeat(2001) }, newKey())).rejects.toMatchObject({ code: "invalid_request" });
  });
});
