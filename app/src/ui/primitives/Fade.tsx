import { useId } from "react";
import { StyleSheet, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Defs, LinearGradient, Rect, Stop } from "react-native-svg";

export interface FadeStop {
  /** 0 at the top, 1 at the bottom. */
  at: number;
  color: string;
  opacity: number;
}

/** A vertical gradient that fills its parent (or the box `style` gives it): the melt's wash, and photos dissolving into the ground (design 3.1). */
export function Fade({ stops, style }: { stops: FadeStop[]; style?: StyleProp<ViewStyle> }) {
  const id = useId().replace(/:/g, "");
  return (
    <Svg width="100%" height="100%" style={style ?? StyleSheet.absoluteFill} pointerEvents="none" preserveAspectRatio="none" viewBox="0 0 1 1">
      <Defs>
        <LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          {stops.map((s) => (
            <Stop key={s.at} offset={s.at} stopColor={s.color} stopOpacity={s.opacity} />
          ))}
        </LinearGradient>
      </Defs>
      <Rect x="0" y="0" width="1" height="1" fill={`url(#${id})`} />
    </Svg>
  );
}
