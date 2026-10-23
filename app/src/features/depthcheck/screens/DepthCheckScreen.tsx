// The LiDAR day-1 check and evaluation captures (roadmap F5 items 1–2; architecture 10; evaluation, LiDAR row). A
// tool for the team in the development build, reached from Settings on a LiDAR iPhone: the camera with the live
// distance, then the shot's report (photo and depth sizes, accuracy, holes, distortion, gravity), a two-point ruler
// for the scale card at 30 cm, and Save into an evaluation case the vision track takes off the phone.
import { useRouter } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useRef, useState } from "react";
import { Image, Pressable, ScrollView, View, useWindowDimensions } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import type { ImageKind } from "@/domain/types";
import { CameraPreview, useCameraAccess, useCapabilities, type CameraPreviewHandle, type CaptureReport } from "@/platform";
import { Banner, Button, Section, Text, TextField, ThemeProvider, camera, minTouch, space, useTheme } from "@/ui";

import { useDepthCheck, type RulerResult } from "../hooks/useDepthCheck";
import { RULER_TOLERANCE, pitchDeg, relativeError, type Norm } from "../model/ruler";

const KINDS: { kind: ImageKind; label: string }[] = [
  { kind: "whole_plant", label: "Whole" },
  { kind: "leaf_closeup", label: "Close-up" },
  { kind: "leaf_underside", label: "Underside" },
  { kind: "soil_surface", label: "Soil" },
  { kind: "flower_fruit", label: "Flower" },
];

export function DepthCheckScreen() {
  const check = useDepthCheck();
  const { depth } = useCapabilities();
  if (!depth) {
    return (
      <View style={{ flex: 1, justifyContent: "center", padding: space[6] }}>
        <Text variant="body">The depth check needs a LiDAR iPhone and the development build.</Text>
      </View>
    );
  }
  return check.shot ? <Result check={check} /> : <Aim onShot={check.actions.take} />;
}

function Aim({ onShot }: { onShot: ReturnType<typeof useDepthCheck>["actions"]["take"] }) {
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const access = useCameraAccess();
  const preview = useRef<CameraPreviewHandle>(null);
  const [distance, setDistance] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  const shoot = async () => {
    if (!preview.current || busy) return;
    setBusy(true);
    try {
      onShot(await preview.current.capture());
    } finally {
      setBusy(false);
    }
  };

  return (
    <ThemeProvider scheme="dark">
      <View style={{ flex: 1, backgroundColor: camera.black }}>
        <StatusBar style="light" />
        {access.status === "granted" ? (
          <CameraPreview ref={preview} active onDistance={setDistance} style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0 }} />
        ) : access.status === "ask" ? (
          <View style={{ flex: 1, justifyContent: "center", padding: space[6] }}>
            <Button label="Allow the camera" variant="secondary" onPress={() => void access.request()} />
          </View>
        ) : null}
        {/* The crosshair marks the square the distance is the median of. */}
        <View pointerEvents="none" style={{ position: "absolute", top: "50%", left: "50%", width: 36, height: 36, marginLeft: -18, marginTop: -18, borderWidth: 1.5, borderColor: camera.ink, borderRadius: 6 }} />
        <View style={{ position: "absolute", top: insets.top + 8, left: 16, right: 16, flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
          <Button label="Close" variant="secondary" size="small" onPress={() => router.back()} />
          <View style={{ paddingHorizontal: 14, paddingVertical: 8, borderRadius: 999, backgroundColor: camera.pill }}>
            <Text variant="title3" style={{ color: camera.ink, fontVariant: ["tabular-nums"] }} testID="live-distance">
              {distance == null ? "No depth" : `${(distance * 100).toFixed(1)} cm`}
            </Text>
          </View>
        </View>
        <View style={{ position: "absolute", bottom: insets.bottom + 24, alignSelf: "center" }}>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Take the photo"
            disabled={busy || access.status !== "granted"}
            onPress={() => void shoot()}
            style={{ width: 76, height: 76, borderRadius: 38, borderWidth: 4, borderColor: camera.ink, alignItems: "center", justifyContent: "center", opacity: busy ? 0.5 : 1 }}
          >
            <View style={{ width: 60, height: 60, borderRadius: 30, backgroundColor: camera.ink }} />
          </Pressable>
        </View>
      </View>
    </ThemeProvider>
  );
}

function Result({ check }: { check: ReturnType<typeof useDepthCheck> }) {
  const insets = useSafeAreaInsets();
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const shot = check.shot!;
  const [known, setKnown] = useState("");
  const [caseId, setCaseId] = useState("");
  const [plantKey, setPlantKey] = useState("");
  const [kind, setKind] = useState<ImageKind>("leaf_closeup");
  const boxW = width - space[4] * 2;
  const boxH = (boxW * shot.file.height) / shot.file.width;

  const onTap = (x: number, y: number) => check.actions.tap({ x: x / boxW, y: y / boxH });

  return (
    <ScrollView contentContainerStyle={{ paddingTop: insets.top + space[3], paddingBottom: insets.bottom + space[8], paddingHorizontal: space[4], gap: space[5] }} style={{ backgroundColor: colors.background }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text variant="title2" accessibilityRole="header">
          Depth check
        </Text>
        <Button label="Retake" variant="secondary" size="small" onPress={check.actions.retake} />
      </View>

      <Pressable
        accessibilityLabel="The photo. Tap two ends of the scale card to measure it."
        onPress={(e) => onTap(e.nativeEvent.locationX, e.nativeEvent.locationY)}
        style={{ width: boxW, height: boxH, borderRadius: 16, overflow: "hidden" }}
      >
        <Image source={{ uri: shot.file.uri }} style={{ width: boxW, height: boxH }} accessibilityIgnoresInvertColors />
        {check.points.map((p, i) => (
          <Marker key={i} p={p} w={boxW} h={boxH} />
        ))}
      </Pressable>

      <Section title="Ruler">
        <View style={{ gap: space[3], padding: space[4] }}>
          <TextField label="Known length (mm)" keyboardType="decimal-pad" value={known} onChangeText={setKnown} placeholder="85.6 for a card's long side" />
          <RulerLine ruler={check.ruler} known={Number(known)} />
        </View>
      </Section>

      {shot.report ? <ReportList report={shot.report} /> : null}

      <Section title="Save for evaluation">
        <View style={{ gap: space[3], padding: space[4] }}>
          <TextField label="Case ID" autoCapitalize="none" autoCorrect={false} value={caseId} onChangeText={setCaseId} placeholder="tomato-0007" />
          <TextField label="Plant" autoCapitalize="none" autoCorrect={false} value={plantKey} onChangeText={setPlantKey} placeholder="tomato-a, the same plant across scans" />
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }} accessibilityRole="radiogroup">
            {KINDS.map((k) => (
              <Pressable
                key={k.kind}
                accessibilityRole="radio"
                accessibilityState={{ selected: kind === k.kind }}
                onPress={() => setKind(k.kind)}
                style={{ minHeight: minTouch, paddingHorizontal: 14, justifyContent: "center", borderRadius: 999, backgroundColor: kind === k.kind ? colors.text : colors.surface }}
              >
                <Text variant="subheadline" style={{ color: kind === k.kind ? colors.background : colors.text }}>
                  {k.label}
                </Text>
              </Pressable>
            ))}
          </View>
          <Button label="Save the shot" onPress={() => check.actions.save(caseId.trim(), plantKey.trim(), kind)} />
          {check.saved ? <Text variant="footnote" color="textMuted">{`${check.saved}. Files › On My iPhone › PDM › evaluation.`}</Text> : null}
          {check.problem ? <Banner tone="watch" title={check.problem} /> : null}
        </View>
      </Section>
    </ScrollView>
  );
}

function Marker({ p, w, h }: { p: Norm; w: number; h: number }) {
  return (
    <View
      pointerEvents="none"
      style={{ position: "absolute", left: p.x * w - 9, top: p.y * h - 9, width: 18, height: 18, borderRadius: 9, borderWidth: 2, borderColor: "#FFFFFF", backgroundColor: "rgba(0,0,0,0.35)" }}
    />
  );
}

function RulerLine({ ruler, known }: { ruler: RulerResult | undefined; known: number }) {
  if (!ruler) return <Text variant="subheadline" color="textMuted">Tap the two ends of the card on the photo.</Text>;
  if ("problem" in ruler) return <Text variant="subheadline">{ruler.problem}</Text>;
  const error = known > 0 ? relativeError(ruler.mm, known) : undefined;
  const pass = error !== undefined && Math.abs(error) <= RULER_TOLERANCE;
  return (
    <Text variant="headline" testID="ruler-result">
      {`${ruler.mm.toFixed(1)} mm`}
      {error !== undefined ? ` · ${error >= 0 ? "+" : ""}${(error * 100).toFixed(1)}% · ${pass ? "within 3%" : "outside 3%"}` : ""}
    </Text>
  );
}

function ReportList({ report }: { report: CaptureReport }) {
  const d = report.depth;
  const rows: [string, string][] = [
    ["Photo, stored", `${report.photo.width} × ${report.photo.height} px, EXIF orientation ${report.photo.orientation}`],
    ["Depth map", d ? `${d.width} × ${d.height} px` : "None"],
    ["Depth accuracy", d ? d.accuracy : "—"],
    ["Pixels with depth", d ? `${Math.round(d.valid_fraction * 100)}% (holes kept: ${d.filtered ? "no, filtered" : "yes"})` : "—"],
    ["Distortion table", d ? (d.has_distortion_table ? "Sent by the camera" : "None") : "—"],
    ["Centre distance", d?.centre_distance_m != null ? `${(d.centre_distance_m * 100).toFixed(1)} cm in the photo's depth` : "No depth at the centre"],
    ["Gravity, camera frame", report.gravity ? `${report.gravity.map((g) => g.toFixed(3)).join(", ")} · camera ${pitchDeg(report.gravity).toFixed(1)}° below level` : "Not read"],
    ["Depth part", report.dropped ? `Not sent: ${report.dropped.replace("_", " ")}` : "Sent with the upload"],
  ];
  return (
    <Section title="What the camera gave">
      <View style={{ padding: space[4], gap: space[3] }}>
        {rows.map(([label, value]) => (
          <View key={label} style={{ gap: 2 }} accessible>
            <Text variant="footnote" color="textMuted">
              {label}
            </Text>
            <Text variant="body" style={{ fontVariant: ["tabular-nums"] }}>
              {value}
            </Text>
          </View>
        ))}
      </View>
    </Section>
  );
}
