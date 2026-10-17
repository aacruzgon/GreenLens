import { Platform, View } from "react-native";
import { useQueryClient } from "@tanstack/react-query";

import { useMe } from "@/api";
import { mockDb, useMockScenario, type Scenario } from "@/api/mock/transport";
import { env } from "@/config/env";
import { useSession } from "@/features/auth";
import { useActiveGarden } from "@/features/gardens";
import { Button, Card, Chip, ListItem, Screen, Section, Text, useTheme } from "@/ui";

const SCENARIOS: { id: Scenario; label: string }[] = [
  { id: "early_blight", label: "Early blight" },
  { id: "question", label: "Asks a question" },
  { id: "healthy", label: "Healthy plant" },
];

/** Settings (screens.md 3.9), with the mock's demo controls when the app runs on fixtures. */
export function SettingsScreen() {
  const t = useTheme();
  const session = useSession();
  const me = useMe();
  const { garden } = useActiveGarden();
  const queryClient = useQueryClient();
  const { scenario, setScenario } = useMockScenario();

  return (
    <Screen>
      <Section title="Account">
        <Card padded={false} style={{ paddingHorizontal: t.space.lg }}>
          <ListItem title={me.data?.display_name || "No name yet"} subtitle={session.email} />
          {garden ? <ListItem title={garden.name} subtitle={`${garden.location_label} · ${garden.my_role}`} /> : null}
        </Card>
      </Section>

      {env.apiMode === "mock" ? (
        <Section title="Demo controls">
          <Card style={{ gap: t.space.md }}>
            <Text variant="footnote" color="textMuted">
              The app is running on sample data (mock mode). These set what the next scan shows.
            </Text>
            <View style={{ flexDirection: "row", flexWrap: "wrap", gap: t.space.sm }}>
              {SCENARIOS.map((s) => (
                <Button
                  key={s.id}
                  label={s.label}
                  variant={scenario === s.id ? "primary" : "secondary"}
                  selected={scenario === s.id}
                  onPress={() => setScenario(s.id)}
                />
              ))}
            </View>
            <Button label="Fail the next upload's quality check" variant="plain" onPress={() => (mockDb.failNextUpload = true)} />
            <Button
              label="Reset sample data"
              variant="plain"
              onPress={() => {
                mockDb.reset(true);
                void queryClient.invalidateQueries();
              }}
            />
            <Chip label={`Running on ${Platform.OS}`} />
          </Card>
        </Section>
      ) : null}

      <Button label="Sign out" variant="destructive" onPress={() => void session.signOut()} />
    </Screen>
  );
}
