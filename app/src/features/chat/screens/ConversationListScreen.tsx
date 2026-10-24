// The Chat tab (screens 3.6): the caller's conversations in the active garden, grouped by anchor (the garden, a
// plant, a scan), newest first, and New chat, anchored to the garden. Every role chats.
import { FlashList } from "@shopify/flash-list";
import { RefreshControl, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { Banner, Button, EmptyState, ErrorState, LineIcon, ListItem, Skeleton, Text, Wash, gutter, minTouch, space, useTheme } from "@/ui";

import { useConversationList, type ConversationRow } from "../hooks/useConversationList";

type Item = { kind: "header"; key: string; title: string } | { kind: "row"; key: string; row: ConversationRow };

export function ConversationListScreen({ gardenId }: { gardenId: string }) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const c = useConversationList(gardenId);

  const header = (
    <View style={{ paddingTop: insets.top + space[2], paddingHorizontal: gutter, gap: space[3], paddingBottom: space[2] }}>
      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
        <Text variant="largeTitle" emphasis>
          Chat
        </Text>
        <Button
          label="New chat"
          variant="secondary"
          size="small"
          onPress={c.actions.newChat}
          busy={c.starting}
          icon={({ color }) => <LineIcon name="plus" color={color} size={18} />}
          testID="chat-new"
        />
      </View>
      {c.problem ? <Banner tone="danger" title={c.problem} /> : null}
    </View>
  );

  const items: Item[] = c.groups.flatMap((g): Item[] => [
    { kind: "header", key: `h-${g.key}`, title: g.title },
    ...g.rows.map((row) => ({ kind: "row" as const, key: row.conversation.id, row })),
  ]);

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="chat-tab">
      <Wash />
      {c.error ? (
        <>
          {header}
          <ErrorState message={c.error} onRetry={c.actions.refresh} />
        </>
      ) : c.loading ? (
        <>
          {header}
          <View style={{ paddingHorizontal: gutter, gap: space[3] }}>
            {[0, 1, 2].map((i) => (
              <Skeleton key={i} height={minTouch + 12} />
            ))}
          </View>
        </>
      ) : (
        <FlashList
          data={items}
          keyExtractor={(item) => item.key}
          getItemType={(item) => item.kind}
          ListHeaderComponent={header}
          refreshControl={<RefreshControl refreshing={c.refreshing} onRefresh={c.actions.refresh} />}
          onEndReached={c.hasMore ? c.actions.more : undefined}
          contentContainerStyle={{ paddingBottom: insets.bottom + space[16] }}
          renderItem={({ item }) =>
            item.kind === "header" ? (
              <Text variant="footnote" color="textMuted" emphasis accessibilityRole="header" style={{ paddingHorizontal: gutter, paddingTop: space[4], paddingBottom: space[1], textTransform: "uppercase", letterSpacing: 0.6 }}>
                {item.title}
              </Text>
            ) : (
              <View testID={`conversation-${item.row.conversation.id}`}>
                <ListItem title={item.row.title} subtitle={`${item.row.about} · ${item.row.when}`} onPress={() => c.actions.open(item.row.conversation.id)} />
              </View>
            )
          }
          ListEmptyComponent={
            <EmptyState
              title="No conversations yet"
              message="Ask about watering, frost, spacing or a scan's result. Answers cite their sources."
              action={{ label: "New chat", onPress: c.actions.newChat, variant: "secondary" }}
            />
          }
        />
      )}
    </View>
  );
}
