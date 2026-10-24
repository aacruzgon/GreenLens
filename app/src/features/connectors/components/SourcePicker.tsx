// The source picker (docs/connectors/README.md 2, item 2, and 5): the channels the bot is in, up to 10, or one
// Drive folder, with the consent text. A page sheet with Cancel and Save (HIG Sheets, best practices: a Done pairs
// with a Cancel); Save is the sheet's one green pill.
import { useState } from "react";
import { ActivityIndicator, Modal, Pressable, ScrollView, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { isApiError, useAvailableSources, useSetConnectorSources } from "@/api";
import type { Connector } from "@/domain/types";
import { Banner, Button, ErrorState, LineIcon, Text, gutter, minTouch, space, useCardStyle, useTheme } from "@/ui";

import { CONSENT, PICKER_NOTE, errorText, sourceName } from "../model/view";

const MAX: Record<Connector["kind"], number> = { slack: 10, google_drive: 1 };

export interface SourcePickerProps {
  connector: Connector;
  onClose: () => void;
}

export function SourcePicker({ connector, onClose }: SourcePickerProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const card = useCardStyle();
  const available = useAvailableSources(connector.id);
  const save = useSetConnectorSources();
  const [chosen, setChosen] = useState<string[]>(() => connector.sources.map((s) => s.id));
  const max = MAX[connector.kind];
  const single = max === 1;

  const toggle = (id: string) =>
    setChosen((now) => (single ? [id] : now.includes(id) ? now.filter((x) => x !== id) : now.length < max ? [...now, id] : now));

  const onSave = () =>
    save.mutate({ id: connector.id, body: { sources: chosen.map((id) => ({ id })) } }, { onSuccess: onClose });

  const title = connector.kind === "slack" ? "Choose channels" : "Choose a folder";
  return (
    <Modal visible animationType="slide" presentationStyle="pageSheet" onRequestClose={onClose}>
      <View style={{ flex: 1, backgroundColor: colors.background }} testID="source-picker">
        <View style={{ flexDirection: "row", alignItems: "center", paddingHorizontal: space[2], paddingTop: space[2], minHeight: minTouch + space[2] }}>
          <Button label="Cancel" variant="plain" onPress={onClose} />
          <Text variant="headline" accessibilityRole="header" style={{ flex: 1, textAlign: "center", marginRight: minTouch + space[4] }}>
            {title}
          </Text>
        </View>
        <ScrollView contentContainerStyle={{ padding: gutter, gap: space[4] }}>
          <Text variant="body">{CONSENT[connector.kind]}</Text>
          <Text variant="footnote" color="textMuted">
            {PICKER_NOTE[connector.kind]}
          </Text>
          {available.isPending ? (
            <ActivityIndicator />
          ) : available.isError ? (
            <ErrorState message={errorText(isApiError(available.error) ? available.error : undefined, connector.kind)} onRetry={() => void available.refetch()} />
          ) : available.data.sources.length === 0 ? (
            <Text variant="body" color="textMuted">
              {connector.kind === "slack" ? "The PDM bot isn't in any channel yet. Invite @PDM to one, then come back." : "No folders were found in this Drive."}
            </Text>
          ) : (
            <View style={[card, { overflow: "hidden" }]} accessibilityRole={single ? "radiogroup" : "list"}>
              {available.data.sources.map((s, i) => {
                const on = chosen.includes(s.id);
                const name = sourceName(connector.kind, s.name);
                return (
                  <Pressable
                    key={s.id}
                    testID={`source-${s.id}`}
                    accessibilityRole={single ? "radio" : "checkbox"}
                    accessibilityLabel={name}
                    accessibilityState={{ checked: on, disabled: !on && !single && chosen.length >= max }}
                    onPress={() => toggle(s.id)}
                    style={({ pressed }) => ({
                      flexDirection: "row",
                      alignItems: "center",
                      minHeight: minTouch,
                      paddingVertical: space[3],
                      paddingHorizontal: space[4],
                      gap: space[3],
                      borderTopWidth: i === 0 ? 0 : 1,
                      borderTopColor: colors.border,
                      backgroundColor: pressed ? colors.surfaceMuted : "transparent",
                    })}
                  >
                    <Text variant="body" style={{ flex: 1 }}>
                      {name}
                    </Text>
                    {on ? <LineIcon name="check" color={colors.text} size={18} /> : null}
                  </Pressable>
                );
              })}
            </View>
          )}
          {save.isError ? <Banner tone="danger" title="Not saved" message={errorText(isApiError(save.error) ? save.error : undefined, connector.kind)} /> : null}
        </ScrollView>
        <View style={{ paddingHorizontal: gutter, paddingTop: space[2], paddingBottom: Math.max(insets.bottom, space[4]) }}>
          <Button label="Save" size="large" busy={save.isPending} disabled={chosen.length === 0 || !available.data} onPress={onSave} />
        </View>
      </View>
    </Modal>
  );
}
