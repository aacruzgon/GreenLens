import { confirmed, deletionSummary } from "./deletion";

describe("account deletion's wording (flows 13)", () => {
  it("needs the word typed", () => {
    expect(confirmed(" Delete ")).toBe(true);
    expect(confirmed("del")).toBe(false);
  });

  it("names the gardens and the new owners from the cache", () => {
    const gardens = new Map([["g1", "Back garden"], ["g2", "Allotment"], ["g3", "Roof"]]);
    const members = new Map([["sam", "Sam Lee"]]);
    expect(deletionSummary({ deleted_gardens: ["g1", "g3"], transferred_gardens: [{ garden_id: "g2", new_owner_user_id: "sam" }] }, gardens, members)).toBe(
      "2 gardens deleted. Allotment is now owned by Sam Lee. The rest finishes in the background.",
    );
    expect(deletionSummary({ deleted_gardens: ["g1"], transferred_gardens: [] }, gardens, members)).toBe("Back garden is deleted. The rest finishes in the background.");
  });
});
