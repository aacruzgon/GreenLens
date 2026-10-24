import { View } from "react-native";

import { Button, Card, ErrorState, ListItem, Screen, Section, Skeleton, Text, TextField, space, useTheme } from "@/ui";

import { BackHeader } from "../components/BackHeader";
import { LocationPicker } from "../components/LocationPicker";
import { useGardenEdit } from "../hooks/useGardenEdit";

// Manage a garden (screens 3.9, owners): its name, where it is ("Toronto, ON · about 1 km"), its members and farm
// connectors, and deleting it. The garden's location is the only one the app stores (contracts 9).
export function GardenEditScreen({ gardenId }: { gardenId: string }) {
  const { colors } = useTheme();
  const vm = useGardenEdit(gardenId);
  const { actions } = vm;

  let body;
  if (vm.loading) {
    body = <Skeleton height={120} />;
  } else if (vm.error || !vm.garden) {
    body = <ErrorState onRetry={vm.retry} />;
  } else if (!vm.isOwner) {
    body = (
      <Text variant="body" color="textMuted">
        Only the garden&apos;s owners change it.
      </Text>
    );
  } else {
    body = (
      <>
        <View style={{ gap: space[3] }}>
          <TextField label="Garden name" value={vm.name} onChangeText={vm.setName} testID="garden-name" />
          {vm.nameChanged ? <Button label="Save name" variant="secondary" onPress={actions.saveName} busy={vm.saving} testID="garden-name-save" /> : null}
        </View>

        <Section title="Location">
          {vm.changingPlace ? (
            <View style={{ gap: space[3] }}>
              <LocationPicker input={vm.where} title="Where is it now?" />
              <Text variant="footnote" color="textMuted">
                We use it for weather and soil.
              </Text>
              <Button label="Save location" variant="secondary" onPress={actions.saveLocation} disabled={!vm.where.location} busy={vm.saving} testID="garden-location-save" />
              <Button label="Cancel" variant="plain" onPress={() => vm.setChangingPlace(false)} />
            </View>
          ) : (
            <Card padded={false}>
              <ListItem
                title={vm.location}
                trailing={<Button label="Change" variant="plain" size="small" onPress={() => vm.setChangingPlace(true)} testID="garden-location-change" />}
              />
            </Card>
          )}
        </Section>

        <Card padded={false}>
          <ListItem title="Members" subtitle={`${vm.garden.member_count} ${vm.garden.member_count === 1 ? "member" : "members"}`} onPress={actions.members} />
          <ListItem title="Farm connectors" subtitle="Slack and Google Drive" onPress={actions.connectors} style={{ borderTopWidth: 1, borderTopColor: colors.border }} />
        </Card>

        {vm.problem ? (
          <Text variant="footnote" color="danger" accessibilityLiveRegion="polite">
            {vm.problem}
          </Text>
        ) : null}

        <Button label="Delete garden" variant="secondary" onPress={actions.deleteGarden} busy={vm.deleting} testID="garden-delete" />
      </>
    );
  }

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="garden-edit-screen">
      <BackHeader title={vm.garden?.name ?? "Garden"} onBack={actions.back} />
      <Screen underHeader>
        <View style={{ gap: space[6], paddingTop: space[4] }}>{body}</View>
      </Screen>
    </View>
  );
}
