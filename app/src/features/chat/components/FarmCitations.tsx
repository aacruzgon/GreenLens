// The farm records an answer checked (EON E10.3; contracts 8, `farm_citations`): a vendor's record opens in Slack
// or Drive; an `era` record opens its text in a sheet, labelled as Era's synthetic data (EON decision 10).
import { useState } from "react";
import { Modal, Pressable, ScrollView, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useConnectors } from "@/api";
import type { FarmCitationWithText } from "@/domain/proposed";
import { formatDateTime } from "@/domain/format";
import { links } from "@/platform";
import { Banner, Button, LineIcon, Text, gutter, minTouch, space, useTheme } from "@/ui";

import { isEraRecord, openFarmCitation, SYSTEM_WORD } from "../model/farm";

export interface FarmCitationsProps {
  citations: readonly FarmCitationWithText[];
  gardenId: string;
  timeZone: string;
}

function RecordSheet({ citation, era, timeZone, onClose }: { citation: FarmCitationWithText; era: boolean; timeZone: string; onClose: () => void }) {
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
          {era ? <Banner tone="info" title="Demo data from Era by Eon (synthetic)" message="This record comes from Era's simulator, not a real farm." /> : null}
          {citation.text ? (
            <Text variant="body" selectable>
              {citation.text}
            </Text>
          ) : (
            <Text variant="body" color="textMuted">
              The record&apos;s text isn&apos;t available in the app yet.
            </Text>
          )}
          {!era ? <Button label={`Open in ${SYSTEM_WORD[citation.system]}`} variant="secondary" onPress={() => void links.openExternal(citation.link)} /> : null}
          <Button label="Done" onPress={onClose} />
        </ScrollView>
      </View>
    </Modal>
  );
}

export function FarmCitations({ citations, gardenId, timeZone }: FarmCitationsProps) {
  const { colors } = useTheme();
  const connectors = useConnectors(gardenId);
  const list = connectors.data?.connectors;
  const [open, setOpen] = useState<FarmCitationWithText>();
  if (citations.length === 0) return null;
  return (
    <View testID="farm-citations">
      <Text variant="caption1" color="textMuted" emphasis style={{ textTransform: "uppercase", letterSpacing: 0.6 }}>
        Records checked
      </Text>
      {citations.map((c) => {
        const era = list ? isEraRecord(c, list) : false;
        return (
          <Pressable
            key={c.record_id}
            accessibilityRole={era ? "button" : "link"}
            accessibilityLabel={`${c.title}, ${SYSTEM_WORD[c.system]}${c.author ? `, ${c.author}` : ""}${era ? ", demo data" : ""}`}
            accessibilityHint={era ? "Opens the record's text" : `Opens in ${SYSTEM_WORD[c.system]}`}
            onPress={() => {
              const action = openFarmCitation(c, list);
              if (action.kind === "external") void links.openExternal(action.url);
              else setOpen(action.citation);
            }}
            style={({ pressed }) => ({ flexDirection: "row", alignItems: "center", gap: space[2], minHeight: minTouch, opacity: pressed ? 0.6 : 1 })}
          >
            <LineIcon name={c.system === "slack" ? "chat" : "book"} color={colors.textMuted} size={16} />
            <Text variant="subheadline" style={{ flex: 1 }} numberOfLines={1}>
              {c.title}
              {c.author ? <Text variant="subheadline" color="textMuted">{` · ${c.author}`}</Text> : null}
            </Text>
            {era ? (
              <Text variant="caption2" color="textMuted">
                DEMO
              </Text>
            ) : (
              <LineIcon name="external" color={colors.textMuted} size={14} />
            )}
          </Pressable>
        );
      })}
      {open ? <RecordSheet citation={open} era={list ? isEraRecord(open, list) : true} timeZone={timeZone} onClose={() => setOpen(undefined)} /> : null}
    </View>
  );
}
