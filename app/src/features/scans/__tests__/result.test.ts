import { CITATIONS, PROBLEMS, ev, makeDiagnosis, makeScore, measureObs } from "@/api/mock/fixtures";
import type { Diagnosis, IndicatorEvidence } from "@/domain/types";

import { basisLine, evidenceRows, featureCards, header, measurementRows, riskWord, shownCandidate, soilRows } from "../model/result";

const ts = "2026-10-17T14:42:00Z";

function blight(over: Partial<Parameters<typeof makeDiagnosis>[0]> = {}): Diagnosis {
  const measured: IndicatorEvidence = { ...ev("lesion_size", "Spots", "yes", 2, "measurement"), measured: { image_id: "img_leaf", quantity: "lesion_diameter_mm", value: 9 } };
  const primary = makeScore(PROBLEMS.earlyBlight, 6, {
    matched: [ev("concentric_rings", "Concentric rings in the spots", "yes", 3, "user"), ev("lower_leaves_first", "Started on the lower leaves", "yes", 2), measured],
    contradicted: [ev("yellow_halo", "Yellow halo around the spots", "no", 1)],
  });
  return makeDiagnosis({
    id: "d1", scanId: "s1", ts, status: "confirmed", primary, candidates: [primary],
    risks: [{ risk: "fungal_spread", level: "high", drivers: ["4 humid days in the forecast", "Crowded: 40 cm from Tomato 1"] }],
    organisms: [{ name: "Ladybug larva", organism_id: "ladybug", role: "beneficial", observation_id: "o_lady", preys_on: [{ name: "Aphids", organism_id: "aphid" }] }],
    citations: [CITATIONS.blight],
    photo: ["Spots on the lower leaves"], modelled: ["Humid days"], user: ["The spots have rings"],
    ...over,
  });
}

describe("the diagnosis header", () => {
  it("names the confirmed problem with its basis, and no number anywhere", () => {
    const h = header("Tomato 2", blight(), 3);
    expect(h.headline).toBe("Early blight");
    expect(h.basis).toBe("Tomato 2 · from 3 photos, your answer and the weather");
    expect(JSON.stringify(h)).not.toMatch(/%|\b0\.\d/);
  });

  it("words an uncertain result as 'Not sure yet' with both candidates", () => {
    const d = blight({ status: "uncertain", primary: undefined, candidates: [makeScore(PROBLEMS.earlyBlight, 3), makeScore(PROBLEMS.septoria, 2)], user: [] });
    const h = header("Tomato 2", d, 3);
    expect(h.headline).toBe("Not sure yet: Early blight or Septoria leaf spot");
    expect(h.basis).toBe("Tomato 2 · from 3 photos and the weather");
  });

  it("names no problem on low evidence (screens 6)", () => {
    const d = blight({ status: "uncertain", lowEvidence: true });
    const h = header("Tomato 2", d, 1);
    expect(h.headline).toBe("We couldn't tell what's wrong from these photos.");
    expect(h.subline).toMatch(/Take a closer photo/);
    expect(shownCandidate(d)).toBeUndefined();
  });

  it("joins the basis for one source and none", () => {
    expect(basisLine("Basil", { evidence_basis: { photo: ["x"], modelled: [], user_input: [] } }, 1)).toBe("Basil · from 1 photo");
    expect(basisLine("Basil", { evidence_basis: { photo: [], modelled: [], user_input: [] } }, 1)).toBe("Basil");
  });
});

describe("the sideways cards", () => {
  it("leads the risk with the crowding driver, then the beneficial larva", () => {
    const [risk, larva] = featureCards(blight());
    expect(risk).toMatchObject({ eyebrow: "Risk of spread: high", title: "Crowded: 40 cm from Tomato 1", detail: "4 humid days in the forecast", tone: "watch" });
    expect(larva).toMatchObject({ eyebrow: "Also found", title: "Ladybug larva", detail: "Leave it: it eats aphids", tone: "healthy" });
  });

  it("reads an unknown risk slug as words", () => {
    expect(riskWord("leaf_scorch")).toBe("Leaf scorch");
  });
});

describe("the lists", () => {
  it("marks the LiDAR-answered indicator as measured, with the measured range, and the contradicted one", () => {
    const obs = [measureObs({ id: "o1", scanId: "s1", ts, quantity: "lesion_diameter_mm", value: 9, min: 8, max: 12, n: 3, distance: 0.4, imageId: "img_leaf" })];
    const rows = evidenceRows(shownCandidate(blight()), obs);
    expect(rows.map((r) => [r.text, r.source, r.measured, r.matched])).toEqual([
      ["Concentric rings in the spots", "Your answer", false, true],
      ["Started on the lower leaves", "The photos", false, true],
      ["8–12 mm spots", "LiDAR depth", true, true],
      ["Yellow halo around the spots", "Not seen", false, false],
    ]);
  });

  it("gives measurements a range and a band, never more precision than they carry", () => {
    const rows = measurementRows([
      measureObs({ id: "h", scanId: "s1", ts, quantity: "plant_height_cm", value: 56, min: 54, max: 58, n: 3, confidence: 0.86, imageId: "w" }),
      measureObs({ id: "c", scanId: "s1", ts, quantity: "lesion_count", value: 4, n: 3, imageId: "w" }),
    ]);
    expect(rows.map((r) => [r.label, r.value])).toEqual([["Height", "54–58 cm"], ["Spots", "4"]]);
    expect(rows[0]?.band).toBe("high");
  });

  it("lists the soil by aspect, never as a score", () => {
    const soil = [{ ...measureObs({ id: "s", scanId: "s1", ts, quantity: "plant_height_cm", value: 1, n: 1, imageId: "w" }), label: "Soil is moist", detail: { aspect: "moisture" as const, value: "moist" } }];
    expect(soilRows(soil).map((r) => [r.label, r.value])).toEqual([["Moisture", "Moist"]]);
  });
});
