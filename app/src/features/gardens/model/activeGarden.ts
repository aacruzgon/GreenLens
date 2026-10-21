import type { Garden, Id } from "@/domain/types";

// The active garden is the one the user chose last, while they still belong to it; otherwise their first garden.
export function pickActiveGarden(gardens: readonly Pick<Garden, "id">[], remembered: Id | null): Id | null {
  if (remembered && gardens.some((g) => g.id === remembered)) return remembered;
  return gardens[0]?.id ?? null;
}

/**
 * Whether the session gate shows its error page. A failed background refetch keeps the app on the data it
 * already has; the page is for a first load that failed, when /me or /gardens has nothing to show.
 */
export function gateShowsError(
  me: { error: unknown; data: unknown },
  gardens: { error: unknown; data: unknown },
): boolean {
  return Boolean(me.error || gardens.error) && (!me.data || !gardens.data);
}
