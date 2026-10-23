// The plan's mini player (roadmap 10 item 6; design 3.1, after Apple Music): "Up next · step 1 of 5" with the
// step's title, and play for step by step. R1: NativeTabs on SDK 57 has a BottomAccessory, but the diagnosis is a
// stack screen pushed over the tabs, so there is no tab bar under it to carry one; this is our own solid bar
// (not glass: glass is only for the system's chrome). Ask opens a chat about this result (F6; flows 9).
import type { ImageSource } from "expo-image";
import { Image } from "expo-image";
import { Pressable, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { LineIcon, Text, minTouch, space, useCardStyle, useTheme } from "@/ui";

export interface MiniPlayerProps {
  /** 1-based. */
  stepNumber: number;
  stepCount: number;
  title: string;
  photo?: ImageSource | number | null;
  onPlay: () => void;
  /** "Ask about this result": a conversation anchored to the scan. */
  onAsk?: () => void;
  asking?: boolean;
}

export const MINI_PLAYER_HEIGHT = 62;

export function MiniPlayer({ stepNumber, stepCount, title, photo, onPlay, onAsk, asking = false }: MiniPlayerProps) {
  const { colors } = useTheme();
  const card = useCardStyle();
  const insets = useSafeAreaInsets();
  return (
    <View
      pointerEvents="box-none"
      style={{ position: "absolute", left: 14, right: 14, bottom: Math.max(insets.bottom, space[4]) + space[2] }}
    >
      <View
        accessibilityRole="toolbar"
        accessibilityLabel="Up next"
        testID="mini-player"
        style={[card, { height: MINI_PLAYER_HEIGHT, borderRadius: MINI_PLAYER_HEIGHT / 2, flexDirection: "row", alignItems: "center", gap: 10, paddingLeft: 8, paddingRight: 6 }]}
      >
        <View style={{ width: 46, height: 46, borderRadius: 23, overflow: "hidden", backgroundColor: colors.wash }}>
          {photo ? <Image source={photo} contentFit="cover" style={{ flex: 1 }} accessible={false} /> : null}
        </View>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`Up next, step ${stepNumber} of ${stepCount}: ${title}`}
          accessibilityHint="Opens the plan step by step"
          onPress={onPlay}
          style={{ flex: 1, justifyContent: "center", minHeight: minTouch }}
        >
          <Text variant="caption1" color="textMuted">
            Up next · step {stepNumber} of {stepCount}
          </Text>
          <Text variant="subheadline" emphasis numberOfLines={1}>
            {title}
          </Text>
        </Pressable>
        {onAsk ? (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Ask about this result"
            accessibilityState={{ busy: asking }}
            disabled={asking}
            onPress={onAsk}
            testID="mini-player-ask"
            style={({ pressed }) => ({ width: minTouch, height: minTouch, alignItems: "center", justifyContent: "center", opacity: pressed || asking ? 0.6 : 1 })}
          >
            <LineIcon name="chat" color={colors.text} size={22} />
          </Pressable>
        ) : null}
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Start step by step"
          onPress={onPlay}
          testID="mini-player-play"
          style={({ pressed }) => ({ width: minTouch, height: minTouch, alignItems: "center", justifyContent: "center", opacity: pressed ? 0.6 : 1 })}
        >
          <LineIcon name="play" color={colors.text} size={22} />
        </Pressable>
      </View>
    </View>
  );
}
