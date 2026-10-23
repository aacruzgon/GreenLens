import type { DepthMeta } from "@/domain/depth";

import { shotUpload } from "../hooks/shotUpload";
import type { ShotFile } from "../model/slot";

jest.mock("@/platform", () => ({
  files: { toUploadPart: jest.fn(async (uri: string) => ({ uri, name: uri.split("/").pop(), type: uri.endsWith(".png") ? "image/png" : "image/jpeg" })) },
}));

const meta: DepthMeta = {
  source: "lidar",
  intrinsics: { fx: 2700, fy: 2700, cx: 2016, cy: 1512, ref_width: 4032, ref_height: 3024 },
  distortion: { center: [2016, 1512], inverse_lookup: [0, 0.01] },
  gravity: [0, 1, 0],
  device_model: "iPhone18,2",
  filtered: false,
};

const file = (depth: boolean): ShotFile => ({
  uri: "file:///docs/capture/s1/leaf-0.jpg",
  name: "leaf-0.jpg",
  type: "image/jpeg",
  picked: false,
  width: 3024,
  height: 4032,
  ...(depth ? { depth: { uri: "file:///docs/capture/s1/leaf-0-depth.png", name: "leaf-0-depth.png", meta } } : {}),
});

describe("a shot's multipart parts (contracts 3)", () => {
  it("sends the photo alone without depth", async () => {
    expect(await shotUpload("leaf_closeup", file(false))).toEqual({
      kind: "leaf_closeup",
      file: { uri: "file:///docs/capture/s1/leaf-0.jpg", name: "leaf-0.jpg", type: "image/jpeg" },
    });
  });

  it("adds the depth PNG and its meta, without the distortion table", async () => {
    const part = await shotUpload("whole_plant", file(true));
    expect(part.depth).toEqual({ uri: "file:///docs/capture/s1/leaf-0-depth.png", name: "leaf-0-depth.png", type: "image/png" });
    expect(part.depthMeta).toEqual({ source: "lidar", intrinsics: meta.intrinsics, gravity: [0, 1, 0], device_model: "iPhone18,2", filtered: false });
  });

  it("leaves depth out of a guided check, and carries the question", async () => {
    const part = await shotUpload("jar_test", file(true), "q_1");
    expect(part.depth).toBeUndefined();
    expect(part.depthMeta).toBeUndefined();
    expect(part.questionId).toBe("q_1");
  });
});
