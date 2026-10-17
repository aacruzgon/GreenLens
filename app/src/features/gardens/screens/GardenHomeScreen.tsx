import { View } from "react-native";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { describeError, qk, tasks as tasksApi, usePlants, useTasks, useWeather } from "@/api";
import { can } from "@/domain/permissions";
import { localToday } from "@/domain/dates";
import { Banner, Button, Card, EmptyState, ErrorState, ListItem, Screen, Section, Skeleton, Text, useTheme } from "@/ui";

import { PlantCard } from "../components/PlantCard";
import { WeatherStrip } from "../components/WeatherStrip";
import { useActiveGarden } from "../hooks/useActiveGarden";

/** Garden home (screens.md 3.1): weather, alerts, today's tasks and the plants. */
export function GardenHomeScreen() {
  const t = useTheme();
  const queryClient = useQueryClient();
  const { garden } = useActiveGarden();
  const plants = usePlants(garden?.id);
  const weather = useWeather(garden?.id);
  const openTasks = useTasks(garden?.id, { state: "open" });
  const canScan = can(garden?.my_role, "scan.create");
  const canTask = can(garden?.my_role, "task.write");

  const today = garden ? localToday(garden.time_zone) : "";
  const alerts = (openTasks.data?.items ?? []).filter((x) => x.source === "forecast_alert");
  const dueToday = (openTasks.data?.items ?? []).filter((x) => x.source !== "forecast_alert" && (!x.due || x.due <= today)).slice(0, 4);

  const refreshing = plants.isRefetching || weather.isRefetching;
  const refresh = () => void Promise.all([plants.refetch(), weather.refetch(), openTasks.refetch()]);

  const completeTask = useMutation({
    mutationFn: (id: string) => tasksApi.update(id, { done: true }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: qk.tasks(garden!.id) }),
  });
  const complete = (id: string) => completeTask.mutate(id);
  const taskError = completeTask.error ? describeError(completeTask.error) : undefined;

  return (
    <Screen
      onRefresh={refresh}
      refreshing={refreshing}
    >
      {weather.data ? (
        <WeatherStrip weather={weather.data.weather} locationLabel={garden?.location_label} />
      ) : weather.isLoading ? (
        <Skeleton height={150} radius={t.radius.lg} />
      ) : null}

      {taskError ? <Banner tone="danger" title="We couldn't mark that done" body={taskError.message} action={{ label: "Dismiss", onPress: () => completeTask.reset() }} /> : null}

      {alerts.map((a) => (
        <Banner key={a.id} tone="info" icon="snow" title={a.title} body={a.detail} action={canTask ? { label: "Done", onPress: () => complete(a.id) } : undefined} />
      ))}

      {dueToday.length > 0 ? (
        <Section title="Today">
          <Card padded={false} style={{ paddingHorizontal: t.space.lg }}>
            {dueToday.map((task) => (
              <ListItem
                key={task.id}
                title={task.title}
                subtitle={task.detail}
                chevron={false}
                trailing={
                  canTask ? <Button label="Done" variant="secondary" onPress={() => complete(task.id)} accessibilityLabel={`Mark done: ${task.title}`} /> : undefined
                }
              />
            ))}
          </Card>
        </Section>
      ) : null}

      <Section title="Plants">
        {plants.isLoading ? (
          <View style={{ flexDirection: "row", gap: t.space.md }}>
            <View style={{ flex: 1 }}>
              <Skeleton height={220} radius={t.radius.lg} />
            </View>
            <View style={{ flex: 1 }}>
              <Skeleton height={220} radius={t.radius.lg} />
            </View>
          </View>
        ) : plants.error ? (
          <ErrorState title="We couldn't load your plants" action={{ label: "Try again", onPress: () => void plants.refetch() }} />
        ) : (plants.data?.items.length ?? 0) === 0 ? (
          <EmptyState
            title="No plants yet"
            body="Scan a plant to add it, and we'll check its health."
          />
        ) : (
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: t.space.md }}>
            {plants.data!.items.map((p) => (
              <View key={p.id} style={{ width: "47%", flexGrow: 1 }}>
                <PlantCard plant={p} />
              </View>
            ))}
          </View>
        )}
      </Section>
      {!canScan && garden ? (
        <Text variant="footnote" color="textMuted" align="center">
          {"You can view this garden. Ask an owner if you'd like to scan plants too."}
        </Text>
      ) : null}
    </Screen>
  );
}
