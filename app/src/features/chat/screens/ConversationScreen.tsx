// A conversation (screens 3.6; flows 9, 10, 18, 19): what it is about at the top, the exchanges, and the composer
// riding the keyboard. Only the conversation's owner reaches it (contracts 8); anyone else gets 404.
import { FlashList, type FlashListRef } from "@shopify/flash-list";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { View } from "react-native";
import { KeyboardAvoidingView } from "react-native-keyboard-controller";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useAllPlants, useGarden, useScan } from "@/api";
import { formatMonthDay } from "@/domain/format";
import { useStartCapture } from "@/features/capture";
import { Banner, Button, EmptyState, ErrorState, IconButton, LineIcon, Skeleton, Text, gutter, space, useTheme } from "@/ui";

import { Composer } from "../components/Composer";
import { MessageView, type MessageContext } from "../components/MessageView";
import { useConversationScreen, type ChatItem } from "../hooks/useConversationScreen";
import { anchorName } from "../model/conversations";

export function ConversationScreen() {
  const { conversationId } = useLocalSearchParams<{ conversationId: string }>();
  const router = useRouter();
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const s = useConversationScreen(conversationId);
  const garden = useGarden(s.gardenId);
  const plants = useAllPlants(s.gardenId);
  const anchor = s.conversation?.anchor;
  const scan = useScan(anchor?.type === "scan" ? anchor.id : "");
  const capture = useStartCapture();
  const [rescanError, setRescanError] = useState<string>();
  const tz = garden.data?.time_zone ?? "UTC";
  // A new turn scrolls into view, also when a block sent it from higher up the conversation (found on the
  // simulator: "Make it a task" left its card below the fold, past FlashList's autoscroll threshold).
  const list = useRef<FlashListRef<ChatItem>>(null);
  const turnCount = s.items.filter((i) => i.kind === "turn").length;
  const lastTurns = useRef(turnCount);
  useEffect(() => {
    if (turnCount > lastTurns.current) requestAnimationFrame(() => list.current?.scrollToEnd({ animated: true }));
    lastTurns.current = turnCount;
  }, [turnCount]);
  const plantName = (id: string) => plants.data?.find((p) => p.id === id)?.name;

  const about = anchor
    ? anchorName(anchor, {
        gardenName: garden.data?.name,
        plantName: anchor.type === "plant" ? plantName(anchor.id) : undefined,
        scanPlantName: scan.data?.plant.name,
        scanDate: scan.data ? formatMonthDay(scan.data.created_at, tz) : undefined,
      })
    : undefined;

  const ctx: MessageContext = {
    gardenId: s.gardenId,
    timeZone: tz,
    canWrite: s.canWrite,
    canShare: s.canShare,
    canScan: s.canScan,
    busy: s.composer.busy,
    plantName,
    onAsk: (text) => s.actions.send(text),
    onPrefill: (text) => s.composer.setText(text),
    onOpenPlant: (plantId) => router.push({ pathname: "/plants/[plantId]", params: { plantId } }),
    onRescan: (plantId) => {
      setRescanError(undefined);
      capture.start({ plantId }, { push: true }).catch(() => setRescanError("The scan couldn't start. Check your network."));
    },
    rescanning: capture.isPending,
    onResend: s.actions.resend,
    requestBefore: s.actions.requestBefore,
  };

  const header = (
    <View style={{ paddingTop: insets.top + space[1], paddingHorizontal: space[2], paddingBottom: space[2], flexDirection: "row", alignItems: "center", gap: space[1], borderBottomWidth: 1, borderBottomColor: colors.border, backgroundColor: colors.background }}>
      <IconButton accessibilityLabel="Back" onPress={() => router.back()} icon={({ color }) => <LineIcon name="back" color={color} size={22} />} />
      <View style={{ flex: 1 }}>
        <Text variant="caption1" color="textMuted">
          About
        </Text>
        <Text variant="headline" numberOfLines={1} accessibilityRole="header">
          {about ?? " "}
        </Text>
      </View>
    </View>
  );

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="conversation-screen">
      {header}
      <KeyboardAvoidingView behavior="padding" style={{ flex: 1 }}>
        {s.gone ? (
          <EmptyState
            title="This conversation is gone"
            message="It was deleted with its plant or garden, or it isn't yours."
            action={{ label: "Back to chats", onPress: () => router.back(), variant: "secondary" }}
          />
        ) : s.error ? (
          <ErrorState message={s.error} onRetry={s.actions.retry} />
        ) : s.loading ? (
          <View style={{ padding: gutter, gap: space[3] }}>
            <Skeleton height={48} />
            <Skeleton height={96} />
          </View>
        ) : (
          <FlashList
            ref={list}
            data={s.items}
            keyExtractor={(item) => item.key}
            getItemType={(item) => (item.kind === "turn" ? "turn" : item.message.role)}
            renderItem={({ item }) => (
              <View style={{ paddingHorizontal: gutter, paddingVertical: space[2] }}>
                <MessageView item={item} ctx={ctx} />
              </View>
            )}
            maintainVisibleContentPosition={{ startRenderingFromBottom: true, autoscrollToBottomThreshold: 0.2 }}
            keyboardShouldPersistTaps="handled"
            // Dragging the conversation puts the keyboard away, as Messages does; nothing else in it would.
            keyboardDismissMode="interactive"
            contentContainerStyle={{ paddingVertical: space[3] }}
            ListHeaderComponent={
              s.hasOlder ? (
                <Button label="Earlier messages" variant="plain" size="small" onPress={s.loadOlder} busy={s.loadingOlder} style={{ alignSelf: "center" }} />
              ) : null
            }
            ListEmptyComponent={
              <View style={{ padding: gutter, gap: space[2] }}>
                <Text variant="title3" emphasis>
                  Ask about {about ?? "your garden"}
                </Text>
                <Text variant="subheadline" color="textMuted">
                  Watering, frost, spacing, how it&apos;s growing, or what the last scan found. Answers cite their sources.
                </Text>
              </View>
            }
          />
        )}
        {rescanError ? <Banner tone="danger" title={rescanError} style={{ marginHorizontal: gutter, marginBottom: space[2] }} /> : null}
        {s.conversation && !s.gone ? (
          <Composer
            text={s.composer.text}
            onChangeText={s.composer.setText}
            photos={s.composer.photos}
            canSend={s.composer.canSend}
            busy={s.composer.busy}
            onSend={() => s.actions.send()}
            onAddPhoto={() => void s.actions.addPhoto()}
            onRetryPhoto={s.actions.retryPhoto}
            onRemovePhoto={s.actions.removePhoto}
            bottomInset={insets.bottom}
          />
        ) : null}
      </KeyboardAvoidingView>
    </View>
  );
}
