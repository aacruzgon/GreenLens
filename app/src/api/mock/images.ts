// Mock-mode photos. Contract URLs are strings, so fixtures use `mock://images/<name>.jpg`;
// a screen resolves one to a bundled module id with `resolveMockImage(url)` and passes the
// result to `<Image source={...}>` (expo-image accepts the same module id).
// Photos and credits: app/assets/mock/CREDITS.md.

export type MockImageName = "tomato" | "tomato-blight" | "pepper" | "basil" | "lettuce" | "kale";

export const MOCK_IMAGE_PREFIX = "mock://images/";

export function mockImageUrl(name: MockImageName): string {
  return `${MOCK_IMAGE_PREFIX}${name}.jpg`;
}

/** Pixel sizes of the bundled files (for ScanImage.width and height). */
export const MOCK_IMAGE_SIZE: Record<MockImageName, { width: number; height: number }> = {
  tomato: { width: 404, height: 900 },
  "tomato-blight": { width: 900, height: 602 },
  pepper: { width: 900, height: 900 },
  basil: { width: 900, height: 675 },
  lettuce: { width: 900, height: 900 },
  kale: { width: 900, height: 675 },
};

/** Credit lines for the reference photos (full table in CREDITS.md). */
export const MOCK_IMAGE_CREDIT: Record<MockImageName, string> = {
  tomato: "Clintacc, CC0, Wikimedia Commons",
  "tomato-blight": "Dwight Sipler, CC BY 2.0, Wikimedia Commons",
  pepper: "Jeff Kwapil, CC BY-SA 4.0, Wikimedia Commons",
  basil: "Vitaium, CC BY-SA 4.0, Wikimedia Commons",
  lettuce: "DeFacto, CC BY-SA 4.0, Wikimedia Commons",
  kale: "Loadmaster (David R. Tribble), CC BY-SA 3.0, Wikimedia Commons",
};

// Metro resolves each require() at bundle time, so every file is named literally.
const MODULES: Record<MockImageName, number> = {
  tomato: require("../../../assets/mock/tomato.jpg"),
  "tomato-blight": require("../../../assets/mock/tomato-blight.jpg"),
  pepper: require("../../../assets/mock/pepper.jpg"),
  basil: require("../../../assets/mock/basil.jpg"),
  lettuce: require("../../../assets/mock/lettuce.jpg"),
  kale: require("../../../assets/mock/kale.jpg"),
};

/** Fixture URL -> bundled module id. */
export const MOCK_IMAGES: Record<string, number> = Object.fromEntries(
  (Object.keys(MODULES) as MockImageName[]).map((name) => [mockImageUrl(name), MODULES[name]]),
);

export function isMockImageUrl(url: string | undefined | null): boolean {
  return typeof url === "string" && url.startsWith(MOCK_IMAGE_PREFIX);
}

/** The module id for a `mock://images/...` URL, or undefined for any other URL. */
export function resolveMockImage(url: string | undefined | null): number | undefined {
  return url ? MOCK_IMAGES[url] : undefined;
}

export function imageName(url: string): MockImageName | undefined {
  const name = url.startsWith(MOCK_IMAGE_PREFIX) ? url.slice(MOCK_IMAGE_PREFIX.length, -4) : "";
  return name in MODULES ? (name as MockImageName) : undefined;
}
