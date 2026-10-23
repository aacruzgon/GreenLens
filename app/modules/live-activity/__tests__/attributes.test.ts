/// <reference types="node" />
// ActivityKit pairs the app's activity with the extension's widget by the attributes type, which each target
// compiles from its own copy. A field added to one copy only would decode to nothing on the Lock Screen.
import { readFileSync } from "fs";
import { join } from "path";

import * as LiveActivity from "..";

const root = join(__dirname, "..", "..", "..");

describe("ScanActivityAttributes", () => {
  it("is the same file in the module and the widget extension", () => {
    const moduleCopy = readFileSync(join(root, "modules/live-activity/ios/ScanActivityAttributes.swift"), "utf8");
    const extensionCopy = readFileSync(join(root, "targets/scan-activity/ScanActivityAttributes.swift"), "utf8");
    expect(extensionCopy).toBe(moduleCopy);
  });

  it("names the fields the JavaScript state sends", () => {
    const swift = readFileSync(join(root, "modules/live-activity/ios/ScanActivityAttributes.swift"), "utf8");
    for (const field of ["mode", "stage", "next", "segments", "startedAt", "total", "scanId", "plantName"]) {
      expect(swift).toMatch(new RegExp(`var ${field}: `));
    }
  });
});

describe("without the native module (Jest, Expo Go, the web)", () => {
  const state = { mode: "running" as const, stage: "Finding the plant", segments: [], startedAt: 0 };

  it("does nothing", async () => {
    expect(LiveActivity.isSupported()).toBe(false);
    await expect(LiveActivity.start({ scanId: "scan:1", plantName: "Tomato 1" }, state)).resolves.toBeUndefined();
    await expect(LiveActivity.update("x", state)).resolves.toBeUndefined();
    await expect(LiveActivity.end("x", state, { policy: "immediate" })).resolves.toBeUndefined();
  });
});
