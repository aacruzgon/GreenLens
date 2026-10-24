import { Switch, View } from "react-native";

import { envResult } from "@/config/env";
import { Banner, Button, Card, ListItem, Screen, Section, Skeleton, Text, TextField, space, useTheme } from "@/ui";

import { DemoControls } from "../components/DemoControls";
import { DeviceTools } from "../components/DeviceTools";
import { useSettings } from "../hooks/useSettings";

// Settings (screens 3.9): the account, the gardens (switch, manage, members, new), sign-out and account deletion.
// Notifications are shown only in builds with push, and there is none yet (roadmap F7 item 4).
export function SettingsScreen() {
  const { colors } = useTheme();
  const vm = useSettings();
  const { actions, session } = vm;
  const mock = envResult.ok && envResult.env.apiMode === "mock";
  const divider = { borderTopWidth: 1, borderTopColor: colors.border };
  const consent = vm.me?.location_consent ?? false;

  return (
    <Screen>
      <View style={{ gap: space[6], paddingTop: space[4] }}>
        <Text variant="largeTitle" emphasis accessibilityRole="header">
          Settings
        </Text>

        <Section title="Account">
          <Card padded={false}>
            {vm.editingName ? (
              <View style={{ padding: space[4], gap: space[3] }}>
                <TextField label="Your name" value={vm.name} onChangeText={vm.setName} autoComplete="name" autoFocus testID="settings-name" />
                <View style={{ flexDirection: "row", gap: space[2] }}>
                  <Button label="Save" variant="secondary" size="small" onPress={actions.saveName} busy={vm.savingName} disabled={!vm.name.trim()} testID="settings-name-save" />
                  <Button label="Cancel" variant="plain" size="small" onPress={actions.cancelEditingName} />
                </View>
              </View>
            ) : (
              <ListItem
                title={vm.me?.display_name || "Your account"}
                subtitle={session.mode === "demo" ? "Demo account" : undefined}
                trailing={<Button label="Edit" variant="plain" size="small" accessibilityLabel="Edit your name" onPress={actions.startEditingName} testID="settings-name-edit" />}
              />
            )}
            <ListItem
              title="Share my location"
              subtitle={consent ? "Gardens can use your phone's location." : "Gardens you own are rounded to about 10 km."}
              style={divider}
              accessibilityLabel="Share my location"
              trailing={<Switch value={consent} onValueChange={actions.setConsent} accessibilityLabel="Share my location" testID="settings-consent" />}
            />
          </Card>
          {session.secondFactorProblem ? (
            <Banner
              tone="watch"
              title="Sign-in codes by text aren't on yet"
              message={`Signing in asks only for your password until this works. ${session.secondFactorProblem.message}`}
              action={{ label: "Try again", onPress: session.secondFactorProblem.retry }}
            />
          ) : null}
        </Section>

        <Section title="Gardens">
          {vm.gardensLoading ? (
            <Skeleton height={52} />
          ) : (
            <Card padded={false}>
              {vm.rows.map((row, i) => (
                <ListItem
                  key={row.garden.id}
                  title={row.garden.name}
                  subtitle={row.subtitle}
                  style={i > 0 ? divider : undefined}
                  chevron={false}
                  accessibilityHint={row.active ? undefined : "Opens this garden"}
                  onPress={() => actions.open(row.garden.id)}
                  trailing={
                    <Button
                      label={row.manage.label}
                      variant="plain"
                      size="small"
                      accessibilityLabel={`${row.manage.label}, ${row.garden.name}`}
                      onPress={row.manage.onPress}
                      testID={`garden-${row.manage.label.toLowerCase()}-${row.garden.id}`}
                    />
                  }
                />
              ))}
            </Card>
          )}
          <Button label="New garden" variant="secondary" onPress={actions.newGarden} testID="garden-new" />
        </Section>

        {vm.problem ? (
          <Text variant="footnote" color="danger" accessibilityLiveRegion="polite">
            {vm.problem}
          </Text>
        ) : null}

        {mock ? <DemoControls /> : null}
        <DeviceTools />
        <View style={{ gap: space[3] }}>
          <Button label="Sign out" variant="secondary" onPress={actions.signOut} busy={vm.signingOut} />
          <Button label="Delete account" variant="plain" onPress={actions.deleteAccount} testID="account-delete" />
        </View>
        {mock ? (
          <Text variant="footnote" color="textMuted" align="center">
            Sample data. Nothing here reaches a server.
          </Text>
        ) : null}
      </View>
    </Screen>
  );
}
