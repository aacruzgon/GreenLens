// One exchange in the conversation (screens 3.6): the user's bubble, and the assistant's answer with its tool
// chips while it works, the text, answer blocks, sources, farm records, cards, a remembered fact, a rescan and
// feedback; an interrupted answer offers Resend (flows 18).
import { Image } from "expo-image";
import { ActivityIndicator, View } from "react-native";

import { imageSource } from "@/api";
import type { AnswerBlock, FarmCitationWithText } from "@/domain/proposed";
import { formatDateTime } from "@/domain/format";
import type { Attachment, Citation, Memory, PendingAction } from "@/domain/types";
import { links } from "@/platform";
import { Button, CitationList, Text, radii, space, useTheme } from "@/ui";

import type { ChatItem } from "../hooks/useConversationScreen";
import { visibleTools, type RescanRequest, type ToolChip, type TurnError } from "../model/messageReducer";
import { AnswerBlockView } from "./blocks/AnswerBlockView";
import { FarmCitations } from "./FarmCitations";
import { FeedbackRow } from "./FeedbackRow";
import { MemoryCard } from "./MemoryCard";
import { PendingActionCard } from "./PendingActionCard";
import { RescanCard } from "./RescanCard";
import { ToolChips } from "./ToolChips";

/** What every message needs from the screen. */
export interface MessageContext {
  gardenId: string;
  timeZone: string;
  canWrite: boolean;
  canShare: boolean;
  canScan: boolean;
  /** `task.write`, for the frost planner's Add. */
  canAddTasks: boolean;
  /** A turn is streaming: Resend waits. */
  busy: boolean;
  plantName: (id: string) => string | undefined;
  /** Puts text in the composer ("Ask again"). */
  onPrefill: (text: string) => void;
  onOpenPlant: (plantId: string) => void;
  onRescan: (plantId: string, photoKind: RescanRequest["photoKind"]) => void;
  rescanning: boolean;
  onResend: (request: { text: string; attachments: Attachment[] }, failedLocalId?: string) => void;
  requestBefore: (messageId: string) => { text: string; attachments: Attachment[] } | undefined;
}

function UserBubble({ text, attachments }: { text: string; attachments: readonly Attachment[] }) {
  const { colors } = useTheme();
  return (
    <View style={{ alignSelf: "flex-end", maxWidth: "85%", gap: space[1], alignItems: "flex-end" }}>
      {attachments.length ? (
        <View style={{ flexDirection: "row", gap: space[1] }}>
          {attachments.map((a) => (
            <Image key={a.image_id} source={imageSource(a.url, a.image_id)} contentFit="cover" accessibilityLabel="Your photo" style={{ width: 72, height: 72, borderRadius: radii.thumb }} />
          ))}
        </View>
      ) : null}
      <View style={{ paddingVertical: space[2], paddingHorizontal: space[3], borderRadius: 20, borderCurve: "continuous", backgroundColor: colors.surfaceMuted }}>
        <Text variant="body">{text}</Text>
      </View>
    </View>
  );
}

interface AnswerProps {
  ctx: MessageContext;
  status: "sending" | "streaming" | "complete" | "errored" | "interrupted" | "failed";
  text: string;
  tools: readonly ToolChip[];
  blocks: readonly AnswerBlock[];
  citations: readonly Citation[];
  farm: readonly FarmCitationWithText[];
  pending: readonly PendingAction[];
  memories: readonly Memory[];
  rescans: readonly RescanRequest[];
  error?: TurnError;
  feedbackId?: string;
  onResend?: () => void;
}

function errorText(e: TurnError, timeZone: string): string {
  if (e.code === "budget_exceeded") {
    return e.resetsAt ? `Today's chat limit is used up. It resets ${formatDateTime(e.resetsAt, timeZone)}.` : "Today's chat limit is used up. Try again tomorrow.";
  }
  return e.message;
}

function Answer(p: AnswerProps) {
  const { colors } = useTheme();
  const { ctx } = p;
  const waiting = (p.status === "sending" || p.status === "streaming") && !p.text && p.tools.length === 0;
  const over = p.status === "complete" || p.status === "errored" || p.status === "interrupted";
  return (
    <View style={{ gap: space[3] }}>
      {p.status !== "failed" ? (
        <View style={{ borderLeftWidth: 2, borderLeftColor: colors.accent, paddingLeft: space[3], gap: space[2] }}>
          <ToolChips tools={p.tools} />
          {waiting ? (
            <View style={{ flexDirection: "row", alignItems: "center", gap: space[2] }} accessible accessibilityLabel="Thinking">
              <ActivityIndicator size="small" color={colors.textMuted} />
              <Text variant="footnote" color="textMuted">
                Thinking…
              </Text>
            </View>
          ) : null}
          {p.text ? (
            <Text variant="body" selectable accessibilityLiveRegion={over ? "none" : "polite"}>
              {p.text}
            </Text>
          ) : null}
        </View>
      ) : null}
      {p.blocks.map((b, i) => (
        <AnswerBlockView key={`${b.kind}-${b.call_id}-${i}`} block={b} gardenId={ctx.gardenId} canAddTasks={ctx.canAddTasks} onOpenPlant={ctx.onOpenPlant} />
      ))}
      {p.citations.length ? (
        <View style={{ borderRadius: radii.field, backgroundColor: colors.surfaceMuted, overflow: "hidden" }}>
          <Text variant="caption1" color="textMuted" emphasis style={{ paddingHorizontal: space[4], paddingTop: space[2], textTransform: "uppercase", letterSpacing: 0.6 }}>
            Sources
          </Text>
          <CitationList citations={p.citations} onOpen={(url) => void links.openInBrowser(url)} />
        </View>
      ) : null}
      <FarmCitations citations={p.farm} gardenId={ctx.gardenId} timeZone={ctx.timeZone} />
      {ctx.canWrite ? p.pending.map((a) => <PendingActionCard key={a.id} action={a} gardenId={ctx.gardenId} onAskAgain={ctx.onPrefill} />) : null}
      {p.memories.map((m) => (
        <MemoryCard key={m.id} memory={m} canShare={ctx.canShare} />
      ))}
      {p.rescans.map((r, i) => (
        <RescanCard key={`${r.plantId}-${i}`} rescan={r} plantName={ctx.plantName(r.plantId)} canScan={ctx.canScan} starting={ctx.rescanning} onScan={() => ctx.onRescan(r.plantId, r.photoKind)} />
      ))}
      {p.status === "errored" && p.error ? (
        <Text variant="footnote" color="danger" accessibilityLiveRegion="polite">
          {errorText(p.error, ctx.timeZone)}
        </Text>
      ) : null}
      {p.status === "failed" && p.error ? (
        <View style={{ gap: space[2] }} accessibilityLiveRegion="polite" testID="turn-failed">
          <Text variant="footnote" color="danger">
            {errorText(p.error, ctx.timeZone)}
          </Text>
          {p.error.retryable && p.onResend ? <Button label="Try again" variant="secondary" size="small" onPress={p.onResend} disabled={ctx.busy} style={{ alignSelf: "flex-start" }} /> : null}
        </View>
      ) : null}
      {p.status === "interrupted" || (p.status === "errored" && p.error?.retryable) ? (
        <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }} testID="turn-interrupted">
          <Text variant="footnote" color="textMuted">
            {p.status === "interrupted" ? "Interrupted" : "Didn't finish"}
          </Text>
          {p.onResend ? <Button label="Resend" variant="secondary" size="small" onPress={p.onResend} disabled={ctx.busy} testID="resend" /> : null}
        </View>
      ) : null}
      {/* Keyed by message: FlashList recycles cells, and the row's state belongs to one answer. */}
      {p.status === "complete" && p.feedbackId ? <FeedbackRow key={p.feedbackId} messageId={p.feedbackId} /> : null}
    </View>
  );
}

export function MessageView({ item, ctx }: { item: ChatItem; ctx: MessageContext }) {
  if (item.kind === "message") {
    const m = item.message;
    if (m.role === "user") return <UserBubble text={m.text} attachments={m.attachments} />;
    const request = item.resendable ? ctx.requestBefore(m.id) : undefined;
    return (
      <Answer
        ctx={ctx}
        status={item.extras?.error ? "errored" : m.interrupted ? "interrupted" : "complete"}
        text={m.text}
        tools={[]}
        blocks={m.blocks ?? []}
        citations={m.citations}
        farm={m.farm_citations ?? []}
        pending={m.pending_actions}
        memories={item.extras?.memories ?? []}
        rescans={item.extras?.rescans ?? []}
        error={item.extras?.error}
        feedbackId={m.interrupted ? undefined : m.id}
        onResend={request ? () => ctx.onResend(request) : undefined}
      />
    );
  }
  const t = item.turn;
  return (
    <View style={{ gap: space[3] }}>
      <UserBubble text={t.request.text} attachments={t.request.attachments} />
      <Answer
        ctx={ctx}
        status={t.status}
        text={t.text}
        tools={visibleTools(t)}
        blocks={t.blocks}
        citations={t.citations}
        farm={[]}
        pending={t.pendingActions}
        memories={t.memories}
        rescans={t.rescans}
        error={t.error}
        feedbackId={t.status === "complete" ? t.messageId : undefined}
        onResend={() => ctx.onResend(t.request, t.localId)}
      />
    </View>
  );
}
