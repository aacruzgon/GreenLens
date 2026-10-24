import type { Conversation } from "@/domain/types";

import { anchorName, conversationTitle, groupConversations, nameList, suggestions } from "./conversations";

const conv = (id: string, type: "garden" | "plant" | "scan", at: string, title?: string): Conversation => ({
  id,
  garden_id: "g",
  anchor: { type, id: `${type}_1` } as Conversation["anchor"],
  created_at: "2026-10-01T10:00:00Z",
  last_message_at: at,
  ...(title ? { title } : {}),
});

describe("the conversation list (screens 3.6)", () => {
  it("groups by anchor, newest first, groups ordered by their newest", () => {
    const groups = groupConversations([
      conv("a", "garden", "2026-10-20T10:00:00Z"),
      conv("b", "plant", "2026-10-22T10:00:00Z"),
      conv("c", "garden", "2026-10-23T10:00:00Z"),
      conv("d", "scan", "2026-10-21T10:00:00Z"),
    ]);
    expect(groups.map((g) => g.key)).toEqual(["garden", "plant", "scan"]);
    expect(groups[0]!.items.map((c) => c.id)).toEqual(["c", "a"]);
    expect(groups[0]!.title).toBe("About the garden");
  });

  it("names an anchor from its records", () => {
    expect(anchorName({ type: "scan", id: "s" }, { scanPlantName: "Tomato 1", scanDate: "Oct 18" })).toBe("Tomato 1, Oct 18 scan");
    expect(anchorName({ type: "scan", id: "s" }, {})).toBe("A scan");
    expect(anchorName({ type: "plant", id: "p" }, { plantName: "Basil" })).toBe("Basil");
    expect(anchorName({ type: "garden", id: "g" }, { gardenName: "Back garden" })).toBe("Back garden");
  });

  it("titles a row with its own title, else what it is about", () => {
    expect(conversationTitle(conv("a", "plant", "2026-10-20T10:00:00Z", "Is it growing?"), {})).toBe("Is it growing?");
    expect(conversationTitle(conv("a", "plant", "2026-10-20T10:00:00Z"), { plantName: "Basil" })).toBe("About Basil");
  });

  it("lists names", () => {
    expect(nameList([])).toBe("");
    expect(nameList(["A"])).toBe("A");
    expect(nameList(["A", "B", "C"])).toBe("A, B and C");
  });
});

describe("suggested follow-ups", () => {
  it("offers two by anchor, leaving out what was asked", () => {
    expect(suggestions("garden", [])).toEqual(["Should I water today?", "Should I cover anything this week?"]);
    expect(suggestions("garden", ["should I water today? "])).toEqual(["Should I cover anything this week?", "Are any plants too close together?"]);
  });
});
