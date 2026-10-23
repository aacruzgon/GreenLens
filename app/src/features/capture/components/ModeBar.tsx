// The mode bar (roadmap F2 item 3; design 3.1; styles.css .modes): the shots as the Camera app's modes, WHOLE PLANT ·
// CLOSE-UP · UNDERSIDE · SOIL · FRUIT, the chosen one in the Camera app's yellow and kept centred; a dot marks a
// stored shot. It replaces screens 3.2's row of shot chips. Each mode is a tab (HIG segmented controls: one choice
// of a few, the selection always visible).
//
// In the app it is Apple's own control: the module's segmented control on Liquid Glass, which iOS draws (design 6).
// Where the module is absent (Expo Go, the web) it is a solid dark pill drawn here, never an imitation of glass.
import { useEffect, useRef, useState } from "react";
import { Pressable, ScrollView, View, type LayoutRectangle } from "react-native";

import { ModePickerView } from "@/platform";
import { Text, camera, useReducedMotion } from "@/ui";

import type { ModeMark } from "../model/draft";
import type { CaptureKind } from "../model/shotPlan";

export interface ModeBarItem {
  kind: CaptureKind;
  label: string;
  mark: ModeMark;
  selected: boolean;
}

const MARK_WORDS: Record<ModeMark, string | undefined> = {
  none: undefined,
  stored: "taken",
  busy: "uploading",
  problem: "needs a retake",
};

function Mark({ mark }: { mark: ModeMark }) {
  if (mark === "none") return null;
  if (mark === "problem") {
    return (
      <Text variant="caption1" style={{ color: camera.ink, fontWeight: "700" }}>
        !
      </Text>
    );
  }
  // A filled dot once stored (.mode.done::before), a ring while the shot is on its way.
  return (
    <View
      style={{
        width: 6,
        height: 6,
        borderRadius: 3,
        backgroundColor: mark === "stored" ? camera.ink : "transparent",
        borderWidth: mark === "busy" ? 1.5 : 0,
        borderColor: camera.ink,
      }}
    />
  );
}

export function ModeBar({ items, onSelect }: { items: ModeBarItem[]; onSelect: (kind: CaptureKind) => void }) {
  if (ModePickerView) {
    return (
      <ModePickerView
        testID="mode-bar"
        items={items}
        onSelect={(e) => onSelect(e.nativeEvent.kind as CaptureKind)}
        style={{ alignSelf: "stretch", height: 48 }}
      />
    );
  }
  return <DrawnModeBar items={items} onSelect={onSelect} />;
}

function DrawnModeBar({ items, onSelect }: { items: ModeBarItem[]; onSelect: (kind: CaptureKind) => void }) {
  const reduced = useReducedMotion();
  const scroll = useRef<ScrollView>(null);
  const [barWidth, setBarWidth] = useState(0);
  const frames = useRef(new Map<CaptureKind, LayoutRectangle>());
  const chosen = items.find((i) => i.selected)?.kind;

  // Keeps the chosen mode centred, as the Camera app does (app.js centreMode).
  useEffect(() => {
    const f = chosen ? frames.current.get(chosen) : undefined;
    if (!f || !barWidth) return;
    scroll.current?.scrollTo({ x: Math.max(0, f.x - (barWidth - f.width) / 2), animated: !reduced });
  }, [chosen, barWidth, reduced]);

  return (
    <View
      accessibilityRole="tablist"
      accessibilityLabel="Shot"
      onLayout={(e) => setBarWidth(e.nativeEvent.layout.width)}
      style={{
        alignSelf: "center",
        maxWidth: "100%",
        borderRadius: 999,
        overflow: "hidden",
        backgroundColor: camera.pill,
        borderWidth: 1,
        borderColor: camera.hairline,
      }}
    >
      {/* A ScrollView grows to fill its parent by default (flexGrow 1); the bar is one row. */}
      <ScrollView
        ref={scroll}
        horizontal
        showsHorizontalScrollIndicator={false}
        style={{ flexGrow: 0 }}
        contentContainerStyle={{ padding: 4, gap: 2 }}
      >
        {items.map((m) => {
          const state = MARK_WORDS[m.mark];
          return (
            <Pressable
              key={m.kind}
              testID={`mode-${m.kind}`}
              accessibilityRole="tab"
              accessibilityState={{ selected: m.selected }}
              accessibilityLabel={state ? `${m.label}, ${state}` : m.label}
              onPress={() => onSelect(m.kind)}
              onLayout={(e) => frames.current.set(m.kind, e.nativeEvent.layout)}
              style={{
                flexDirection: "row",
                alignItems: "center",
                gap: 6,
                minHeight: 36,
                paddingHorizontal: 14,
                borderRadius: 999,
                backgroundColor: m.selected ? camera.selected : "transparent",
              }}
            >
              <Mark mark={m.mark} />
              <Text
                variant="footnote"
                style={{
                  color: m.selected ? camera.modeOn : camera.inkSoft,
                  fontWeight: "600",
                  letterSpacing: 0.8,
                  textTransform: "uppercase",
                }}
              >
                {m.label}
              </Text>
            </Pressable>
          );
        })}
      </ScrollView>
    </View>
  );
}
