// A plant's page (roadmap 10 item 8; screens 3.5): the melt hero with its name, the status, the species with Change,
// the timeline of scans (each opening its result), its notes, and Scan. The growth curve comes with Insights in F7.
import { Image } from "expo-image";
import { useLocalSearchParams } from "expo-router";
import { Pressable, ScrollView, View, useWindowDimensions } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import {
  BAR_INK,
  BarButton,
  Button,
  Card,
  EmptyState,
  ErrorState,
  LineIcon,
  MeltPhoto,
  Section,
  Skeleton,
  StatusChip,
  Text,
  TopEdge,
  TopScrim,
  gutter,
  minTouch,
  radii,
  space,
  useHeroScroll,
  useTheme,
} from "@/ui";

import { usePlantDetail } from "../hooks/usePlantDetail";

export function PlantScreen() {
  const { plantId } = useLocalSearchParams<{ plantId: string }>();
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const v = usePlantDetail(plantId);
  const p = v.plant;
  const heroHeight = Math.round(width * 0.9);
  // Back floats at the top. Once the name reaches it, the ground goes behind it and Back takes the scheme's ink.
  const backTop = Math.max(insets.top - 2, 8);
  const edgeHeight = backTop + minTouch + space[2];
  const { past, scrollProps } = useHeroScroll(heroHeight - space[16] - edgeHeight);

  if (v.notFound || v.error) {
    return (
      <View style={{ flex: 1, backgroundColor: colors.background, justifyContent: "center", padding: gutter }}>
        {v.notFound ? (
          <EmptyState title="We can't find this" message="This plant was deleted, or you no longer have access to it." action={{ label: "Back", onPress: v.actions.back, variant: "secondary" }} />
        ) : (
          <ErrorState message="The plant didn't load." onRetry={v.retry} />
        )}
      </View>
    );
  }

  const overPhoto = Boolean(v.photo) && !past;
  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="plant-screen">
      <ScrollView {...scrollProps} contentContainerStyle={{ paddingBottom: insets.bottom + space[10] }}>
        {v.photo ? (
          <View>
            <MeltPhoto source={v.photo} accessibilityLabel={p ? `${p.name}, its latest photo` : ""} melt="hero" height={heroHeight} />
            <TopScrim />
          </View>
        ) : (
          <View style={{ height: insets.top + minTouch + space[6], backgroundColor: colors.wash }} />
        )}
        <View style={{ paddingHorizontal: gutter, marginTop: v.photo ? -space[16] : space[2], gap: space[2] }}>
          {p ? (
            <>
              <Text variant="display">{p.name}</Text>
              <View style={{ flexDirection: "row", alignItems: "center", gap: space[3], flexWrap: "wrap" }}>
                <StatusChip status={p.status} />
                {v.species ? (
                  <Text variant="subheadline" color="textMuted" style={{ flexShrink: 1 }}>
                    {[v.species.name, v.species.latin, v.species.status].filter(Boolean).join(" · ")}
                  </Text>
                ) : null}
                {v.can.changeSpecies ? (
                  <Pressable accessibilityRole="button" accessibilityLabel="Change the species" onPress={v.actions.changeSpecies} hitSlop={12} testID="plant-change-species">
                    <Text variant="subheadline" color="accent" emphasis>
                      Change
                    </Text>
                  </Pressable>
                ) : null}
              </View>
            </>
          ) : (
            <>
              <Skeleton height={40} width="60%" />
              <Skeleton height={20} width="80%" />
            </>
          )}
        </View>

        {v.can.scan && p ? (
          <View style={{ paddingHorizontal: gutter, marginTop: space[5] }}>
            <Button
              label={`Scan ${p.name}`}
              size="large"
              busy={v.scanning}
              onPress={v.actions.scan}
              icon={({ color, size }) => <LineIcon name="camera" color={color} size={size} />}
              testID="plant-scan"
            />
          </View>
        ) : null}

        <View style={{ paddingHorizontal: gutter, marginTop: space[6], gap: space[6] }}>
          <Section title="Timeline">
            {v.timeline.loading ? (
              <Skeleton height={64} radius={radii.card} />
            ) : v.timeline.rows.length === 0 ? (
              <Text variant="subheadline" color="textMuted" style={{ marginHorizontal: space[1] }}>
                No scans yet.
              </Text>
            ) : (
              <Card padded={false}>
                {v.timeline.rows.map((r, i) => (
                  <Pressable
                    key={r.scanId}
                    accessibilityRole="button"
                    accessibilityLabel={`${r.day}, ${r.title}`}
                    onPress={() => v.actions.open(r)}
                    style={({ pressed }) => ({
                      flexDirection: "row",
                      alignItems: "center",
                      gap: space[3],
                      paddingVertical: space[3],
                      paddingHorizontal: space[4],
                      borderTopWidth: i === 0 ? 0 : 1,
                      borderTopColor: colors.border,
                      opacity: pressed ? 0.6 : 1,
                    })}
                    testID={`timeline-${r.scanId}`}
                  >
                    <View style={{ width: 48, height: 48, borderRadius: radii.thumb, overflow: "hidden", backgroundColor: colors.surfaceMuted }}>
                      {r.thumb ? <Image source={r.thumb} contentFit="cover" style={{ flex: 1 }} accessible={false} /> : null}
                    </View>
                    <View style={{ flex: 1, gap: 2 }}>
                      <Text variant="body">{r.title}</Text>
                      <Text variant="footnote" color="textMuted">
                        {r.day}
                      </Text>
                    </View>
                    <LineIcon name="chevronRight" color={colors.textMuted} size={16} />
                  </Pressable>
                ))}
              </Card>
            )}
            {v.timeline.more ? <Button label="Show older scans" variant="plain" onPress={v.timeline.more} /> : null}
          </Section>

          <Section title="Notes" action={v.can.note ? { label: "Add a note", onPress: v.actions.addNote } : undefined}>
            {v.notes.length === 0 ? (
              <Text variant="subheadline" color="textMuted" style={{ marginHorizontal: space[1] }}>
                {v.can.note ? "Nothing noted yet. A note helps the plan and the chat, like “It's in a container”." : "Nothing noted yet."}
              </Text>
            ) : (
              <Card>
                {v.notes.map((n, i) => (
                  <View key={n.id} style={{ paddingVertical: space[2], borderTopWidth: i === 0 ? 0 : 1, borderTopColor: colors.border }}>
                    <Text variant="body">{n.text}</Text>
                    {n.shared ? null : (
                      <Text variant="footnote" color="textMuted">
                        Only you
                      </Text>
                    )}
                  </View>
                ))}
              </Card>
            )}
          </Section>
        </View>
      </ScrollView>

      {v.photo ? <TopEdge past={past} color={colors.background} separator={colors.border} height={edgeHeight} /> : null}
      <View pointerEvents="box-none" style={{ position: "absolute", top: backTop, left: 14 }}>
        {overPhoto ? (
          <BarButton accessibilityLabel="Back" onPress={v.actions.back} testID="plant-back">
            <LineIcon name="back" color={BAR_INK} size={22} strokeWidth={2} />
          </BarButton>
        ) : (
          <Pressable accessibilityRole="button" accessibilityLabel="Back" onPress={v.actions.back} style={{ width: minTouch, height: minTouch, alignItems: "center", justifyContent: "center" }} testID="plant-back">
            <LineIcon name="back" color={colors.text} size={22} strokeWidth={2} />
          </Pressable>
        )}
      </View>
    </View>
  );
}
