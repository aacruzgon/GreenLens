// A confirmation card's state (flows 10 and 19; README 11, chat cards). Pure: the card feeds it the record, the
// clock and what confirm or reject answered, and shows what `cardView` says.
import type { PendingAction, Task } from "@/domain/types";

export type CardState =
  | { kind: "proposed" }
  | { kind: "confirming" }
  | { kind: "rejecting" }
  | { kind: "confirmed"; task?: Task; planUpdated: boolean }
  | { kind: "rejected" }
  | { kind: "expired" }
  /** The plan changed, or a plant in it was removed, since the card was made: "Ask again". */
  | { kind: "stale"; reason: "plan_changed" | "neighbour_removed" }
  | { kind: "plant_removed" }
  /** A network or server error: the buttons stay, so the user can try again. */
  | { kind: "error"; message: string };

/** The state a card opens in, from its stored status and `expires_at`. */
export function initialCardState(pa: Pick<PendingAction, "status" | "expires_at">, now: Date = new Date()): CardState {
  switch (pa.status) {
    case "confirmed":
      return { kind: "confirmed", planUpdated: false };
    case "rejected":
      return { kind: "rejected" };
    case "expired":
      return { kind: "expired" };
    case "proposed":
      return new Date(pa.expires_at).getTime() <= now.getTime() ? { kind: "expired" } : { kind: "proposed" };
  }
}

/** The fields of an ApiError the card reads; a plain object, so the model needs no api import. */
export interface ConfirmFailure {
  status?: number;
  code?: string;
  details?: Record<string, unknown>;
  message?: string;
}

/** Confirm's `409 invalid_state` rows (flows 10), or a plain error that keeps the card's buttons. */
export function stateFromConfirmError(e: ConfirmFailure): CardState {
  if (e.code === "invalid_state") {
    const d = e.details ?? {};
    if (typeof d.current_plan_id === "string") return { kind: "stale", reason: "plan_changed" };
    if (d.hint === "neighbour_removed") return { kind: "stale", reason: "neighbour_removed" };
    if (d.hint === "plant_removed") return { kind: "plant_removed" };
    if (d.status === "rejected") return { kind: "rejected" };
    return { kind: "expired" };
  }
  return { kind: "error", message: e.message ?? "That didn't go through. Try again." };
}

export interface CardView {
  /** The confirm and reject buttons are shown. */
  actionable: boolean;
  /** A line under the card's body: the outcome, or why it can't be done. */
  note?: string;
  /** "Ask again" prefills the composer (flows 10). */
  askAgain: boolean;
}

export function cardView(state: CardState, pa: Pick<PendingAction, "preview">): CardView {
  switch (state.kind) {
    case "proposed":
    case "confirming":
    case "rejecting":
      return { actionable: true, askAgain: false };
    case "error":
      return { actionable: true, askAgain: false, note: state.message };
    case "confirmed":
      return { actionable: false, askAgain: false, note: pa.preview.kind === "plan" || state.planUpdated ? "Plan updated" : "Added to your tasks" };
    case "rejected":
      return { actionable: false, askAgain: false, note: "Not added" };
    case "expired":
      return { actionable: false, askAgain: false, note: "This suggestion has expired." };
    case "stale":
      return {
        actionable: false,
        askAgain: true,
        note: state.reason === "plan_changed" ? "The plan changed since this was suggested. Ask again?" : "A plant in this plan was removed. Ask again?",
      };
    case "plant_removed":
      return { actionable: false, askAgain: false, note: "That plant was removed." };
  }
}

/** What "Ask again" puts in the composer: the card's own request, in the user's words. */
export function askAgainText(pa: Pick<PendingAction, "card" | "preview">): string {
  return pa.preview.kind === "plan" ? `Can you suggest the plan change again? ${pa.preview.plan_summary}` : `Can you suggest this again? ${pa.card.body}`;
}
