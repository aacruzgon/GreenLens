import type { Memory, PendingAction } from "@/domain/types";

import { isTurnOver, startTurn, turnReducer, visibleTools, type Turn, type TurnEvent } from "./messageReducer";

const ev = (event: TurnEvent) => ({ type: "event" as const, event });
const play = (t: Turn, events: TurnEvent[]) => events.reduce((s, e) => turnReducer(s, ev(e)), t);

const card: PendingAction = {
  id: "pa_1",
  conversation_id: "c",
  tool: "create_task",
  status: "proposed",
  card: { title: "Add a task?", body: "Check the soil", confirm_label: "Add task", reject_label: "Not now" },
  preview: { kind: "task", task: { id: "t", garden_id: "g", source: "chat", title: "Check the soil", created_at: "2026-10-23T10:00:00Z" } },
  expires_at: "2026-10-24T10:00:00Z",
};
const memory: Memory = { id: "m", garden_id: "g", kind: "fact", topic: "container", source: "chat", shared: false, text: "It's in a container", created_at: "2026-10-23T10:00:00Z" };

describe("turnReducer", () => {
  const base = startTurn("l1", { text: "Should I water today?", attachments: [] });

  it("builds a turn from the contract's events (flows 9)", () => {
    const t = play(base, [
      { type: "message_started", message_id: "msg_1", route: "care_question" },
      { type: "tool_started", call_id: "c1", label: "Checking the forecast" },
      { type: "tool_finished", call_id: "c1", ok: true },
      { type: "text_delta", text: "Rain is " },
      { type: "text_delta", text: "forecast." },
      { type: "citation", citation: { source_id: "s", publisher: "OMAFRA", title: "Watering", url: "https://x" } },
      { type: "citation", citation: { source_id: "s", publisher: "OMAFRA", title: "Watering", url: "https://x" } },
      { type: "pending_action", pending_action: card },
      { type: "memory_saved", memory },
      { type: "answer_block", block: { kind: "weather_hours", call_id: "c1", days: [] } },
      { type: "message_completed", message_id: "msg_1" },
    ]);
    expect(t.status).toBe("complete");
    expect(t.messageId).toBe("msg_1");
    expect(t.route).toBe("care_question");
    expect(t.text).toBe("Rain is forecast.");
    expect(t.citations).toHaveLength(1);
    expect(t.pendingActions).toEqual([card]);
    expect(t.memories).toEqual([memory]);
    expect(t.blocks.map((b) => b.kind)).toEqual(["weather_hours"]);
    expect(t.tools).toEqual([{ callId: "c1", label: "Checking the forecast", state: "done" }]);
  });

  it("shows tool chips while it streams and hides them once over", () => {
    const running = play(base, [{ type: "tool_started", call_id: "c1", label: "Reading the garden map" }]);
    expect(running.status).toBe("streaming");
    expect(visibleTools(running)).toHaveLength(1);
    const done = turnReducer(running, ev({ type: "message_completed", message_id: "m" }));
    expect(visibleTools(done)).toEqual([]);
  });

  it("marks a tool that failed", () => {
    const t = play(base, [
      { type: "tool_started", call_id: "c1", label: "Searching" },
      { type: "tool_finished", call_id: "c1", ok: false },
    ]);
    expect(t.tools[0]!.state).toBe("failed");
  });

  it("ends on an error event with its text (flows 18: errored)", () => {
    const t = play(base, [{ type: "error", code: "upstream_unavailable", message: "The model is down.", retryable: true }]);
    expect(t.status).toBe("errored");
    expect(t.error).toEqual({ code: "upstream_unavailable", message: "The model is down.", retryable: true });
  });

  it("is interrupted when the stream drops before message_completed", () => {
    const t = turnReducer(play(base, [{ type: "text_delta", text: "Rain" }]), { type: "dropped" });
    expect(t.status).toBe("interrupted");
    expect(t.text).toBe("Rain");
    expect(isTurnOver(t)).toBe(true);
  });

  it("fails on an HTTP error before the stream", () => {
    const t = turnReducer(base, { type: "failed", error: { code: "budget_exceeded", message: "Today's limit is used up.", retryable: false, resetsAt: "2026-10-24T00:00:00Z" } });
    expect(t.status).toBe("failed");
    expect(t.error?.resetsAt).toBe("2026-10-24T00:00:00Z");
  });

  it("takes a JSON replay as complete (contracts 1)", () => {
    const t = turnReducer(base, {
      type: "replayed",
      message: { id: "msg_9", role: "assistant", text: "Done.", attachments: [], citations: [], pending_actions: [card], interrupted: false, created_at: "2026-10-23T10:00:00Z" },
    });
    expect(t).toMatchObject({ status: "complete", messageId: "msg_9", text: "Done.", pendingActions: [card] });
  });

  it("ignores events after the turn is over", () => {
    const over = turnReducer(base, { type: "dropped" });
    expect(turnReducer(over, ev({ type: "text_delta", text: "late" }))).toBe(over);
  });
});
