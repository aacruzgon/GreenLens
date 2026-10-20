// The garden home (screens 3.1, roadmap F1), drawn as the Conservatory mockup's home: the hero photo
// melting into the wash under the garden's name, the weather, forecast alerts, the plants sorted by health,
// today's tasks and the garden's notes. The scan button and its sheet live in the tabs layout; the content
// leaves room for them at the bottom.
import { useState } from "react";
import { RefreshControl, ScrollView, View, type NativeScrollEvent, type NativeSyntheticEvent } from "react-native";
import { usePathname } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { TaskRow } from "@/features/tasks";
import { Card, EmptyState, Enter, ErrorState, Section, Skeleton, Text, WASH_HEIGHT, Wash, gutter, space, useTheme } from "@/ui";

import { AlertCard } from "../components/AlertCard";
import { GardenHero } from "../components/GardenHero";
import { GardenNotes } from "../components/GardenNotes";
import { HeroControls, chooseGarden } from "../components/HeroControls";
import { PlantCarousel, PlantCarouselSkeleton } from "../components/PlantCarousel";
import { WeatherCard } from "../components/WeatherCard";
import { useActiveGardenId } from "../hooks/useActiveGarden";
import { useGardenHome } from "../hooks/useGardenHome";
import { emptyPlantsTitle, mixHex } from "../model/home";

/** styles.css: .h-hero under data-melt is 470 pt tall. */
const HERO_HEIGHT = 470;
/** Room for the tab bar and the scan button beside it (iOS 26's floating bar is about 62 pt plus its margin). */
const TAB_BAR_CLEARANCE = 96;
/** styles.css .sec: margin-top 26. */
const SECTION_GAP = 26;
/** Entrance stagger between sections (the mockup's --i steps). */
const STAGGER = 60;

function TodaySkeleton() {
  return (
    <Card testID="today-loading" style={{ gap: space[4] }}>
      {[0, 1].map((i) => (
        <View key={i} style={{ flexDirection: "row", gap: space[3] }}>
          <Skeleton width={24} height={24} radius={12} />
          <View style={{ flex: 1, gap: 6 }}>
            <Skeleton height={16} width="80%" />
            <Skeleton height={12} width="50%" />
          </View>
        </View>
      ))}
    </Card>
  );
}

export function GardenHomeScreen() {
  const gardenId = useActiveGardenId();
  const view = useGardenHome(gardenId);
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  // The home is the tabs' index; another tab or a sheet over it has its own path and its own status bar.
  const focused = usePathname() === "/";
  const [overHero, setOverHero] = useState(true);
  const [pulling, setPulling] = useState(false);

  const hasPhoto = Boolean(view.hero.photo);
  // The photo melts into the wash as it stands at the hero's bottom edge, so the page below has no seam.
  const ground = mixHex(colors.wash, colors.background, HERO_HEIGHT / WASH_HEIGHT);

  const onScroll = (e: NativeSyntheticEvent<NativeScrollEvent>) => {
    setOverHero(e.nativeEvent.contentOffset.y < HERO_HEIGHT * 0.45);
  };

  const onRefresh = async () => {
    setPulling(true);
    try {
      await view.actions.refresh();
    } finally {
      setPulling(false);
    }
  };

  if (view.garden.status === "error") {
    return (
      <View style={{ flex: 1, backgroundColor: colors.background, justifyContent: "center" }}>
        <ErrorState title="Couldn't load your garden" message={view.garden.message} onRetry={view.garden.retry} />
      </View>
    );
  }

  const g = view.garden.status === "ready" ? view.garden : undefined;
  const plants = view.plants;
  const today = view.today;
  const emptyGarden = plants.status === "ready" && plants.items.length === 0;
  const gardens = view.gardens.items;
  // The switcher is the ▾ on the title, only with two or more gardens (screens 3.1).
  const onSwitchGarden = gardens.length >= 2 ? () => chooseGarden(gardens, view.actions.switchGarden) : undefined;

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="garden-home">
      {/* Light over the photo (roadmap F1 item 1); the scheme's own once it has scrolled away. */}
      {focused ? <StatusBar style={hasPhoto && overHero ? "light" : "auto"} /> : null}
      <ScrollView
        contentInsetAdjustmentBehavior="never"
        onScroll={onScroll}
        scrollEventThrottle={32}
        refreshControl={<RefreshControl refreshing={pulling} onRefresh={() => void onRefresh()} />}
        contentContainerStyle={{ flexGrow: 1, paddingBottom: insets.bottom + TAB_BAR_CLEARANCE }}
      >
        <Wash />
        <GardenHero
          name={g?.name}
          eyebrow={g?.eyebrow}
          weatherLine={g?.weatherLine}
          note={g?.note}
          onSwitchGarden={onSwitchGarden}
          photo={view.hero.photo}
          photoLabel={view.hero.label}
          ground={ground}
          height={HERO_HEIGHT}
          topInset={insets.top}
        />
        <HeroControls
          onAddPlant={view.can.addPlant ? view.actions.addPlant : undefined}
          overPhoto={hasPhoto}
          topInset={insets.top}
        />

        {/* .sec: 16 pt gutters and 26 pt between sections, the first included. */}
        <View style={{ paddingHorizontal: gutter, paddingTop: SECTION_GAP, gap: SECTION_GAP }}>
          <Enter delay={STAGGER}>
            <WeatherCard state={view.weather} />
          </Enter>

          {view.alerts.length > 0 ? (
            <Enter delay={STAGGER * 2} style={{ gap: space[3] }}>
              {view.alerts.map((a) => (
                <AlertCard
                  key={a.id}
                  testID={`alert-${a.id}`}
                  title={a.title}
                  detail={a.detail}
                  onDone={view.can.writeTasks ? () => view.actions.completeTask(a.id) : undefined}
                />
              ))}
            </Enter>
          ) : null}

          <Enter delay={STAGGER * 3}>
            <Section title="Plants">
              {plants.status === "loading" ? (
                <PlantCarouselSkeleton />
              ) : plants.status === "error" ? (
                <ErrorState title="Couldn't load your plants" message={plants.message} onRetry={plants.retry} />
              ) : emptyGarden ? (
                <EmptyState title={emptyPlantsTitle(view.can.scan)} style={{ paddingVertical: space[6] }} />
              ) : (
                <PlantCarousel plants={plants.items} />
              )}
            </Section>
          </Enter>

          {emptyGarden ? null : (
            <Enter delay={STAGGER * 4}>
              <Section title="Today">
                {today.status === "loading" ? (
                  <TodaySkeleton />
                ) : today.status === "error" ? (
                  <ErrorState title="Couldn't load your tasks" message={today.message} onRetry={today.retry} />
                ) : today.items.length === 0 ? (
                  <Card>
                    <Text variant="subheadline" color="textMuted">
                      Nothing is due today.
                    </Text>
                  </Card>
                ) : (
                  // .card.list: no padding, each row 12 × 16, full-width rules between rows.
                  <Card padded={false}>
                    {today.items.map((t, i) => (
                      <View key={t.id} style={i > 0 ? { borderTopWidth: 1, borderTopColor: colors.border } : undefined}>
                        <TaskRow
                          testID={`task-${t.id}`}
                          title={t.title}
                          meta={t.meta}
                          done={t.done}
                          onCheck={view.can.writeTasks ? () => view.actions.completeTask(t.id) : undefined}
                          onPress={t.scanHint && !t.done ? () => view.actions.openTask(t.id) : undefined}
                          pressHint={t.scanHint}
                        />
                      </View>
                    ))}
                  </Card>
                )}
              </Section>
            </Enter>
          )}

          <Enter delay={STAGGER * 5}>
            <GardenNotes state={view.notes} onAddNote={view.can.note ? view.actions.addNote : undefined} />
          </Enter>
        </View>
      </ScrollView>
    </View>
  );
}
