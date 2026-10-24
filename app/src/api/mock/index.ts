// Mock mode (docs/frontend/README.md 15). Wire `mockTransport` where the live transport goes
// when EXPO_PUBLIC_API_MODE=mock. The debug menu drives `scenario`; tests call `resetMock()`.

import { scenario, type DemoAccount } from "./scenario";
import { mockStore } from "./store";

export { mockTransport } from "./transport";
export { useScenario, scenario } from "./scenario";
export type { NextScan, UploadFault, CallFault, ChatFault, DemoAccount, ScenarioState } from "./scenario";
export { mockStore } from "./store";
export { IDS } from "./fixtures";
export type { PlantKey } from "./fixtures";
export {
  isMockImageUrl,
  MOCK_IMAGE_CREDIT,
  MOCK_IMAGE_PREFIX,
  MOCK_IMAGE_SIZE,
  MOCK_IMAGES,
  mockImageUrl,
  resolveMockImage,
} from "./images";
export type { MockImageName } from "./images";

/** Re-seeds the demo story (dates relative to now) and clears the scenario. */
export function resetMock(now: Date = new Date()): void {
  mockStore.reset(now);
  scenario.reset();
}

/**
 * Switches the demo controls' account. "new" empties the account so onboarding shows; "demo" re-seeds the demo
 * story, since a new account's first garden cleared it (POST /gardens). Choosing the account already in use
 * changes nothing, so scans made in the demo survive a stray tap. The caller refetches every query after.
 */
export function chooseAccount(account: DemoAccount, now: Date = new Date()): void {
  if (scenario.get().account === account) return;
  if (account === "demo") mockStore.reset(now);
  scenario.set({ account, emptyAccount: account === "new" });
}
