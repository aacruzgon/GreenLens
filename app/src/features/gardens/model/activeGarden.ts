// Which garden the app shows: the stored choice when it is still one of the user's gardens,
// otherwise the first (docs/frontend 12: checked against GET /gardens on start).
export function pickActiveGarden<T extends { id: string }>(gardens: T[], stored: string | null): T | undefined {
  return gardens.find((g) => g.id === stored) ?? gardens[0];
}
