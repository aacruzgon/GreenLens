import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { Alert } from "react-native";

import { newKey, scans, setTransport } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import { files } from "@/platform";
import { ThemeProvider } from "@/ui";

import { useDraftStore } from "../hooks/draftStore";
import { createDraft, takeShot } from "../model/draft";
import { CaptureScreen } from "../screens/CaptureScreen";

// The camera, the library and the file copies are the platform's; the API is the mock's, with no delays.
let mockShots = 0;
jest.mock("@/platform/capture", () => {
  const { forwardRef: fwd, useImperativeHandle: handle } = jest.requireActual<typeof import("react")>("react");
  const { View: V } = jest.requireActual<typeof import("react-native")>("react-native");
  return {
    CameraPreview: fwd(function CameraPreview(_props: object, ref: React.Ref<unknown>) {
      handle(ref, () => ({
        capture: async () => ({ file: { uri: `file:///cache/shot-${++mockShots}.jpg`, mimeType: "image/jpeg", width: 4032, height: 3024 } }),
      }));
      return <V testID="camera-preview" />;
    }),
    pickFromLibrary: jest.fn(async () => null),
    useCameraAccess: () => ({ status: "granted" }),
  };
});
jest.mock("@/platform/files", () => ({
  files: {
    keep: jest.fn((scanId: string, _uri: string, name: string) => `file:///documents/capture/${scanId}/${name}`),
    toUploadPart: jest.fn(async (uri: string) => ({ uri, name: uri.split("/").pop(), type: "image/jpeg" })),
    reencode: jest.fn(async (uri: string) => uri),
    remove: jest.fn(),
    drop: jest.fn(),
    dropAll: jest.fn(),
  },
}));
// jest-expo's expo-crypto stub returns no UUID; every shot needs a real idempotency key.
jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));
const mockRouter = { back: jest.fn(), replace: jest.fn(), push: jest.fn(), dismissAll: jest.fn() };
let mockParams: { scanId: string } = { scanId: "" };
jest.mock("expo-router", () => ({
  useLocalSearchParams: () => mockParams,
  useRouter: () => mockRouter,
}));

async function newScan(): Promise<string> {
  const scan = await scans.create({ garden_id: IDS.garden, plant_id: IDS.tomato2 }, newKey());
  return scan.id;
}

async function renderCapture(scanId: string) {
  mockParams = { scanId };
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="dark" reducedMotion>
        <CaptureScreen />
      </ThemeProvider>
    </QueryClientProvider>,
  );
  await screen.findByText("Scan · Tomato 2");
}

const draftOf = (scanId: string) => useDraftStore.getState().drafts[scanId];
const shutter = () => screen.getByRole("button", { name: /Take photo|Retake photo/ });
const analyse = () => screen.getByTestId("analyse");

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  useDraftStore.setState({ drafts: {} });
  jest.clearAllMocks();
});

describe("CaptureScreen", () => {
  it("takes the whole plant, uploads it, fills the dial and enables Analyse", async () => {
    const id = await newScan();
    await renderCapture(id);
    expect(screen.getByText("Step back about 1 m and get the whole plant in the frame.")).toBeTruthy();
    expect(screen.getByText("0 of 2 needed shots")).toBeTruthy();
    expect(screen.getByText(/Start with the whole plant/)).toBeTruthy();
    expect(analyse().props.accessibilityState).toMatchObject({ disabled: true });

    await fireEvent.press(shutter());
    await waitFor(() => expect(screen.getByRole("tab", { name: "Whole plant, taken" })).toBeTruthy());
    expect(screen.getByText("1 of 2 needed shots")).toBeTruthy();
    expect(analyse().props.accessibilityState).toMatchObject({ disabled: false });
    // The draft holds the kept copy and the image the server stored.
    const slot = draftOf(id)!.slots.find((s) => s.id === "whole_plant:0")!;
    expect(slot).toMatchObject({ state: "stored", file: { uri: expect.stringContaining(`/capture/${id}/whole_plant-0-`) } });
    expect(mockStore.scan(id)!.scan.images.map((i) => i.kind)).toEqual(["whole_plant"]);
  });

  it("switches modes from the bar and shows the chosen shot's instruction", async () => {
    await renderCapture(await newScan());
    await fireEvent.press(screen.getByRole("tab", { name: "Close-up" }));
    expect(screen.getByRole("tab", { name: "Close-up" }).props.accessibilityState).toMatchObject({ selected: true });
    expect(screen.getByText("Fill the frame with one affected leaf, about 30 cm away. Tap the leaf to focus.")).toBeTruthy();
  });

  it("shows a blurry shot's message with its hint, and the retake uploads", async () => {
    const id = await newScan();
    await renderCapture(id);
    scenario.set({ uploadFault: "image_quality" });
    await fireEvent.press(shutter());
    expect(await screen.findByText("This photo is too blurry. Hold the phone still and tap the leaf to focus.")).toBeTruthy();
    expect(screen.getByLabelText("Tap to focus")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Whole plant, needs a retake" })).toBeTruthy();

    await fireEvent.press(shutter());
    await waitFor(() => expect(screen.getByRole("tab", { name: "Whole plant, taken" })).toBeTruthy());
    expect(screen.queryByTestId("shot-problem")).toBeNull();
    expect(mockStore.scan(id)!.scan.images).toHaveLength(1);
  });

  it("submits on Analyse, drops the draft with its photos and opens the scan", async () => {
    const id = await newScan();
    await renderCapture(id);
    await fireEvent.press(shutter());
    await waitFor(() => expect(analyse().props.accessibilityState).toMatchObject({ disabled: false }));
    await fireEvent.press(analyse());
    await waitFor(() => expect(mockRouter.replace).toHaveBeenCalledWith({ pathname: "/scans/[scanId]", params: { scanId: id } }));
    expect(draftOf(id)).toBeUndefined();
    expect(files.drop).toHaveBeenCalledWith(id);
    expect(mockStore.scan(id)!.scan.status).not.toBe("pending");
  });

  it("asks before Discard, then deletes the scan and its draft", async () => {
    const alert = jest.spyOn(Alert, "alert");
    const id = await newScan();
    await renderCapture(id);
    await fireEvent.press(screen.getByRole("button", { name: "Discard" }));
    expect(alert).toHaveBeenCalledWith("Discard this scan?", "Its photos will be deleted.", expect.any(Array));
    const buttons = alert.mock.calls[0]![2]!;
    await act(async () => buttons.find((b) => b.style === "destructive")!.onPress!());
    await waitFor(() => expect(mockRouter.back).toHaveBeenCalled());
    expect(mockStore.scan(id)).toBeUndefined();
    expect(draftOf(id)).toBeUndefined();
  });

  it("resumes a draft left by a closed app: the shot uploads under its saved key", async () => {
    const id = await newScan();
    const scan = mockStore.scan(id)!.scan;
    const file = { uri: `file:///documents/capture/${id}/w.jpg`, name: "w.jpg", type: "image/jpeg", picked: false, width: 4032, height: 3024 };
    useDraftStore.getState().put(takeShot(createDraft(scan), "whole_plant", file, "key-from-before"));
    const upload = jest.spyOn(scans, "uploadImage");
    await renderCapture(id);
    await waitFor(() => expect(screen.getByRole("tab", { name: "Whole plant, taken" })).toBeTruthy());
    expect(upload).toHaveBeenCalledWith(id, expect.objectContaining({ kind: "whole_plant" }), "key-from-before");
  });

  it("keeps the draft when ✕ closes capture", async () => {
    const id = await newScan();
    await renderCapture(id);
    await fireEvent.press(screen.getByRole("button", { name: "Close" }));
    expect(mockRouter.back).toHaveBeenCalled();
    expect(draftOf(id)).toBeDefined();
  });
});
