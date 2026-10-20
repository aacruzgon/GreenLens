// The ui layer's public surface (docs/frontend/README.md 14). Features import from "@/ui" only.
export * from "./tokens";
export { ThemeProvider, useTheme, useReducedMotion, useSystemReducedMotion, type Theme, type ThemeProviderProps } from "./theme";
export { DISPLAY_FAMILY, useDisplayFontLoaded, displayFamily } from "./fonts";

export { Text, type TextProps, type TextVariant } from "./primitives/Text";
export { Button, type ButtonProps, type ButtonVariant, type ButtonSize, type IconRenderProps } from "./primitives/Button";
export { IconButton, type IconButtonProps } from "./primitives/IconButton";
export { Card, useCardStyle, type CardProps } from "./primitives/Card";
export { Chip, type ChipProps } from "./primitives/Chip";
export { Banner, type BannerProps, type BannerTone } from "./primitives/Banner";
export { Screen, Wash, WASH_HEIGHT, type ScreenProps } from "./primitives/Screen";
export { Section, type SectionProps } from "./primitives/Section";
export { Sheet, type SheetProps } from "./primitives/Sheet";
export { TextField, type TextFieldProps } from "./primitives/TextField";
export { ListItem, type ListItemProps } from "./primitives/ListItem";
export { Skeleton, type SkeletonProps } from "./primitives/Skeleton";
export { EmptyState, type EmptyStateProps } from "./primitives/EmptyState";
export { ErrorState, type ErrorStateProps } from "./primitives/ErrorState";
export { Enter, type EnterProps } from "./primitives/Enter";
export { Fade, type FadeProps } from "./primitives/Fade";

export { StatusShape, STATUS_SHAPE, type StatusShapeProps } from "./composites/StatusShape";
export { StatusChip, type StatusChipProps } from "./composites/StatusChip";
export { MeltPhoto, type MeltPhotoProps, type MeltKind } from "./composites/MeltPhoto";
export { ConfidenceBadge, type ConfidenceBadgeProps } from "./composites/ConfidenceBadge";
