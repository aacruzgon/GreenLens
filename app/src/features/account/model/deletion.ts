import type { AccountDeletion } from "@/domain/types";

// Account deletion as Settings explains it (flows 13; architecture 5, deletion; contracts 9, account).

/** What happens, said before the user confirms (HIG Managing accounts, deleting accounts). */
export const DELETION_POINTS = [
  "Gardens only you are in are deleted, with their plants, scans and photos.",
  "A garden you alone own but share passes to its longest-standing editor. With no editor, it is deleted.",
  "In shared gardens your private chats go. Your scans and shared notes stay, without your name.",
  "Your sign-in is deleted too. Signing in again starts a new account.",
] as const;

/** The word typed to confirm (flows 13). */
export const CONFIRM_WORD = "delete";

export function confirmed(typed: string): boolean {
  return typed.trim().toLowerCase() === CONFIRM_WORD;
}

/**
 * The summary after DELETE /me (flows 13): "2 gardens deleted. Allotment is now owned by Sam Lee." Names come from
 * what the cache held before the call, since the gardens are gone once it returns.
 */
export function deletionSummary(
  result: AccountDeletion,
  gardenNames: ReadonlyMap<string, string>,
  memberNames: ReadonlyMap<string, string>,
): string {
  const parts: string[] = [];
  const n = result.deleted_gardens.length;
  if (n === 1) parts.push(`${gardenNames.get(result.deleted_gardens[0]!) ?? "1 garden"} is deleted.`);
  else if (n > 1) parts.push(`${n} gardens deleted.`);
  for (const t of result.transferred_gardens) {
    const garden = gardenNames.get(t.garden_id) ?? "A shared garden";
    const owner = memberNames.get(t.new_owner_user_id);
    parts.push(owner ? `${garden} is now owned by ${owner}.` : `${garden} has a new owner.`);
  }
  parts.push("The rest finishes in the background.");
  return parts.join(" ");
}
