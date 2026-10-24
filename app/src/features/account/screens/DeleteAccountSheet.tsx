import { View } from "react-native";

import { Button, Sheet, Text, TextField, space } from "@/ui";

import { useDeleteAccount } from "../hooks/useDeleteAccount";
import { CONFIRM_WORD, DELETION_POINTS } from "../model/deletion";

// account/delete (screens 1; flows 13): what is deleted and what stays with shared gardens (architecture 5), then
// the word typed to confirm. On success the app signs out and says what happened to each garden.
export function DeleteAccountSheet() {
  const vm = useDeleteAccount();
  return (
    <Sheet
      title="Delete your account"
      footer={<Button label="Delete account" variant="secondary" onPress={() => void vm.run()} disabled={!vm.ready} busy={vm.busy} testID="account-delete-confirm" />}
    >
      <View style={{ gap: space[2] }}>
        {DELETION_POINTS.map((point) => (
          <Text key={point} variant="body">
            {`•  ${point}`}
          </Text>
        ))}
      </View>
      <Text variant="footnote" color="textMuted">
        Deletion starts at once and finishes in the background. It can&apos;t be undone.
      </Text>
      <TextField
        label={`Type "${CONFIRM_WORD}" to confirm`}
        value={vm.typed}
        onChangeText={vm.setTyped}
        autoCapitalize="none"
        autoCorrect={false}
        testID="account-delete-word"
      />
      {vm.problem ? (
        <Text variant="footnote" color="danger" accessibilityLiveRegion="polite">
          {vm.problem}
        </Text>
      ) : null}
    </Sheet>
  );
}
