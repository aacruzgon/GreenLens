import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";

import { newKey, scans, setTransport, tasks } from "@/api";
import { IDS, mockStore, mockTransport, resetMock, scenario } from "@/api/mock";
import type { Question } from "@/domain/types";
import { ThemeProvider } from "@/ui";

import { GuidedCaptureScreen, QuestionCaptureScreen } from "../screens/OneShotScreen";

// The camera, the library and the upload parts are the platform's; the API is the mock's, with no delays.
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
    toUploadPart: jest.fn(async (uri: string) => ({ uri, name: uri.split("/").pop(), type: "image/jpeg" })),
    reencode: jest.fn(async (uri: string) => uri),
    remove: jest.fn(),
  },
}));
// jest-expo's expo-crypto stub returns no UUID; every action needs a real idempotency key.
jest.mock("expo-crypto", () => ({ randomUUID: () => jest.requireActual<{ randomUUID: () => string }>("crypto").randomUUID() }));
const mockRouter = { back: jest.fn(), replace: jest.fn(), push: jest.fn() };
let mockParams: Record<string, string> = {};
jest.mock("expo-router", () => ({
  useLocalSearchParams: () => mockParams,
  useRouter: () => mockRouter,
}));

async function newScan(): Promise<string> {
  return (await scans.create({ garden_id: IDS.garden, plant_id: IDS.tomato2 }, newKey())).id;
}

/** A submitted scan waiting on a photo request (flows 6). */
async function scanAskingForPhoto(): Promise<{ scanId: string; question: Question }> {
  const scanId = await newScan();
  const question: Question = {
    id: "q-underside",
    run: 1,
    kind: "photo_request",
    text: "Are there tiny spots under the leaves?",
    photo_kind: "leaf_underside",
    photo_hint: "Turn the leaf over and photograph the underside",
    candidates: ["spider_mites", "early_blight"],
    asked_at: "2026-10-21T10:00:00Z",
    continue_at: "2026-10-21T10:10:00Z",
    status: "open",
  };
  const st = mockStore.scan(scanId)!;
  st.scan.status = "awaiting_answer";
  st.scan.open_question = question;
  return { scanId, question };
}

async function renderScreen(Screen: () => React.ReactElement | null, params: Record<string, string>) {
  mockParams = params;
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  await render(
    <QueryClientProvider client={qc}>
      <ThemeProvider scheme="dark" reducedMotion>
        <Screen />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

const shutter = () => screen.getByRole("button", { name: /Take photo|Retake photo/ });
const send = () => screen.getByTestId("send");

beforeEach(() => {
  resetMock();
  scenario.set({ delayScale: 0 });
  setTransport(mockTransport);
  jest.clearAllMocks();
});

describe("question mode", () => {
  it("shows the question's hint, holds the photo for Send, then sends it with the question and opens the scan", async () => {
    const { scanId } = await scanAskingForPhoto();
    await renderScreen(QuestionCaptureScreen, { scanId, questionId: "q-underside" });
    expect(await screen.findByText("Turn the leaf over and photograph the underside")).toBeTruthy();
    expect(screen.getByText("Are there tiny spots under the leaves?")).toBeTruthy();
    expect(send().props.accessibilityState).toMatchObject({ disabled: true });

    await fireEvent.press(shutter());
    // Taken, not sent: nothing is stored until Send.
    expect(await screen.findByLabelText("The photo you took")).toBeTruthy();
    expect(mockStore.scan(scanId)!.scan.images).toHaveLength(0);
    expect(send().props.accessibilityState).toMatchObject({ disabled: false });

    await fireEvent.press(send());
    await waitFor(() => expect(mockRouter.replace).toHaveBeenCalledWith({ pathname: "/scans/[scanId]", params: { scanId } }));
    const scan = mockStore.scan(scanId)!.scan;
    expect(scan.images.map((i) => [i.kind, i.answers_question_id])).toEqual([["leaf_underside", "q-underside"]]);
    expect(scan.open_question?.status).toBe("answering");
  });

  it("lets the photo be retaken before Send", async () => {
    const { scanId } = await scanAskingForPhoto();
    await renderScreen(QuestionCaptureScreen, { scanId, questionId: "q-underside" });
    await fireEvent.press(await screen.findByRole("button", { name: "Take photo" }));
    await fireEvent.press(await screen.findByRole("button", { name: "Retake photo" }));
    expect(await screen.findByTestId("camera-preview")).toBeTruthy();
    expect(send().props.accessibilityState).toMatchObject({ disabled: true });
  });

  it("asks for another photo when the quality check refuses it", async () => {
    const { scanId } = await scanAskingForPhoto();
    await renderScreen(QuestionCaptureScreen, { scanId, questionId: "q-underside" });
    await fireEvent.press(await screen.findByRole("button", { name: "Take photo" }));
    scenario.set({ uploadFault: "image_quality" });
    await fireEvent.press(send());
    expect(await screen.findByText("This photo is too blurry. Hold the phone still and tap the leaf to focus.")).toBeTruthy();
    expect(screen.getByLabelText("Tap to focus")).toBeTruthy();
    expect(screen.getByTestId("camera-preview")).toBeTruthy();
    expect(mockStore.scan(scanId)!.scan.open_question?.status).toBe("open");
    expect(mockRouter.replace).not.toHaveBeenCalled();
  });

  it("says so when the question was answered meanwhile", async () => {
    const { scanId } = await scanAskingForPhoto();
    mockStore.scan(scanId)!.scan.open_question = undefined;
    await renderScreen(QuestionCaptureScreen, { scanId, questionId: "q-underside" });
    expect(await screen.findByText("This question has been answered.")).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "Close" }));
    expect(mockRouter.back).toHaveBeenCalled();
  });
});

describe("guided mode", () => {
  it("shows the squeeze test's steps, then the camera, and sends the photo as a new check", async () => {
    const scanId = await newScan();
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "squeeze_test" });
    expect(await screen.findByText("Squeeze it hard in your fist, then open your hand.")).toBeTruthy();
    expect(screen.queryByTestId("camera-preview")).toBeNull();

    await fireEvent.press(screen.getByRole("button", { name: "Start" }));
    expect(await screen.findByText("Photograph the soil in your open hand, from about 30 cm.")).toBeTruthy();
    await fireEvent.press(shutter());
    await fireEvent.press(await screen.findByTestId("send"));
    await waitFor(() => expect(mockRouter.replace).toHaveBeenCalled());
    expect(mockStore.scan(scanId)!.scan.guided_checks).toEqual([expect.objectContaining({ kind: "squeeze_test", status: "analysing" })]);
    // The squeeze test adds no task.
    expect(mockStore.tasks.filter((t) => t.scan_id === scanId)).toHaveLength(0);
  });

  it("adds the jar test's task on Start, with the scan, the plant and a due date", async () => {
    const scanId = await newScan();
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "jar_test" });
    expect(await screen.findByText(/Leave it to settle for at least 6 hours/)).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "Start" }));
    expect(await screen.findByTestId("jar-task-note")).toBeTruthy();
    const task = mockStore.tasks.find((t) => t.scan_id === scanId);
    expect(task).toMatchObject({ title: "Photograph the jar test", garden_id: IDS.garden, plant_id: IDS.tomato2, source: "user" });
    expect(task?.due).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it("goes straight to the camera when opened from the jar test's task", async () => {
    const scanId = await newScan();
    await tasks.create({ garden_id: IDS.garden, plant_id: IDS.tomato2, scan_id: scanId, title: "Photograph the jar test" }, newKey());
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "jar_test" });
    expect(await screen.findByTestId("camera-preview")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Start" })).toBeNull();
    await fireEvent.press(shutter());
    await fireEvent.press(await screen.findByTestId("send"));
    await waitFor(() => expect(mockStore.scan(scanId)!.scan.guided_checks).toHaveLength(1));
    // Only one task: Start wasn't run again.
    expect(mockStore.tasks.filter((t) => t.scan_id === scanId)).toHaveLength(1);
  });

  it("refuses a check on a failed scan", async () => {
    const scanId = await newScan();
    mockStore.scan(scanId)!.scan.status = "failed";
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "jar_test" });
    expect(await screen.findByText("This scan failed, so it can't take a soil check. Start a new scan.")).toBeTruthy();
  });
});
