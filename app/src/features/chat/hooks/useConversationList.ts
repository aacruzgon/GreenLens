// The Chat tab (screens 3.6): the caller's conversations in the active garden, grouped by anchor, and New chat,
// which always starts a conversation anchored to the garden.
import { useRouter } from "expo-router";
import { useRef, useState } from "react";

import { flattenPages, isApiError, useAllPlants, useConversations, useCreateConversation, useGarden } from "@/api";
import { formatDay, formatMonthDay } from "@/domain/format";
import type { Conversation } from "@/domain/types";

import { anchorName, conversationTitle, groupConversations, type AnchorNames } from "../model/conversations";

export interface ConversationRow {
  conversation: Conversation;
  title: string;
  about: string;
  when: string;
}

export function useConversationList(gardenId: string) {
  const router = useRouter();
  const list = useConversations(gardenId);
  const garden = useGarden(gardenId);
  const plants = useAllPlants(gardenId);
  const create = useCreateConversation();
  const [problem, setProblem] = useState<string>();
  const starting = useRef(false);

  const tz = garden.data?.time_zone ?? "UTC";
  const plantName = new Map((plants.data ?? []).map((p) => [p.id, p.name]));
  // A scan anchor is named after its plant when the plant's latest scan is that scan; otherwise "A scan".
  const scanPlant = new Map((plants.data ?? []).flatMap((p) => (p.latest_scan ? [[p.latest_scan.scan_id, { name: p.name, at: p.latest_scan.created_at }] as const] : [])));
  const namesFor = (c: Conversation): AnchorNames => ({
    gardenName: garden.data?.name,
    plantName: c.anchor.type === "plant" ? plantName.get(c.anchor.id) : undefined,
    scanPlantName: c.anchor.type === "scan" ? scanPlant.get(c.anchor.id)?.name : undefined,
    scanDate: c.anchor.type === "scan" && scanPlant.get(c.anchor.id) ? formatMonthDay(scanPlant.get(c.anchor.id)!.at, tz) : undefined,
  });

  const all = flattenPages(list.data);
  // A conversation nobody wrote in is left out: New chat, or Ask then Back, would otherwise leave an empty row each
  // time (seen on the simulator). New chat reuses one instead of starting another.
  const used = all.filter((c) => c.last_message_at !== undefined);
  const groups = groupConversations(used).map((g) => ({
    ...g,
    rows: g.items.map((c): ConversationRow => ({
      conversation: c,
      title: conversationTitle(c, namesFor(c)),
      about: anchorName(c.anchor, namesFor(c)),
      when: formatDay(c.last_message_at ?? c.created_at, tz),
    })),
  }));

  const newChat = async () => {
    if (starting.current || !gardenId) return;
    starting.current = true;
    setProblem(undefined);
    try {
      const empty = all.find((c) => c.anchor.type === "garden" && c.last_message_at === undefined);
      const c = empty ?? (await create.mutateAsync({ garden_id: gardenId, anchor: { type: "garden", id: gardenId } }));
      router.push({ pathname: "/chat/[conversationId]", params: { conversationId: c.id } });
    } catch (e) {
      setProblem(isApiError(e) ? e.message : "A new chat couldn't start. Check your network.");
    } finally {
      starting.current = false;
    }
  };

  return {
    loading: list.isPending,
    error: list.isError ? "Your conversations couldn't be loaded." : undefined,
    refreshing: list.isRefetching && !list.isFetchingNextPage,
    groups,
    hasMore: list.hasNextPage,
    starting: create.isPending,
    problem,
    actions: {
      open: (id: string) => router.push({ pathname: "/chat/[conversationId]", params: { conversationId: id } }),
      newChat: () => void newChat(),
      refresh: () => void list.refetch(),
      more: () => void list.fetchNextPage(),
    },
  };
}
