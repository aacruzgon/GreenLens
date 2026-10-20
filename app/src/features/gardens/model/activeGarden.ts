import type { Garden, Id } from "@/domain/types";

// The active garden is the one the user chose last, while they still belong to it; otherwise their first garden.
export function pickActiveGarden(gardens: readonly Pick<Garden, "id">[], remembered: Id | null): Id | null {
  if (remembered && gardens.some((g) => g.id === remembered)) return remembered;
  return gardens[0]?.id ?? null;
}
