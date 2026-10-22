// The live analysis (roadmap F3; screens 3.3; design 3.1, after the Camera app's visual intelligence mode): the
// photo just taken, held still and full screen, with the findings as hotspots on it; the stage card dropping from
// the top, "Not sure yet" under it and the one question low and compact, so the spots stay in view between the
// cards; the photo strip, and the shutter's place taken by a glowing ring until "See the diagnosis and plan".
// Always dark, like the camera it continues.
import { useLocalSearchParams } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { ActionSheetIOS, ActivityIndicator, Alert, Platform, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { PROVISIONAL_PLAN_NOTE } from "@/domain/diagnosis";
import { Button, CandidateCompare, IconButton, Text, ThemeProvider, camera, minTouch, radii, space } from "@/ui";

import { FailureCard, LowEvidenceCard } from "../components/OutcomeCards";
import { AnalysisRing, PhotoStage, PhotoStrip } from "../components/PhotoStage";
import { QuestionCard } from "../components/QuestionCard";
import { ScanIcon } from "../components/ScanIcons";
import { StageCard } from "../components/StageCard";
import { useLiveAnalysis } from "../hooks/useLiveAnalysis";

const KIND_WORDS = { fungal: "Fungal", bacterial: "Bacterial", viral: "Viral", pest: "A pest", wildlife: "Wildlife", abiotic: "Growing conditions" } as const;

export function ScanScreen() {
  return (
    <ThemeProvider scheme="dark">
      <LiveAnalysis />
    </ThemeProvider>
  );
}

function LiveAnalysis() {
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  const v = useLiveAnalysis(scanId);
  const insets = useSafeAreaInsets();

  if (v.gone) return <Gone onClose={v.actions.close} />;

  const soilMenu = () => {
    const options = ["Squeeze test", "Jar test", "Cancel"];
    const pick = (i: number) => {
      if (i === 0) v.actions.checkSoil("squeeze_test");
      if (i === 1) v.actions.checkSoil("jar_test");
    };
    if (Platform.OS === "ios") {
      ActionSheetIOS.showActionSheetWithOptions({ title: "Check your soil", options, cancelButtonIndex: 2 }, pick);
    } else {
      Alert.alert("Check your soil", undefined, [
        { text: options[0]!, onPress: () => pick(0) },
        { text: options[1]!, onPress: () => pick(1) },
        { text: options[2]!, style: "cancel" },
      ]);
    }
  };

  const ending = v.ending;
  const showQuestion = v.question && ending.kind !== "failed";

  return (
    <View style={{ flex: 1, backgroundColor: camera.black }} testID="live-analysis">
      <StatusBar style="light" />
      <PhotoStage image={v.image} hotspots={v.hotspots} animated={v.liveKeys} species={showQuestion ? undefined : v.species} />

      {v.loading ? (
        <ActivityIndicator color={camera.ink} style={{ position: "absolute", top: "50%", alignSelf: "center" }} accessibilityLabel="Loading the scan" />
      ) : (
        <View style={{ position: "absolute", top: insets.top + 6, left: 12, right: 12, gap: 8 }} pointerEvents="box-none">
          <StageCard line={v.line} phases={v.phases} />
          {v.unsure ? (
            <Panel testID="not-sure-yet">
              <CandidateCompare
                title={v.unsure.title}
                plates={[
                  { name: v.unsure.plates[0].name, caption: KIND_WORDS[v.unsure.plates[0].kind] },
                  { name: v.unsure.plates[1].name, caption: KIND_WORDS[v.unsure.plates[1].kind] },
                ]}
                footnote="One answer settles it."
              />
            </Panel>
          ) : null}
          {v.lastAnswer ? (
            <Panel testID="last-answer">
              <View style={{ flexDirection: "row", gap: space[3], alignItems: "center" }} accessibilityLiveRegion="polite">
                <ScanIcon name="check" color={camera.ink} size={18} />
                <Text variant="subheadline" style={{ flex: 1 }}>
                  You answered <Text variant="subheadline" style={{ fontWeight: "600" }}>{v.lastAnswer}</Text>.
                </Text>
              </View>
            </Panel>
          ) : null}
          {v.uncertainLine && ending.kind === "plan" ? (
            <Panel testID="uncertain-result">
              <Text variant="headline">{v.uncertainLine}</Text>
              <Text variant="subheadline" color="textMuted">
                {PROVISIONAL_PLAN_NOTE}
              </Text>
            </Panel>
          ) : null}
          {ending.kind === "failed" && v.state?.failure ? (
            <FailureCard failure={v.state.failure} canAct={v.canRetry} busy={v.retrying} onRetry={v.actions.retry} onNewScan={v.actions.newScan} />
          ) : null}
          {ending.kind === "low_evidence" ? <LowEvidenceCard canAct={v.canRetry} onCloserPhoto={v.actions.closerPhoto} /> : null}
          {v.checkingSoil ? (
            <Panel testID="checking-soil">
              <Text variant="subheadline">Checking your soil photo</Text>
            </Panel>
          ) : null}
        </View>
      )}

      {showQuestion && v.question ? (
        <View style={{ position: "absolute", left: 12, right: 12, bottom: insets.bottom + 150 }} pointerEvents="box-none">
          <QuestionCard
            question={v.question}
            canAnswer={v.canAnswer}
            sending={v.sending}
            onAnswer={v.actions.answer}
            onTakePhoto={v.actions.takePhoto}
            problem={v.answerProblem}
          />
        </View>
      ) : null}

      {/* The controls: back, the ring or the pill, and the soil checks (index.html .cam). */}
      <View style={{ position: "absolute", left: 0, right: 0, bottom: insets.bottom + 68, flexDirection: "row", alignItems: "center", paddingHorizontal: 24, gap: space[3] }}>
        <IconButton
          accessibilityLabel="Close"
          accessibilityHint="The analysis goes on; its result appears on the plant"
          onPress={v.actions.close}
          icon={() => <ScanIcon name="back" color={camera.ink} />}
          style={{ width: minTouch + 4, height: minTouch + 4, borderRadius: (minTouch + 4) / 2, backgroundColor: camera.card }}
        />
        <View style={{ flex: 1, alignItems: "center" }}>
          {ending.kind === "plan" || ending.kind === "low_evidence" ? (
            <Button label="See the diagnosis and plan" size="large" onPress={v.actions.seeResult} testID="see-result" style={{ alignSelf: "stretch" }} />
          ) : ending.kind === "running" && !v.loading ? (
            <AnalysisRing />
          ) : null}
        </View>
        {v.canCheckSoil ? (
          <IconButton
            accessibilityLabel="Check your soil"
            onPress={soilMenu}
            icon={() => <ScanIcon name="more" color={camera.ink} />}
            style={{ width: minTouch + 4, height: minTouch + 4, borderRadius: (minTouch + 4) / 2, backgroundColor: camera.card }}
          />
        ) : (
          <View style={{ width: minTouch + 4 }} />
        )}
      </View>

      <View style={{ position: "absolute", left: 0, right: 0, bottom: insets.bottom + 8 }}>
        <PhotoStrip images={v.images} selectedId={v.image?.id} onSelect={v.actions.selectImage} />
      </View>
    </View>
  );
}

/** A solid dark card under the stage card (styles.css .layer[data-scan="camera"] .live > .sec .card). */
function Panel({ children, testID }: { children: React.ReactNode; testID?: string }) {
  return (
    <View
      testID={testID}
      style={{ paddingVertical: 12, paddingHorizontal: 16, gap: 4, borderRadius: radii.sheet, borderCurve: "continuous", backgroundColor: camera.card, borderWidth: 1, borderColor: camera.hairline }}
    >
      {children}
    </View>
  );
}

function Gone({ onClose }: { onClose: () => void }) {
  return (
    <View style={{ flex: 1, backgroundColor: camera.black, justifyContent: "center", padding: space[6], gap: space[4] }} testID="scan-gone">
      <StatusBar style="light" />
      <Text variant="title2" accessibilityRole="header">
        This scan is gone
      </Text>
      <Text variant="body" color="textMuted">
        It was deleted, or you no longer have access to its garden.
      </Text>
      <Button label="Back to the garden" variant="secondary" onPress={onClose} />
    </View>
  );
}
