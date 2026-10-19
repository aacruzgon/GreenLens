import { useCallback, useState } from "react";
import { ScrollView, View } from "react-native";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useFocusEffect } from "expo-router";
import { setStatusBarStyle } from "expo-status-bar";

import { describeError, qk, tasks as tasksApi, usePlants, useTasks, useWeather } from "@/api";
import { can } from "@/domain/permissions";
import { localToday } from "@/domain/dates";
import type { Plant, PlantStatus } from "@/domain/types";
import { Banner, Card, EmptyState, Enter, ErrorState, Screen, Section, Skeleton, Text, useTheme } from "@/ui";

import { GardenHero } from "../components/GardenHero";
import { PlantCard } from "../components/PlantCard";
import { TaskCheckRow } from "../components/TaskCheckRow";
import { WeatherStrip } from "../components/WeatherStrip";
import { useActiveGarden } from "../hooks/useActiveGarden";

// Plants by health: the ones that need you first (design 5, garden home).
const ORDER: Record<PlantStatus, number> = { sick: 0, watch: 1, healthy: 2, dead: 3 };
const byHealth = (a: Plant, b: Plant) => ORDER[a.status] - ORDER[b.status];

/** Garden home (screens.md 3.1) in the Conservatory direction (design 3.1): the melt, weather, alerts, plants, today. */
export function GardenHomeScreen() {
  const t = useTheme();
  const queryClient = useQueryClient();
  const { garden } = useActiveGarden();
  const plants = usePlants(garden?.id);
  const weather = useWeather(garden?.id);
  const openTasks = useTasks(garden?.id, { state: "open" });
  const canScan = can(garden?.my_role, "scan.create");
  const canTask = can(garden?.my_role, "task.write");
  const [checked, setChecked] = useState<ReadonlySet<string>>(new Set());

  const today = garden ? localToday(garden.time_zone) : "";
  const alerts = (openTasks.data?.items ?? []).filter((x) => x.source === "forecast_alert");
  const dueToday = (openTasks.data?.items ?? []).filter((x) => x.source !== "forecast_alert" && (!x.due || x.due <= today)).slice(0, 4);
  const sorted = [...(plants.data?.items ?? [])].sort(byHealth);
  const heroPlant = sorted.find((p) => p.latest_scan?.thumbnail_url);
  const heroPhoto = heroPlant?.latest_scan?.thumbnail_url
    ? { uri: heroPlant.latest_scan.thumbnail_url, cacheKey: heroPlant.latest_scan.thumbnail_image_id }
    : undefined;

  // The top of the hero is a photo, so the status bar is light while home is showing.
  const overPhoto = !!heroPhoto;
  const scheme = t.scheme;
  useFocusEffect(
    useCallback(() => {
      if (!overPhoto) return;
      setStatusBarStyle("light");
      return () => setStatusBarStyle(scheme === "dark" ? "light" : "dark");
    }, [overPhoto, scheme]),
  );

  const refreshing = plants.isRefetching || weather.isRefetching;
  const refresh = () => void Promise.all([plants.refetch(), weather.refetch(), openTasks.refetch()]);

  const completeTask = useMutation({
    mutationFn: (id: string) => tasksApi.update(id, { done: true }),
    onMutate: (id) => setChecked((s) => new Set(s).add(id)),
    onError: (_, id) =>
      setChecked((s) => {
        const next = new Set(s);
        next.delete(id);
        return next;
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: qk.tasks(garden!.id) }),
  });
  const complete = (id: string) => completeTask.mutate(id);
  const taskError = completeTask.error ? describeError(completeTask.error) : undefined;

  return (
    <Screen onRefresh={refresh} refreshing={refreshing} hero={garden ? <GardenHero garden={garden} weather={weather.data?.weather} photo={heroPhoto} /> : undefined}>
      <Enter index={1}>
        {weather.data ? (
          <WeatherStrip weather={weather.data.weather} />
        ) : weather.isLoading ? (
          <Skeleton height={190} radius={t.radius.card} />
        ) : null}
      </Enter>

      {taskError ? <Banner tone="danger" title="We couldn't mark that done" body={taskError.message} action={{ label: "Dismiss", onPress: () => completeTask.reset() }} /> : null}

      {alerts.map((a, i) => (
        <Enter key={a.id} index={2 + i}>
          <Banner tone="info" icon="snow-outline" title={a.title} body={a.detail} action={canTask ? { label: "Done", onPress: () => complete(a.id) } : undefined} />
        </Enter>
      ))}

      <Enter index={3}>
        <Section title="Plants">
          {plants.isLoading ? (
            <View style={{ flexDirection: "row", gap: t.space.md }}>
              <Skeleton height={232} width={170} radius={t.radius.card} />
              <Skeleton height={232} width={170} radius={t.radius.card} />
            </View>
          ) : plants.error ? (
            <ErrorState title="We couldn't load your plants" action={{ label: "Try again", onPress: () => void plants.refetch() }} />
          ) : sorted.length === 0 ? (
            <EmptyState title="No plants yet" body="Scan a plant to add it, and we'll check its health." />
          ) : (
            <ScrollView
              horizontal
              showsHorizontalScrollIndicator={false}
              snapToInterval={170 + t.space.md}
              decelerationRate="fast"
              // Room around the cards for their shadows, which the scroll view would otherwise clip.
              style={{ marginHorizontal: -t.space.lg, marginVertical: -t.space.lg }}
              contentContainerStyle={{ paddingHorizontal: t.space.lg, paddingTop: t.space.lg, paddingBottom: t.space.xxl, gap: t.space.md }}
            >
              {sorted.map((p) => (
                <PlantCard key={p.id} plant={p} />
              ))}
            </ScrollView>
          )}
        </Section>
      </Enter>

      {dueToday.length > 0 ? (
        <Enter index={4}>
          <Section title="Today">
            <Card padded={false} style={{ paddingHorizontal: t.space.lg }}>
              {dueToday.map((task, i) => (
                <View key={task.id} style={i > 0 ? { borderTopWidth: 1, borderTopColor: t.colors.border } : undefined}>
                  <TaskCheckRow task={task} done={checked.has(task.id)} onCheck={canTask ? () => complete(task.id) : undefined} />
                </View>
              ))}
            </Card>
          </Section>
        </Enter>
      ) : null}

      {!canScan && garden ? (
        <Text variant="footnote" color="textMuted" align="center">
          {"You can view this garden. Ask an owner if you'd like to scan plants too."}
        </Text>
      ) : null}
    </Screen>
  );
}
