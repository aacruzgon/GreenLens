// The Chat tab's list, grouped by anchor (screens 3.6): the garden, a plant, a scan; each group newest first, and
// the groups in the order of their newest conversation.
import type { Anchor, AnchorType, Conversation } from "@/domain/types";

export interface ConversationGroup {
  key: AnchorType;
  title: string;
  items: Conversation[];
}

const GROUP_TITLE: Record<AnchorType, string> = {
  garden: "About the garden",
  plant: "About a plant",
  scan: "About a scan",
};

const latest = (c: Conversation) => c.last_message_at ?? c.created_at;

export function groupConversations(conversations: readonly Conversation[]): ConversationGroup[] {
  const groups = new Map<AnchorType, Conversation[]>();
  for (const c of conversations) {
    const list = groups.get(c.anchor.type) ?? [];
    list.push(c);
    groups.set(c.anchor.type, list);
  }
  return [...groups.entries()]
    .map(([key, items]) => ({ key, title: GROUP_TITLE[key], items: [...items].sort((a, b) => latest(b).localeCompare(latest(a))) }))
    .sort((a, b) => latest(b.items[0]!).localeCompare(latest(a.items[0]!)));
}

/** What the screen knows about an anchor's records, to name it. */
export interface AnchorNames {
  gardenName?: string;
  plantName?: string;
  /** For a scan anchor: its plant's name and the scan's date, already formatted ("Oct 17"). */
  scanPlantName?: string;
  scanDate?: string;
}

/** "Back garden", "Tomato 1", "Tomato 2, Oct 17 scan" (screens 3.6: "About: Tomato 1, Oct 18 scan"). */
export function anchorName(anchor: Anchor, names: AnchorNames): string {
  switch (anchor.type) {
    case "garden":
      return names.gardenName ?? "Your garden";
    case "plant":
      return names.plantName ?? "A plant";
    case "scan":
      if (names.scanPlantName && names.scanDate) return `${names.scanPlantName}, ${names.scanDate} scan`;
      return names.scanPlantName ? `${names.scanPlantName}, a scan` : "A scan";
  }
}

/** A row's title: the conversation's own, or what it is about. */
export function conversationTitle(c: Conversation, names: AnchorNames): string {
  return c.title?.trim() || `About ${anchorName(c.anchor, names)}`;
}

/** "A, B and C". */
export function nameList(names: readonly string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
}

/**
 * Follow-up questions under the conversation (the design's mockup, screens 3.6), by what it is about, leaving out
 * ones already asked. Fixed wording the app owns: they name no diagnosis, and a tap sends the question as is.
 */
const SUGGESTIONS: Record<AnchorType, readonly string[]> = {
  garden: ["Should I water today?", "Should I cover anything this week?", "Are any plants too close together?"],
  plant: ["How has it grown?", "Is it too close to its neighbours?", "Should I water today?"],
  scan: ["How do I tell it from look-alikes?", "Should I water today?", "Is it too close to its neighbours?"],
};

export function suggestions(anchor: AnchorType, asked: readonly string[], max = 2): string[] {
  const seen = new Set(asked.map((t) => t.trim().toLowerCase()));
  return SUGGESTIONS[anchor].filter((q) => !seen.has(q.toLowerCase())).slice(0, max);
}
