// The farm records an answer checked (EON E10.3; contracts 8, `farm_citations`): a vendor's record opens in Slack
// or Drive; an `era` record opens its text in a sheet, labelled as Era's synthetic data (EON decision 10).
import { useState } from "react";
import { Pressable, View } from "react-native";

import { useConnectors } from "@/api";
import type { FarmCitation } from "@/domain/types";
import { links } from "@/platform";
import { LineIcon, Text, minTouch, space, useTheme } from "@/ui";

import { isVendorRecord, openFarmCitation, SYSTEM_WORD } from "../model/farm";
import { FarmRecordSheet } from "./FarmRecordSheet";

export interface FarmCitationsProps {
  citations: readonly FarmCitation[];
  gardenId: string;
  timeZone: string;
}

export function FarmCitations({ citations, gardenId, timeZone }: FarmCitationsProps) {
  const { colors } = useTheme();
  const connectors = useConnectors(gardenId);
  const list = connectors.data?.connectors;
  const [open, setOpen] = useState<FarmCitation>();
  if (citations.length === 0) return null;
  return (
    <View testID="farm-citations">
      <Text variant="caption1" color="textMuted" emphasis style={{ textTransform: "uppercase", letterSpacing: 0.6 }}>
        Records checked
      </Text>
      {citations.map((c) => {
        // Shown as demo data once the connectors are known and this one isn't a vendor's.
        const era = list ? !isVendorRecord(c, list) : false;
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
      {open ? <FarmRecordSheet citation={open} vendor={isVendorRecord(open, list)} timeZone={timeZone} onClose={() => setOpen(undefined)} /> : null}
    </View>
  );
}
