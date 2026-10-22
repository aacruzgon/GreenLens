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
const mockRouter = { canGoBack: jest.fn(() => true), back: jest.fn(), replace: jest.fn(), push: jest.fn() };
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

  it("says so when the question is no longer open", async () => {
    const { scanId } = await scanAskingForPhoto();
    mockStore.scan(scanId)!.scan.open_question = undefined;
    await renderScreen(QuestionCaptureScreen, { scanId, questionId: "q-underside" });
    expect(await screen.findByText("This question is no longer open.")).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "Close" }));
    expect(mockRouter.back).toHaveBeenCalled();
  });
});

it("closes to the garden home when it was opened directly, with nothing to go back to", async () => {
  const scanId = await newScan();
  mockRouter.canGoBack.mockReturnValue(false);
  try {
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "squeeze_test" });
    await fireEvent.press(await screen.findByRole("button", { name: "Close" }));
    expect(mockRouter.back).not.toHaveBeenCalled();
    expect(mockRouter.replace).toHaveBeenCalledWith("/");
  } finally {
    mockRouter.canGoBack.mockReturnValue(true);
  }
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

  it("doesn't take a plan's task with a scan link for the jar test's", async () => {
    const scanId = await newScan();
    mockStore.tasks.push({ id: "task_plan", garden_id: IDS.garden, plant_id: IDS.tomato2, scan_id: scanId, source: "plan", title: "Check Tomato 2's lower leaves", created_at: "2026-10-21T10:00:00Z" });
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "jar_test" });
    await fireEvent.press(await screen.findByRole("button", { name: "Start" }));
    await waitFor(() => expect(mockStore.tasks.filter((t) => t.scan_id === scanId && t.source === "user")).toHaveLength(1));
  });

  it("finds the jar test's task beyond the first page of tasks", async () => {
    const scanId = await newScan();
    for (let i = 0; i < 45; i++) {
      await tasks.create({ garden_id: IDS.garden, title: `Water bed ${i}`, due: "2026-10-01" }, newKey());
    }
    await tasks.create({ garden_id: IDS.garden, plant_id: IDS.tomato2, scan_id: scanId, title: "Photograph the jar test", due: "2026-12-31" }, newKey());
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "jar_test" });
    expect(await screen.findByTestId("camera-preview", {}, { timeout: 5000 })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Start" })).toBeNull();
  });

  it("retries Start with the same body and key after a lost response", async () => {
    const scanId = await newScan();
    const posts: { key: string | null; body: string }[] = [];
    let failNext = true;
    setTransport({
      fetch: async (input, init) => {
        const req = new Request(input, init);
        if (req.method === "POST" && new URL(req.url).pathname.endsWith("/tasks")) {
          posts.push({ key: req.headers.get("Idempotency-Key"), body: await req.clone().text() });
          if (failNext) {
            failNext = false;
            throw new TypeError("Network request failed");
          }
        }
        return mockTransport.fetch(req);
      },
    });
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "jar_test" });
    await fireEvent.press(await screen.findByRole("button", { name: "Start" }));
    await fireEvent.press(await screen.findByRole("button", { name: "Try again" }));
    expect(await screen.findByTestId("jar-task-note")).toBeTruthy();
    expect(posts).toHaveLength(2);
    expect(posts[1]).toEqual(posts[0]);
    expect(JSON.parse(posts[0]!.body)).toMatchObject({ scan_id: scanId, due: expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/) });
    expect(mockStore.tasks.filter((t) => t.scan_id === scanId)).toHaveLength(1);
  });

  it("keeps ✕ off while the photo is sending", async () => {
    const scanId = await newScan();
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => (release = resolve));
    setTransport({
      fetch: async (input, init) => {
        const req = new Request(input, init);
        if (req.method === "POST" && new URL(req.url).pathname.endsWith("/images")) await held;
        return mockTransport.fetch(req);
      },
    });
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "squeeze_test" });
    await fireEvent.press(await screen.findByRole("button", { name: "Start" }));
    await fireEvent.press(shutter());
    await fireEvent.press(await screen.findByTestId("send"));
    const close = screen.getByRole("button", { name: "Close" });
    expect(close.props.accessibilityState).toMatchObject({ disabled: true });
    release();
    await waitFor(() => expect(mockRouter.replace).toHaveBeenCalled());
    expect(mockRouter.back).not.toHaveBeenCalled();
  });

  it("refuses a check on a failed scan", async () => {
    const scanId = await newScan();
    mockStore.scan(scanId)!.scan.status = "failed";
    await renderScreen(GuidedCaptureScreen, { scanId, kind: "jar_test" });
    expect(await screen.findByText("This scan failed, so it can't take a soil check. Start a new scan.")).toBeTruthy();
  });
});
