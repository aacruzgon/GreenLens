import { addShot, isCaseId, shotNames } from "./evalCase";

describe("evaluation captures", () => {
  it("names each shot's four files after its place and kind", () => {
    expect(shotNames(2, "leaf_closeup", true)).toEqual({
      file: "03-leaf_closeup.jpg",
      kind: "leaf_closeup",
      depth: "03-leaf_closeup-depth.png",
      depth_meta: "03-leaf_closeup-depth_meta.json",
      report: "03-leaf_closeup-report.json",
    });
    expect(shotNames(0, "whole_plant", false)).toEqual({ file: "01-whole_plant.jpg", kind: "whole_plant", report: "01-whole_plant-report.json" });
  });

  it("adds shots to a saved case and keeps its plant", () => {
    const first = addShot(undefined, "tomato-0007", "tomato-a", "whole_plant", true, "iPhone18,2", "t1");
    const second = addShot(first, "tomato-0007", "", "leaf_closeup", true, undefined, "t2");
    expect(second.plant_key).toBe("tomato-a");
    expect(second.device_model).toBe("iPhone18,2");
    expect(second.images.map((i) => i.file)).toEqual(["01-whole_plant.jpg", "02-leaf_closeup.jpg"]);
  });

  it("takes the manifest's case IDs only", () => {
    expect(isCaseId("tomato-0007")).toBe(true);
    expect(isCaseId("Tomato 7")).toBe(false);
    expect(isCaseId("../x")).toBe(false);
  });
});
