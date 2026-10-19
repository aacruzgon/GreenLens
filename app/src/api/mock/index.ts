// Mock mode (docs/frontend/README.md 15). Wire `mockTransport` where the live transport goes
// when EXPO_PUBLIC_API_MODE=mock. The debug menu drives `scenario`; tests call `resetMock()`.

import { scenario } from "./scenario";
import { mockStore } from "./store";

export { mockTransport } from "./transport";
export { useScenario, scenario } from "./scenario";
export type { NextScan, UploadFault, CallFault, ScenarioState } from "./scenario";
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
