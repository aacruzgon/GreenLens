// The pictures for picture answers, by `Question.indicator_id` (contracts 3): a small drawing of what Yes and No
// look like, as the reference crops of design 5 (after Merlin). Drawn, not photographed, so they carry no
// licence and read at 52 pt. An indicator without art here gets text answers (PictureAnswer without a picture).
// The drawings are the mockup's (docs/design/mockups/index.html, the question card's .qpic).
import type { ReactNode } from "react";
import Svg, { Circle, Rect } from "react-native-svg";

import { hiddenSvg } from "@/ui";

const LEAF = "#4e8a3c";
const SPOT = "#6e4c29";
const PLAIN_SPOT = "#5a4026";
const RING = "#2c1c0e";

function Frame({ children }: { children: ReactNode }) {
  return (
    <Svg width="100%" height="100%" viewBox="0 0 64 48" preserveAspectRatio="xMidYMid slice" {...hiddenSvg}>
      <Rect width={64} height={48} fill={LEAF} />
      {children}
    </Svg>
  );
}

/** A spot with dark rings inside it, like a target. */
function Rings() {
  return (
    <Frame>
      <Circle cx={32} cy={24} r={17} fill={SPOT} />
      <Circle cx={32} cy={24} r={11.5} fill="none" stroke={RING} strokeWidth={1.6} />
      <Circle cx={32} cy={24} r={6} fill="none" stroke={RING} strokeWidth={1.4} />
    </Frame>
  );
}

/** Many small plain spots, without rings. */
function PlainSpots() {
  return (
    <Frame>
      <Circle cx={24} cy={20} r={6} fill={PLAIN_SPOT} />
      <Circle cx={40} cy={28} r={5} fill={PLAIN_SPOT} />
      <Circle cx={30} cy={34} r={4} fill={PLAIN_SPOT} />
      <Circle cx={44} cy={16} r={3.5} fill={PLAIN_SPOT} />
    </Frame>
  );
}

interface IndicatorPictures {
  yes: { label: string; picture: () => ReactNode };
  no: { label: string; picture: () => ReactNode };
}

/** Indicator slugs from the knowledge base (ml/knowledge), with the answers' labels the pictures stand for. */
const ART: Record<string, IndicatorPictures> = {
  concentric_rings: {
    yes: { label: "Yes, rings", picture: () => <Rings /> },
    no: { label: "No, plain spots", picture: () => <PlainSpots /> },
  },
};

export function indicatorArt(indicatorId: string | undefined): IndicatorPictures | undefined {
  return indicatorId ? ART[indicatorId] : undefined;
}
