// A farm record's text in a sheet (EON E10.3, decision 10): an `era` record is labelled as Era's synthetic demo data
// and never offers its link; a vendor's record offers "Open in Slack/Drive". Shared with the plan (scans) through
// the chat feature's index.
import { Modal, ScrollView, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { formatDateTime } from "@/domain/format";
import type { FarmCitation } from "@/domain/types";
import { links } from "@/platform";
import { Banner, Button, Text, gutter, space, useTheme } from "@/ui";

import { SYSTEM_WORD } from "../model/farm";

/** `vendor`: the record's connector is known and real, so its link opens; anything else reads as demo data. */
export function FarmRecordSheet({ citation, vendor, timeZone, onClose }: { citation: FarmCitation; vendor: boolean; timeZone: string; onClose: () => void }) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const meta = [SYSTEM_WORD[citation.system], citation.author, formatDateTime(citation.at, timeZone)].filter(Boolean).join(" · ");
  return (
    <Modal visible animationType="slide" presentationStyle="pageSheet" onRequestClose={onClose}>
      <View style={{ flex: 1, backgroundColor: colors.background }} testID="farm-record-sheet">
        <ScrollView contentContainerStyle={{ padding: gutter, paddingTop: space[6], gap: space[4], paddingBottom: insets.bottom + space[6] }}>
          <Text variant="title3" emphasis accessibilityRole="header">
            {citation.title}
          </Text>
          <Text variant="footnote" color="textMuted">
            {meta}
          </Text>
          {!vendor ? <Banner tone="info" title="Demo data from Era by Eon (synthetic)" message="This record comes from Era's simulator, not a real farm." /> : null}
          {citation.text ? (
            <Text variant="body" selectable>
              {citation.text}
            </Text>
          ) : (
            <Text variant="body" color="textMuted">
              The record&apos;s text isn&apos;t available in the app yet.
            </Text>
          )}
          {vendor ? <Button label={`Open in ${SYSTEM_WORD[citation.system]}`} variant="secondary" size="large" onPress={() => void links.openExternal(citation.link)} /> : null}
          <Button label="Done" size="large" onPress={onClose} />
        </ScrollView>
      </View>
    </Modal>
  );
}
