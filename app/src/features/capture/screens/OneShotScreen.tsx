// Question and guided capture (roadmap F2 item 7; screens 3.2): the capture view cut to one slot. Question mode
// takes the photo a question asks for, with its photo_hint as the instruction; guided mode shows the squeeze or jar
// steps first, with Start, then the camera. The photo is shown before Send, which uploads it (flows 6 and 8).
import { useLocalSearchParams, useRouter } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, Image, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { CameraPreview, pickFromLibrary, useCameraAccess, useCapabilities, type CameraPreviewHandle } from "@/platform";
import { Button, DropCard, Fade, IconButton, Text, ThemeProvider, camera, minTouch, space } from "@/ui";

import { CameraAction, ProblemLine } from "../components/CameraControls";
import { CaptureIcon } from "../components/CaptureIcons";
import { CoverageDial } from "../components/CoverageDial";
import { DistanceChip } from "../components/DistanceChip";
import { hasDistanceRange } from "../model/distanceRanges";
import { leaveCapture } from "../hooks/leaveCapture";
import { useOneShotData, useOneShotSession, type OneShotData, type OneShotSession } from "../hooks/useOneShotSession";
import { isGuidedKind, type OneShotPurpose } from "../model/oneShot";

/** `capture/[scanId]/question/[questionId]`: one photo for a scan's photo request. */
export function QuestionCaptureScreen() {
  const { scanId, questionId } = useLocalSearchParams<{ scanId: string; questionId: string }>();
  return (
    <ThemeProvider scheme="dark">
      <OneShot scanId={scanId} purpose={{ type: "question", questionId }} />
    </ThemeProvider>
  );
}

/** `capture/[scanId]/guided/[kind]`: a squeeze or jar test. */
export function GuidedCaptureScreen() {
  const { scanId, kind } = useLocalSearchParams<{ scanId: string; kind: string }>();
  const router = useRouter();
  return (
    <ThemeProvider scheme="dark">
      {isGuidedKind(kind) ? (
        <OneShot scanId={scanId} purpose={{ type: "guided", kind }} />
      ) : (
        <Unavailable message="There's no such soil check." onClose={() => leaveCapture(router)} />
      )}
    </ThemeProvider>
  );
}

function OneShot({ scanId, purpose }: { scanId: string; purpose: OneShotPurpose }) {
  const router = useRouter();
  const data = useOneShotData(scanId, purpose);
  if (data.loadError) {
    return <Unavailable message="Couldn't open this scan" onClose={() => leaveCapture(router)} onRetry={data.retryLoad} />;
  }
  if (!data.ready || !data.target) {
    return (
      <Shell>
        <ActivityIndicator color={camera.ink} style={{ position: "absolute", top: "50%", alignSelf: "center" }} />
      </Shell>
    );
  }
  if (!data.target.ok) return <Unavailable message={data.target.message} onClose={() => leaveCapture(router)} />;
  // Mounted once the data is ready, so the first step is chosen once: the steps, or the camera from the jar task.
  return <OneShotCamera scanId={scanId} purpose={purpose} data={data} />;
}

function OneShotCamera({ scanId, purpose, data }: { scanId: string; purpose: OneShotPurpose; data: OneShotData }) {
  const session = useOneShotSession(scanId, purpose, data);
  const { state, target, actions } = session;
  const access = useCameraAccess();
  const insets = useSafeAreaInsets();
  const preview = useRef<CameraPreviewHandle>(null);
  // A photo for a question can be measured, so it gets the distance chip; a guided check has no range.
  const { depth } = useCapabilities();
  const [distance, setDistance] = useState<number | null>(null);
  const [flash, setFlash] = useState(false);
  const [shooting, setShooting] = useState(false);
  const [covered, setCovered] = useState(false);
  const aiming = state.step === "aim";

  // As in capture, the system's camera alert comes when the camera is first needed, with no screen of ours before
  // it (HIG Privacy). In guided mode that is after Start, once the steps are read.
  const asked = useRef(false);
  useEffect(() => {
    if (aiming && access.status === "ask" && !asked.current) {
      asked.current = true;
      void access.request();
    }
  }, [aiming, access]);

  const shoot = async () => {
    if (state.step === "review") {
      actions.retake();
      return;
    }
    if (!preview.current || shooting) return;
    setShooting(true);
    try {
      const shot = await preview.current.capture();
      setFlash(true);
      setTimeout(() => setFlash(false), 380);
      actions.take(shot, false);
    } finally {
      setShooting(false);
    }
  };

  const pick = async () => {
    setCovered(true);
    try {
      const shot = await pickFromLibrary();
      if (shot) actions.take(shot, true);
    } finally {
      setCovered(false);
    }
  };

  // A question closed by the refetch after a 409 says so, as on opening.
  if (!target?.ok) return <Unavailable message={target?.message ?? "Couldn't open this scan"} onClose={actions.close} />;

  const photo = state.step === "review" || state.step === "sending" ? state.file : undefined;
  const stepProblem = state.step === "aim" || state.step === "review" ? state.problem : undefined;

  return (
    <Shell>
      {photo ? (
        <Image source={{ uri: photo.uri }} resizeMode="cover" style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }} accessibilityIgnoresInvertColors accessibilityLabel="The photo you took" />
      ) : state.step !== "intro" && access.status === "granted" ? (
        <CameraPreview
          ref={preview}
          active={aiming && !covered}
          onDistance={depth ? setDistance : undefined}
          style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
        />
      ) : state.step !== "intro" && access.status === "denied" ? (
        <View style={{ position: "absolute", top: "42%", left: space[6], right: space[6], gap: space[4], alignItems: "center" }}>
          <Text variant="body" style={{ color: camera.ink, textAlign: "center" }}>
            PDM uses the camera to photograph your plants. Turn it on in Settings, or choose a photo from your library.
          </Text>
          <Button label="Open Settings" variant="secondary" onPress={() => void access.openSettings()} />
        </View>
      ) : null}
      <Fade visible={flash} duration={flash ? 40 : 340} style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, backgroundColor: camera.ink }}>
        <View />
      </Fade>

      {/* ✕ leaves; nothing is stored until Send, so there is nothing to discard. */}
      <View
        style={{ position: "absolute", top: insets.top, left: 0, right: 0, flexDirection: "row", alignItems: "center", paddingHorizontal: 14, gap: space[3], minHeight: minTouch }}
      >
        <IconButton
          accessibilityLabel="Close"
          accessibilityHint={session.busy.sending ? "Available once the photo is sent" : undefined}
          disabled={session.busy.sending}
          onPress={actions.close}
          icon={() => <CaptureIcon name="close" color={camera.ink} />}
          style={{ backgroundColor: camera.card, borderRadius: minTouch / 2 }}
        />
        <Text variant="headline" style={{ color: camera.ink, flexShrink: 1 }} numberOfLines={1}>
          {purpose.type === "question" ? "Photo for a question" : target.title}
        </Text>
      </View>

      <DropCard dropKey={state.step === "intro" ? "intro" : "camera"} testID="instruction-card" style={{ position: "absolute", top: insets.top + 56, left: 12, right: 12 }}>
        {state.step === "intro" && target.guided ? (
          <Intro session={session} />
        ) : (
          <View style={{ gap: space[3] }}>
            <View style={{ gap: 4 }}>
              <Text variant="display" style={{ color: camera.ink, fontSize: 20, lineHeight: 25 }} accessibilityRole="header">
                {target.guided ? target.guided.title : target.title}
              </Text>
              <Text variant="subheadline" style={{ color: camera.inkMuted }}>
                {target.instruction}
              </Text>
            </View>
            {session.jarTask ? (
              <Text variant="footnote" style={{ color: camera.inkMuted }} testID="jar-task-note">
                {`On your tasks: “${session.jarTask.title}”. Come back to it once the water has cleared.`}
              </Text>
            ) : null}
            {stepProblem ? (
              <ProblemLine
                message={stepProblem.message}
                hint={stepProblem.hint}
                action={stepProblem.retry ? { label: "Try again", onPress: actions.send } : undefined}
              />
            ) : null}
          </View>
        )}
      </DropCard>

      {state.step !== "intro" ? (
        <View style={{ position: "absolute", left: 0, right: 0, bottom: insets.bottom + 16, gap: 14 }}>
          {depth && aiming && access.status === "granted" && hasDistanceRange(target.kind) ? <DistanceChip kind={target.kind} metres={distance} /> : null}
          <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingHorizontal: 22 }}>
            <View style={{ flex: 1, alignItems: "flex-start" }}>
              <IconButton
                accessibilityLabel="Choose from library"
                onPress={() => void pick()}
                disabled={state.step === "sending"}
                icon={() => <CaptureIcon name="library" color={camera.ink} size={26} />}
                style={{ width: 52, height: 52, borderRadius: 14, backgroundColor: camera.pill }}
              />
            </View>
            <CoverageDial
              segments={[{ stored: Boolean(photo) }]}
              shutter={state.step === "review" ? "retake" : aiming && access.status === "granted" && !shooting ? "take" : "busy"}
              onShutter={() => void shoot()}
              coverageText={photo ? "Photo taken" : "No photo yet"}
            />
            <View style={{ flex: 1, alignItems: "flex-end" }}>
              <CameraAction
                testID="send"
                label="Send"
                enabled={state.step === "review"}
                busy={state.step === "sending"}
                onPress={actions.send}
                disabledHint="Take the photo first"
              />
            </View>
          </View>
        </View>
      ) : null}
    </Shell>
  );
}

/** Guided mode's steps, then Start: for the jar test, Start also adds the task to photograph it later (flows 8). */
function Intro({ session }: { session: OneShotSession }) {
  const guided = session.target?.ok ? session.target.guided : undefined;
  if (!guided) return null;
  return (
    <View style={{ gap: space[3] }}>
      <Text variant="display" style={{ color: camera.ink, fontSize: 20, lineHeight: 25 }} accessibilityRole="header">
        {guided.title}
      </Text>
      <View style={{ gap: space[2] }}>
        {guided.steps.map((step, i) => (
          <View key={i} style={{ flexDirection: "row", gap: space[2] }}>
            <Text variant="subheadline" style={{ color: camera.ink, fontWeight: "600", minWidth: 16 }}>
              {`${i + 1}.`}
            </Text>
            <Text variant="subheadline" style={{ color: camera.inkMuted, flex: 1 }}>
              {step}
            </Text>
          </View>
        ))}
      </View>
      {session.problem ? <ProblemLine message={session.problem} action={{ label: "Try again", onPress: session.actions.start }} /> : null}
      {/* Neutral like every control on the camera, white on black, never tinted (screens 3.2; design 2). */}
      <View style={{ alignSelf: "flex-start" }}>
        <CameraAction testID="start" label="Start" enabled busy={session.busy.starting} onPress={session.actions.start} disabledHint="" />
      </View>
    </View>
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <View style={{ flex: 1, backgroundColor: camera.black }} testID="one-shot-capture">
      <StatusBar style="light" />
      {children}
    </View>
  );
}

function Unavailable({ message, onClose, onRetry }: { message: string; onClose?: () => void; onRetry?: () => void }) {
  return (
    <Shell>
      <View style={{ flex: 1, justifyContent: "center", padding: space[6], gap: space[4] }}>
        <Text variant="title3" style={{ color: camera.ink }}>
          {message}
        </Text>
        {onRetry ? <Button label="Try again" onPress={onRetry} /> : null}
        {onClose ? <Button label="Close" variant="secondary" onPress={onClose} /> : null}
      </View>
    </Shell>
  );
}
