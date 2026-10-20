// A plant's status as a word, a colour and a shape (design.md 6; styles.css .status, .s-*).
import type { StyleProp, ViewStyle } from "react-native";

import { Chip } from "../primitives/Chip";
import { statusRoles, statusWords, type PlantStatus } from "../tokens";
import { StatusShape } from "./StatusShape";

export interface StatusChipProps {
  status: PlantStatus;
  onPress?: () => void;
  style?: StyleProp<ViewStyle>;
}

export function StatusChip({ status, onPress, style }: StatusChipProps) {
  const roles = statusRoles[status];
  const word = statusWords[status];
  return (
    <Chip
      label={word}
      leading={<StatusShape status={status} />}
      color={roles.fg}
      fill={roles.bg}
      onPress={onPress}
      accessibilityLabel={`Status: ${word}`}
      style={style}
    />
  );
}
