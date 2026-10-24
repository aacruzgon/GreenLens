import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Alert } from "react-native";

import { flattenPages, isApiError, useDeleteMe, useGardens } from "@/api";
import type { Member } from "@/domain/types";
import { useSession } from "@/features/auth";
import { clearSignedOutState } from "@/features/shell";

import { confirmed, deletionSummary } from "../model/deletion";

// account/delete (flows 13): note the names the summary needs from the cache, DELETE /me, then sign out here.
// The cascade removes the push token, so sign-out makes no push-token call. Once DELETE /me answered, the account
// is deleted whatever sign-out does: if Clerk's signOut throws, this phone's state is cleared anyway, and the
// session, whose tokens the API refuses from the request on (contracts 9, DELETE /me), ends at Clerk's next refresh.
export function useDeleteAccount() {
  const qc = useQueryClient();
  const session = useSession();
  const gardens = useGardens();
  const remove = useDeleteMe();
  const [typed, setTyped] = useState("");
  const [problem, setProblem] = useState<string>();

  const run = async () => {
    if (!confirmed(typed)) return;
    setProblem(undefined);
    const gardenNames = new Map(flattenPages(gardens.data).map((g) => [g.id, g.name]));
    const memberNames = new Map<string, string>();
    // Every garden's member list in the cache: `qk.members(gardenId)` starts with "members".
    for (const [, list] of qc.getQueriesData<Member[]>({ queryKey: ["members"] })) {
      for (const m of list ?? []) memberNames.set(m.user_id, m.display_name);
    }
    let result;
    try {
      result = await remove.mutateAsync();
    } catch (e) {
      setProblem(isApiError(e) ? e.message : "Your account wasn't deleted. Check your connection and try again.");
      return;
    }
    try {
      await session.signOut();
    } catch {
      clearSignedOutState();
    }
    Alert.alert("Your account is deleted", deletionSummary(result, gardenNames, memberNames));
  };

  return { typed, setTyped, ready: confirmed(typed), busy: remove.isPending, problem, run };
}
