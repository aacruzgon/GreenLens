import { isBlackShot } from "@/platform/capture";

import { pauseText, shotFailedText } from "../model/interruption";

describe("what the camera screens say", () => {
  it("names a black shot, which wasn't kept, apart from any other failure", () => {
    expect(isBlackShot(Object.assign(new Error("The photo came out black."), { code: "ERR_LIDAR_BLACK" }))).toBe(true);
    expect(isBlackShot(Object.assign(new Error("The camera stopped. Try again."), { code: "ERR_LIDAR_CAPTURE" }))).toBe(false);
    expect(isBlackShot(undefined)).toBe(false);
    expect(shotFailedText(true)).toMatch(/black, so it wasn't kept/);
    expect(shotFailedText(false)).toBe("Couldn't take the photo. Try again.");
  });

  it("says why the camera is paused", () => {
    expect(pauseText("in_use")).toMatch(/Another app/);
    expect(pauseText("background")).toBe("The camera is paused. It comes back on its own.");
  });
});
