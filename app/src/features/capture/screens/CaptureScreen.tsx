// Capture (roadmap F2; screens 3.2; design 3.1, after the Camera app): the camera full screen, always dark. The
// chosen shot's instruction drops from the top, the framing guide sits where the subject goes, and the mode bar,
// the shutter with its coverage dial, Library and Analyse sit at the bottom, neutral white and solid dark on the
// photo (design 2). Each shot uploads as it is taken (flows 4).
import { useLocalSearchParams } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, Alert, Image, Pressable, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { CameraPreview, pickFromLibrary, useCameraAccess, useCapabilities, type CameraPreviewHandle } from "@/platform";
import { Button, DropCard, Fade, IconButton, Text, ThemeProvider, camera, minTouch, space } from "@/ui";

import { CameraAction, ProblemLine } from "../components/CameraControls";
import { CameraNotice } from "../components/CameraNotice";
import { CaptureIcon } from "../components/CaptureIcons";
import { CoverageDial } from "../components/CoverageDial";
import { FramingGuide } from "../components/FramingGuide";
import { DistanceChip } from "../components/DistanceChip";
import { ModeBar } from "../components/ModeBar";
import { useCaptureSession, type CaptureSessionView } from "../hooks/useCaptureSession";
import { pauseText, type CameraPause } from "../model/interruption";

export function CaptureScreen() {
  return (
    <ThemeProvider scheme="dark">
      <Capture />
    </ThemeProvider>
  );
}

function Capture() {
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  const session = useCaptureSession(scanId);
  const access = useCameraAccess();
  const insets = useSafeAreaInsets();
  const preview = useRef<CameraPreviewHandle>(null);
  const [flash, setFlash] = useState(false);
  const [shooting, setShooting] = useState(false);
  // The preview stops while the library picker or the discard alert covers it, to save power and heat.
  const [covered, setCovered] = useState(false);
  // Where the drop card ends, so the framing guide starts below it.
  const [cardBottom, setCardBottom] = useState(0);
  // LiDAR phones only: metres to the centre of the frame, for the distance chip (architecture 6.7).
  const { depth } = useCapabilities();
  const [distance, setDistance] = useState<number | null>(null);
  // The camera taken away by a call or another app: the shutter waits, and the screen says why.
  const [paused, setPaused] = useState<CameraPause | null>(null);
  const [shotFailed, setShotFailed] = useState(false);
  const v = session.view;
  const { actions } = session;

  // The user just chose to scan, so the context says why: the system's alert comes straight away, with no screen
  // of ours before it (HIG Privacy, requesting permission and pre-alert screens). It asks once; after a refusal
  // only Settings can change it.
  const asked = useRef(false);
  useEffect(() => {
    if (access.status === "ask" && !asked.current) {
      asked.current = true;
      void access.request();
    }
  }, [access]);

  const shoot = async () => {
    if (!preview.current || shooting || paused) return;
    setShooting(true);
    try {
      const shot = await preview.current.capture();
      setShotFailed(false);
      setFlash(true);
      setTimeout(() => setFlash(false), 380);
      actions.addShot(shot, false);
    } catch {
      setShotFailed(true);
    } finally {
      setShooting(false);
    }
  };

  const pick = async () => {
    setCovered(true);
    try {
      const shot = await pickFromLibrary();
      if (shot) actions.addShot(shot, true);
    } finally {
      setCovered(false);
    }
  };

  // Discard asks first (screens 3.2), with the platform's own alert and a destructive button.
  const confirmDiscard = () => {
    setCovered(true);
    // A jar test started on this scan goes with it: the server deletes its task with the scan (flows 4, discard).
    Alert.alert("Discard this scan?", session.jarTask ? "Its photos and its jar-test task will be deleted." : "Its photos will be deleted.", [
      { text: "Cancel", style: "cancel", onPress: () => setCovered(false) },
      {
        text: "Discard",
        style: "destructive",
        onPress: () => {
          setCovered(false);
          actions.discard();
        },
      },
    ]);
  };

  if (session.scan.error) {
    return (
      <Shell>
        <View style={{ flex: 1, justifyContent: "center", padding: space[6], gap: space[4] }}>
          <Text variant="title3" style={{ color: camera.ink }}>
            {"Couldn't open this scan"}
          </Text>
          <Button label="Close" variant="secondary" onPress={actions.close} />
        </View>
      </Shell>
    );
  }

  return (
    <Shell>
      {access.status === "granted" ? (
        <CameraPreview
          ref={preview}
          active={!covered}
          onDistance={depth ? setDistance : undefined}
          onInterruption={setPaused}
          style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }}
        />
      ) : (
        <CameraAccessNote access={access} />
      )}
      <Fade visible={flash} duration={flash ? 40 : 340} style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, backgroundColor: camera.ink }}>
        <View />
      </Fade>
      {v ? <FramingGuide kind={v.mode.kind} clearTop={cardBottom} /> : null}

      {/* The top bar: ✕ keeps the draft, Discard deletes the scan (screens 3.2). */}
      <View
        style={{
          position: "absolute",
          top: insets.top,
          left: 0,
          right: 0,
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          paddingHorizontal: 14,
          minHeight: minTouch,
        }}
      >
        <IconButton
          accessibilityLabel="Close"
          accessibilityHint="Keeps the photos for later"
          onPress={actions.close}
          icon={() => <CaptureIcon name="close" color={camera.ink} />}
          style={{ backgroundColor: camera.card, borderRadius: minTouch / 2 }}
        />
        <Text variant="headline" style={{ color: camera.ink, flexShrink: 1 }} numberOfLines={1}>
          {v?.title ?? "Scan"}
        </Text>
        <Pressable
          accessibilityRole="button"
          onPress={confirmDiscard}
          disabled={session.busy.discarding}
          hitSlop={8}
          style={{ minHeight: minTouch, justifyContent: "center", paddingHorizontal: 6 }}
        >
          <Text variant="headline" style={{ color: camera.ink }}>
            {session.busy.discarding ? "Discarding…" : "Discard"}
          </Text>
        </Pressable>
      </View>

      {v ? (
        <DropCard
          dropKey={v.mode.kind}
          testID="instruction-card"
          onLayout={(e) => setCardBottom(e.nativeEvent.layout.y + e.nativeEvent.layout.height)}
          style={{ position: "absolute", top: insets.top + 56, left: 12, right: 12 }}
        >
          <Instruction view={v} problem={session.problem} onRetry={actions.retry} onRemove={actions.removeShot} onNewScan={actions.close} />
        </DropCard>
      ) : null}

      {v ? (
        <View style={{ position: "absolute", left: 0, right: 0, bottom: insets.bottom + 8, gap: 14 }}>
          {paused ? (
            <CameraNotice text={pauseText(paused)} />
          ) : shotFailed ? (
            <CameraNotice text="Couldn't take the photo. Try again." />
          ) : depth && access.status === "granted" ? (
            <DistanceChip kind={v.mode.kind} metres={distance} />
          ) : null}
          <View style={{ alignSelf: "center", maxWidth: "92%", paddingHorizontal: 12, paddingVertical: 6, borderRadius: 999, backgroundColor: camera.pill }}>
            <Text variant="footnote" style={{ color: camera.ink, textAlign: "center" }} testID="coverage-text">
              <Text variant="footnote" style={{ color: camera.ink, fontWeight: "600" }}>
                {v.neededText}
              </Text>
              {v.suggestion ? ` · ${v.suggestion}` : ""}
            </Text>
          </View>

          <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingHorizontal: 22 }}>
            <View style={{ flex: 1, alignItems: "flex-start" }}>
              <IconButton
                accessibilityLabel="Choose from library"
                onPress={() => void pick()}
                disabled={v.shutter === "busy"}
                icon={() => <CaptureIcon name="library" color={camera.ink} size={26} />}
                style={{ width: 52, height: 52, borderRadius: 14, backgroundColor: camera.pill }}
              />
            </View>
            <CoverageDial
              segments={v.dial}
              shutter={access.status === "granted" && !shooting && !paused ? v.shutter : "busy"}
              onShutter={() => void shoot()}
              coverageText={v.neededText}
            />
            <View style={{ flex: 1, alignItems: "flex-end" }}>
              <CameraAction
                testID="analyse"
                label="Analyse"
                enabled={v.canAnalyse}
                busy={session.busy.submitting}
                onPress={actions.analyse}
                disabledHint="Take the whole plant first"
              />
            </View>
          </View>

          <View style={{ paddingHorizontal: 20 }}>
            <ModeBar items={v.modes} onSelect={actions.selectMode} />
          </View>
        </View>
      ) : (
        <ActivityIndicator color={camera.ink} style={{ position: "absolute", top: "50%", alignSelf: "center" }} />
      )}
    </Shell>
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <View style={{ flex: 1, backgroundColor: camera.black }} testID="capture">
      <StatusBar style="light" />
      {children}
    </View>
  );
}

function Instruction({
  view,
  problem,
  onRetry,
  onRemove,
  onNewScan,
}: {
  view: CaptureSessionView;
  problem: ReturnType<typeof useCaptureSession>["problem"];
  onRetry: (slotId: string) => void;
  onRemove: (slotId: string) => void;
  onNewScan: () => void;
}) {
  const shotProblem = view.problem;
  return (
    <View style={{ gap: space[3] }}>
      <View style={{ gap: 4 }}>
        <Text variant="display" style={{ color: camera.ink, fontSize: 20, lineHeight: 25 }} accessibilityRole="header">
          {view.mode.title}
        </Text>
        <Text variant="subheadline" style={{ color: camera.inkMuted }}>
          {view.mode.instruction}
        </Text>
      </View>

      {view.shots.length > 0 ? (
        <View style={{ flexDirection: "row", gap: space[2] }}>
          {view.shots.map((s) => (
            <View key={s.id} style={{ width: 56, height: 56 }}>
              <View style={{ width: 56, height: 56, borderRadius: 14, overflow: "hidden", backgroundColor: camera.selected }}>
                {s.uri ? <Image source={{ uri: s.uri }} style={{ width: 56, height: 56 }} accessibilityIgnoresInvertColors /> : null}
                {s.state === "uploading" || s.state === "captured" || s.state === "deleting" ? (
                  <ActivityIndicator color={camera.ink} style={{ position: "absolute", top: 18, left: 18 }} />
                ) : null}
              </View>
              {s.state === "stored" ? (
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={`Remove this ${view.mode.title.toLowerCase()} photo`}
                  onPress={() => onRemove(s.id)}
                  hitSlop={10}
                  style={{ position: "absolute", top: -6, right: -6, width: 22, height: 22, borderRadius: 11, alignItems: "center", justifyContent: "center", backgroundColor: camera.selected }}
                >
                  <CaptureIcon name="close" color={camera.ink} size={12} />
                </Pressable>
              ) : null}
            </View>
          ))}
        </View>
      ) : null}

      {problem ? (
        <ProblemLine message={problem.message} action={problem.newScan ? { label: "Close", onPress: onNewScan } : undefined} />
      ) : shotProblem ? (
        <ProblemLine
          message={shotProblem.message}
          hint={shotProblem.hint}
          action={shotProblem.retryable ? { label: "Try again", onPress: () => onRetry(shotProblem.slotId) } : undefined}
        />
      ) : null}
    </View>
  );
}

/** After the camera was refused: why it's needed and the way to Settings. Library still works without it. */
function CameraAccessNote({ access }: { access: ReturnType<typeof useCameraAccess> }) {
  if (access.status !== "denied") return null;
  return (
    <View style={{ position: "absolute", top: "38%", left: space[6], right: space[6], gap: space[4], alignItems: "center" }}>
      <Text variant="body" style={{ color: camera.ink, textAlign: "center" }}>
        PDM uses the camera to photograph your plants. Turn it on in Settings, or choose a photo from your library.
      </Text>
      <Button label="Open Settings" variant="secondary" onPress={() => void access.openSettings()} />
    </View>
  );
}
