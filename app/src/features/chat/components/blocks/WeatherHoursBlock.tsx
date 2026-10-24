// Weather hours (design 5): each day's hours over 90% humidity, the leaf-wetness stand-in, as bars; a tap on a day
// shows its hours, rain and temperatures. Weather is a pattern here, never a cause (analytics 2).
import { useState } from "react";
import { Pressable, View } from "react-native";

import type { AnswerBlock } from "@/domain/proposed";
import { formatLocalDate } from "@/domain/format";
import { Text, minTouch, space, useTheme } from "@/ui";

import { blockSummary, dayLabel, humidHoursText, humidShare } from "../../model/blocks";
import { BlockFrame } from "./BlockFrame";

type Block = Extract<AnswerBlock, { kind: "weather_hours" }>;

const BAR = 72;

export function WeatherHoursBlock({ block }: { block: Block }) {
  const { colors } = useTheme();
  const [picked, setPicked] = useState(0);
  const day = block.days[picked];
  return (
    <BlockFrame title={block.scan_id ? "Weather before the scan" : "Humid hours"} icon="rain" summary={blockSummary(block)} testID="block-weather_hours">
      <View style={{ flexDirection: "row", justifyContent: "space-between" }} accessibilityRole="tablist">
        {block.days.map((d, i) => (
          <Pressable
            key={d.date}
            accessibilityRole="tab"
            accessibilityState={{ selected: i === picked }}
            accessibilityLabel={`${formatLocalDate(d.date)}, ${humidHoursText(d)}`}
            onPress={() => setPicked(i)}
            style={{ alignItems: "center", gap: space[1], minWidth: minTouch }}
          >
            <View style={{ height: BAR, width: 14, justifyContent: "flex-end", borderRadius: 7, backgroundColor: colors.surfaceMuted, overflow: "hidden" }}>
              <View style={{ height: Math.max(2, humidShare(d) * BAR), backgroundColor: i === picked ? colors.info : colors.infoSoft }} />
            </View>
            <Text variant="caption1" color={i === picked ? "text" : "textMuted"} emphasis={i === picked}>
              {dayLabel(d.date)}
            </Text>
          </Pressable>
        ))}
      </View>
      {day ? (
        <Text variant="subheadline" accessibilityLiveRegion="polite" testID="weather-day">
          {formatLocalDate(day.date)}: {humidHoursText(day)}, {day.precip_mm} mm of rain, {day.t_min_c} to {day.t_max_c} °C.
        </Text>
      ) : null}
    </BlockFrame>
  );
}
