// A confirmation card (screens 3.6; flows 10, 19; README 11, chat cards): the server's own card text and the
// preview, the task or the plan's changes, with "This changes the plan for everyone in the garden" on a plan.
// Nothing is written until Confirm. Only editors and owners are offered write tools, so only they get cards.
import { useState } from "react";
import { View } from "react-native";

import { isApiError, useConfirmPendingAction, useRejectPendingAction } from "@/api";
import { formatLocalDate } from "@/domain/format";
import type { PendingAction } from "@/domain/types";
import { haptics } from "@/platform";
import { Button, Card, LineIcon, Text, space, useTheme } from "@/ui";

import { askAgainText, cardView, initialCardState, stateFromConfirmError, type CardState } from "../model/pendingCard";

export interface PendingActionCardProps {
  action: PendingAction;
  gardenId: string;
  /** "Ask again": prefills the composer (flows 10). */
  onAskAgain: (text: string) => void;
}

function Preview({ action }: { action: PendingAction }) {
  const p = action.preview;
  if (p.kind === "task") {
    const meta = [p.task.due ? `Due ${formatLocalDate(p.task.due)}` : undefined, p.task.repeat ? `Every ${p.task.repeat.every_days} days` : undefined].filter(Boolean).join(" · ");
    return (
      <View style={{ gap: 2 }}>
        <Text variant="subheadline" emphasis>
          {p.task.title}
        </Text>
        {p.task.detail ? (
          <Text variant="footnote" color="textMuted">
            {p.task.detail}
          </Text>
        ) : null}
        {meta ? (
          <Text variant="footnote" color="textMuted">
            {meta}
          </Text>
        ) : null}
      </View>
    );
  }
  return (
    <View style={{ gap: space[1] }}>
      <Text variant="subheadline">{p.plan_summary}</Text>
      {p.changed_steps.map((s) => (
        <Text key={s.id} variant="footnote">
          • {s.title}
        </Text>
      ))}
      {p.removed_step_ids.length ? (
        <Text variant="footnote" color="textMuted">
          {p.removed_step_ids.length === 1 ? "1 step removed" : `${p.removed_step_ids.length} steps removed`}
        </Text>
      ) : null}
      {p.constraints_added.map((c) => (
        <Text key={c} variant="footnote" color="textMuted">
          From now on: {c}
        </Text>
      ))}
      <Text variant="footnote" color="watch" emphasis>
        This changes the plan for everyone in the garden.
      </Text>
    </View>
  );
}

export function PendingActionCard({ action, gardenId, onAskAgain }: PendingActionCardProps) {
  const { colors } = useTheme();
  const confirm = useConfirmPendingAction(gardenId);
  const reject = useRejectPendingAction();
  const [state, setState] = useState<CardState>(() => initialCardState(action));
  const view = cardView(state, action);

  const onConfirm = () => {
    setState({ kind: "confirming" });
    confirm.mutate(action.id, {
      onSuccess: (res) => {
        void haptics.success();
        setState({ kind: "confirmed", task: res.result.task, planUpdated: Boolean(res.result.plan) });
      },
      onError: (e) =>
        setState(
          stateFromConfirmError(isApiError(e) ? { status: e.status, code: e.code, details: e.details, message: e.message } : { message: "That didn't go through. Check your network and try again." }),
        ),
    });
  };
  const onReject = () => {
    setState({ kind: "rejecting" });
    reject.mutate(action.id, {
      onSuccess: () => setState({ kind: "rejected" }),
      onError: () => setState({ kind: "error", message: "That didn't go through. Try again." }),
    });
  };

  const done = state.kind === "confirmed";
  return (
    <Card style={{ gap: space[3] }} testID={`pending-${action.id}`} accessibilityLabel={`${action.card.title} ${action.card.body}`}>
      <View style={{ flexDirection: "row", alignItems: "center", gap: space[2] }}>
        {done ? <LineIcon name="check" color={colors.healthy} size={18} /> : null}
        <Text variant="headline" accessibilityRole="header">
          {done && action.preview.kind === "task" ? "Task added" : action.card.title}
        </Text>
      </View>
      <Text variant="body">{action.card.body}</Text>
      <Preview action={action} />
      {view.note ? (
        <Text variant="footnote" color={state.kind === "error" ? "danger" : "textMuted"} accessibilityLiveRegion="polite" testID="pending-note">
          {view.note}
        </Text>
      ) : null}
      {view.actionable ? (
        <View style={{ flexDirection: "row", justifyContent: "flex-end", gap: space[2] }}>
          <Button label={action.card.reject_label} variant="secondary" size="small" onPress={onReject} busy={state.kind === "rejecting"} disabled={state.kind === "confirming"} />
          <Button label={action.card.confirm_label} size="small" onPress={onConfirm} busy={state.kind === "confirming"} disabled={state.kind === "rejecting"} testID="pending-confirm" />
        </View>
      ) : null}
      {view.askAgain ? (
        <View style={{ flexDirection: "row", justifyContent: "flex-end" }}>
          <Button label="Ask again" variant="secondary" size="small" onPress={() => onAskAgain(askAgainText(action))} />
        </View>
      ) : null}
    </Card>
  );
}
