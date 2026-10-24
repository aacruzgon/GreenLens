import type { PendingAction } from "@/domain/types";

import { askAgainText, cardView, initialCardState, stateFromConfirmError } from "./pendingCard";

const task: Pick<PendingAction, "card" | "preview"> = {
  card: { title: "Add a task?", body: "Cover the tomatoes", confirm_label: "Add task", reject_label: "Not now" },
  preview: { kind: "task", task: { id: "t", garden_id: "g", source: "chat", title: "Cover the tomatoes", created_at: "2026-10-23T10:00:00Z" } },
};
const plan: Pick<PendingAction, "card" | "preview"> = {
  card: { title: "Revise the plan?", body: "No copper", confirm_label: "Revise", reject_label: "Not now" },
  preview: { kind: "plan", base_plan_id: "p", plan_summary: "Drop the copper spray", changed_steps: [], removed_step_ids: ["s1"], constraints_added: ["No copper products"] },
};

describe("pending action cards (flows 10, 19)", () => {
  const now = new Date("2026-10-23T12:00:00Z");

  it("opens from the stored status, and expires on the clock", () => {
    expect(initialCardState({ status: "proposed", expires_at: "2026-10-24T12:00:00Z" }, now)).toEqual({ kind: "proposed" });
    expect(initialCardState({ status: "proposed", expires_at: "2026-10-23T11:00:00Z" }, now)).toEqual({ kind: "expired" });
    expect(initialCardState({ status: "confirmed", expires_at: "2026-10-24T12:00:00Z" }, now).kind).toBe("confirmed");
    expect(initialCardState({ status: "rejected", expires_at: "2026-10-24T12:00:00Z" }, now).kind).toBe("rejected");
  });

  it.each([
    [{ current_plan_id: "p2" }, { kind: "stale", reason: "plan_changed" }],
    [{ hint: "neighbour_removed" }, { kind: "stale", reason: "neighbour_removed" }],
    [{ hint: "plant_removed" }, { kind: "plant_removed" }],
    [{ status: "rejected" }, { kind: "rejected" }],
    [{ status: "expired" }, { kind: "expired" }],
  ])("maps confirm's 409 %j", (details, state) => {
    expect(stateFromConfirmError({ status: 409, code: "invalid_state", details })).toEqual(state);
  });

  it("keeps the buttons after a network error", () => {
    const s = stateFromConfirmError({ message: "No connection" });
    expect(cardView(s, task)).toEqual({ actionable: true, askAgain: false, note: "No connection" });
  });

  it("words each outcome as the flows table does", () => {
    expect(cardView({ kind: "confirmed", planUpdated: false }, task).note).toBe("Added to your tasks");
    expect(cardView({ kind: "confirmed", planUpdated: true }, plan).note).toBe("Plan updated");
    expect(cardView({ kind: "stale", reason: "plan_changed" }, plan)).toEqual({ actionable: false, askAgain: true, note: "The plan changed since this was suggested. Ask again?" });
    expect(cardView({ kind: "stale", reason: "neighbour_removed" }, plan).note).toBe("A plant in this plan was removed. Ask again?");
    expect(cardView({ kind: "plant_removed" }, task)).toEqual({ actionable: false, askAgain: false, note: "That plant was removed." });
  });

  it("asks again in the card's own words", () => {
    expect(askAgainText(plan)).toBe("Can you suggest the plan change again? Drop the copper spray");
    expect(askAgainText(task)).toBe("Can you suggest this again? Cover the tomatoes");
  });
});
