import type { CandidateScore } from "./types";
import { wordDiagnosis, wordOrganism } from "./diagnosis";
import { can } from "./permissions";
import { isAtRest } from "./scan";

describe("can", () => {
  it("follows the role matrix of contracts 1", () => {
    expect(can("viewer", "scan.create")).toBe(false);
    expect(can("reviewer", "scan.feedback")).toBe(true);
    expect(can("reviewer", "scan.answer")).toBe(false);
    expect(can("editor", "scan.create")).toBe(true);
    expect(can("editor", "plant.delete")).toBe(false);
    expect(can("owner", "members.manage")).toBe(true);
    expect(can(undefined, "scan.create")).toBe(false);
  });
});

describe("isAtRest", () => {
  it("is at rest when planned with no question and no running check", () => {
    expect(isAtRest({ status: "planned", guided_checks: [] })).toBe(true);
    expect(isAtRest({ status: "planned", open_question: { status: "open" }, guided_checks: [] })).toBe(false);
    expect(isAtRest({ status: "failed", guided_checks: [{ status: "analysing" }] })).toBe(false);
    expect(isAtRest({ status: "analysing", guided_checks: [] })).toBe(false);
  });
});

describe("wordDiagnosis (screens.md 6)", () => {
  const c = (name: string): CandidateScore => ({ problem_id: name, name, kind: "fungal", score: 1, matched: [], contradicted: [], unsure: [], weather_fit: "neutral" });

  it("names no problem and gives no number with low evidence", () => {
    const w = wordDiagnosis({ status: "uncertain", low_evidence: true, candidates: [c("Early blight")] });
    expect(w.named).toEqual([]);
    expect(w.headline).not.toMatch(/\d|Early blight/);
  });

  it("offers the top two when uncertain", () => {
    expect(wordDiagnosis({ status: "uncertain", low_evidence: false, candidates: [c("Early blight"), c("Septoria leaf spot")] }).headline).toBe(
      "Not sure yet: Early blight or Septoria leaf spot",
    );
  });

  it("says looks healthy, not fine", () => {
    expect(wordDiagnosis({ status: "healthy", low_evidence: false, candidates: [] }).headline).toBe("Looks healthy");
  });

  it("never suggests removing a beneficial organism", () => {
    expect(wordOrganism({ name: "Ladybug larva", role: "beneficial", preys_on: [{ organism_id: "aphid", name: "Aphids" }] })).toBe(
      "Ladybug larva: leave it, it eats aphids",
    );
  });
});
