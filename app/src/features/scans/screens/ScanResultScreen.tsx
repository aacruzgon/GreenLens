// The diagnosis and plan (roadmap F4; screens 3.3–3.4; design 3.1, the diagnosis after Apple Music). "See the
// diagnosis and plan" pushes it over the live analysis, and a plant's timeline opens it too. The photo fades into a
// ground tinted from it, the diagnosis sits centred over the faded edge, and the lists follow on cards over the
// tint, with the plan's mini player at the bottom. It follows the light or dark scheme: it is not a camera screen.
import { useLocalSearchParams, useRouter } from "expo-router";
import { ScrollView, View, useWindowDimensions } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useAskAbout } from "@/features/chat";
import { Banner, Button, CitationList, EmptyState, ErrorState, Skeleton, Text, TopEdge, gutter, space, useHeroScroll } from "@/ui";

import { DiagnosisHero, heroHeightFor } from "../components/DiagnosisHero";
import { FeatureCards } from "../components/FeatureCards";
import { MINI_PLAYER_HEIGHT, MiniPlayer } from "../components/MiniPlayer";
import { PlanChecklist } from "../components/PlanChecklist";
import { PlanQuestion } from "../components/PlanQuestion";
import { EvidenceList, ValueList } from "../components/ResultLists";
import { TintCard } from "../components/TintCard";
import { useDiagnosis } from "../hooks/useDiagnosis";

function Heading({ children }: { children: string }) {
  return (
    <Text variant="section" style={{ marginHorizontal: space[1] }}>
      {children}
    </Text>
  );
}

export function ScanResultScreen() {
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const d = useDiagnosis(scanId);
  const { tint, view, plan } = d;
  const chat = useAskAbout(d.gardenId);
  const askAbout = () => void chat.ask({ type: "scan", id: scanId });
  // The bar buttons scroll away with the photo; once it is under the status bar, the ground goes behind that.
  const { past, scrollProps } = useHeroScroll(heroHeightFor(width) - insets.top);

  if (d.gone || (d.error && !d.view)) {
    return (
      <View style={{ flex: 1, backgroundColor: tint.ground, justifyContent: "center", padding: gutter }}>
        {d.gone ? (
          <EmptyState title="We can't find this" message="This scan was deleted, or you no longer have access to it." action={{ label: "Back to the garden", onPress: d.actions.inGarden, variant: "secondary" }} />
        ) : (
          <ErrorState message="The result didn't load." onRetry={() => router.replace({ pathname: "/scans/[scanId]/result", params: { scanId } })} />
        )}
      </View>
    );
  }

  if (d.pending) {
    return (
      <View style={{ flex: 1, backgroundColor: tint.ground, justifyContent: "center", padding: gutter }}>
        <EmptyState
          title="The diagnosis isn't ready yet"
          message={`${d.plantName}'s scan is still being analysed.`}
          action={{ label: "Follow the analysis", onPress: () => router.replace({ pathname: "/scans/[scanId]", params: { scanId } }) }}
        />
      </View>
    );
  }

  if (d.loading || !view) {
    return (
      <View style={{ flex: 1, backgroundColor: tint.ground, paddingTop: insets.top + space[16], paddingHorizontal: gutter, gap: space[4] }} testID="result-loading">
        <Skeleton height={260} radius={0} style={{ marginHorizontal: -gutter, marginTop: -(insets.top + space[16]) }} />
        <Skeleton height={40} width="70%" style={{ alignSelf: "center" }} />
        <Skeleton height={16} width="50%" style={{ alignSelf: "center" }} />
        <Skeleton height={140} radius={28} />
      </View>
    );
  }

  const next = plan?.next;
  return (
    <View style={{ flex: 1, backgroundColor: tint.ground }} testID="scan-result">
      <ScrollView
        {...scrollProps}
        testID="result-scroll"
        contentInsetAdjustmentBehavior="never"
        contentContainerStyle={{ paddingBottom: insets.bottom + space[8] + (next ? MINI_PLAYER_HEIGHT + space[4] : 0) }}
      >
        <DiagnosisHero photo={d.photo} ground={tint.ground} header={view.header} onBack={d.actions.back} onInGarden={d.actions.inGarden} />

        {view.lowEvidence && d.canScan ? (
          <Button label="Take a closer photo" onPress={d.actions.closerPhoto} style={{ alignSelf: "center", marginTop: space[4] }} />
        ) : null}

        <View style={{ marginTop: space[6] }}>
          <FeatureCards cards={view.cards} fill={tint.card} onReport={d.actions.openSource} />
        </View>

        <View style={{ paddingHorizontal: gutter, gap: space[6], marginTop: space[6] }}>
          {view.evidence.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>What matched</Heading>
              <EvidenceList rows={view.evidence} fill={tint.card} separator={tint.separator} />
            </View>
          ) : null}

          {d.question ? (
            <PlanQuestion
              question={d.question}
              canAnswer={d.canAnswer}
              sending={d.sending}
              problem={d.answerProblem}
              fill={tint.card}
              onAnswer={d.actions.answer}
            />
          ) : null}

          {plan ? (
            <PlanChecklist
              steps={plan.steps}
              summary={plan.summary}
              provisional={plan.provisional}
              updated={plan.updated}
              canCheck={plan.canCheck}
              checkProblem={plan.checkProblem}
              fill={tint.card}
              separator={tint.separator}
              onCheck={plan.check}
              onOpenSteps={d.actions.stepByStep}
              onOpenFarmRecord={d.actions.openFarmRecord}
            />
          ) : null}

          {plan && plan.watchFor.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>Watch for</Heading>
              <TintCard fill={tint.card} style={{ paddingVertical: 14, paddingHorizontal: space[4], gap: space[1] }}>
                {plan.watchFor.map((w) => (
                  <Text key={w} variant="subheadline">
                    {w}
                  </Text>
                ))}
              </TintCard>
            </View>
          ) : null}

          {plan && plan.constraints.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>Your constraints</Heading>
              <TintCard fill={tint.card} style={{ paddingVertical: 14, paddingHorizontal: space[4], gap: space[1] }}>
                {plan.constraints.map((c) => (
                  <Text key={c} variant="subheadline">
                    {c}
                  </Text>
                ))}
              </TintCard>
            </View>
          ) : null}

          {view.weather.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>Weather at the scan</Heading>
              <ValueList rows={view.weather} fill={tint.card} separator={tint.separator} testID="weather-list" />
            </View>
          ) : null}

          {view.soil.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>Soil</Heading>
              <ValueList rows={view.soil} fill={tint.card} separator={tint.separator} testID="soil-list" />
            </View>
          ) : null}

          {view.measurements.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>Measurements</Heading>
              <ValueList rows={view.measurements} fill={tint.card} separator={tint.separator} testID="measurement-list" />
            </View>
          ) : null}

          {view.organisms.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>Found on the plant</Heading>
              <ValueList rows={view.organisms} fill={tint.card} separator={tint.separator} testID="organism-list" />
            </View>
          ) : null}

          {d.sources.length > 0 ? (
            <View style={{ gap: 10 }}>
              <Heading>Sources</Heading>
              <TintCard fill={tint.card}>
                <CitationList citations={d.sources} onOpen={d.actions.openSource} separator={tint.separator} />
              </TintCard>
            </View>
          ) : null}

          {chat.problem ? <Banner tone="danger" title={chat.problem} /> : null}
          {/* The result screen's main action, as in the mockup: the large primary button. */}
          <Button label="Ask about this result" size="large" onPress={askAbout} busy={chat.opening} style={{ alignSelf: "stretch" }} testID="result-ask" />
          {d.canFeedback ? (
            <Button label="Was this right?" variant="secondary" onPress={d.actions.feedback} style={{ alignSelf: "center" }} testID="result-feedback" />
          ) : null}
        </View>
      </ScrollView>
      <TopEdge past={past} color={tint.ground} separator={tint.separator} height={insets.top} />

      {next && plan ? (
        <MiniPlayer
          stepNumber={next.index + 1}
          stepCount={plan.steps.length}
          title={next.step.title}
          photo={d.photo?.source}
          onPlay={() => d.actions.stepByStep(next.index)}
          onAsk={askAbout}
          asking={chat.opening}
        />
      ) : null}
    </View>
  );
}
