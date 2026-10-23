import { createDraft, fileUris, reconcileDraft, relocateDraft } from "./draft";
import { sendsDepth, uploadMeta } from "./depth";
import { hasDistanceRange } from "./distanceRanges";
import type { ShotFile } from "./slot";

const meta = { source: "lidar", device_model: "iPhone18,2", filtered: false, distortion: { center: [1, 2], inverse_lookup: [0, 0.1] } };
const file: ShotFile = {
  uri: "file:///old/capture/s1/whole_plant-0-1.jpg",
  name: "whole_plant-0-1.jpg",
  type: "image/jpeg",
  picked: false,
  width: 3024,
  height: 4032,
  depth: { uri: "file:///old/capture/s1/whole_plant-0-1-depth.png", name: "whole_plant-0-1-depth.png", meta },
};

describe("depth parts (contracts 3)", () => {
  it("keeps the distortion table on the phone until the day-1 check asks for it", () => {
    expect(uploadMeta(meta)).toEqual({ source: "lidar", device_model: "iPhone18,2", filtered: false });
    expect(uploadMeta(meta, true)).toBe(meta);
  });

  it("sends depth with plant and soil kinds, never with a guided check", () => {
    expect(sendsDepth("leaf_closeup")).toBe(true);
    expect(sendsDepth("soil_surface")).toBe(true);
    expect(sendsDepth("jar_test")).toBe(false);
    expect(sendsDepth("squeeze_test")).toBe(false);
  });

  it("gives the distance chip only to capture kinds", () => {
    expect(hasDistanceRange("leaf_underside")).toBe(true);
    expect(hasDistanceRange("squeeze_test")).toBe(false);
  });
});

describe("a shot's depth map in the draft", () => {
  const draft = () => {
    const d = createDraft({ id: "s1", garden_id: "g", plant_id: "p" });
    return { ...d, slots: d.slots.map((s) => (s.id === "whole_plant:0" ? { ...s, state: "stored" as const, imageId: "img1", file } : s)) };
  };

  it("moves with the photo when the app's folder moves", () => {
    const moved = relocateDraft(draft(), (name) => `file:///new/capture/s1/${name}`);
    const slot = moved.slots.find((s) => s.id === "whole_plant:0");
    expect(slot?.file?.uri).toBe("file:///new/capture/s1/whole_plant-0-1.jpg");
    expect(slot?.file?.depth?.uri).toBe("file:///new/capture/s1/whole_plant-0-1-depth.png");
    expect(slot?.file?.depth?.meta).toBe(meta);
  });

  it("is deleted with its photo when the server no longer has the shot", () => {
    const { unused } = reconcileDraft(draft(), { images: [] }, new Set(["img1"]));
    expect(unused).toEqual(fileUris(file));
    expect(unused).toHaveLength(2);
  });
});
