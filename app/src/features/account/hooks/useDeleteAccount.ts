import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Alert } from "react-native";

import { flattenPages, isApiError, useDeleteMe, useGardens } from "@/api";
import type { Member } from "@/domain/types";
import { useSession } from "@/features/auth";

import { confirmed, deletionSummary } from "../model/deletion";

// account/delete (flows 13): note the names the summary needs from the cache, DELETE /me, then sign out here.
// The cascade removes the push token, so sign-out makes no push-token call.
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
    try {
      const result = await remove.mutateAsync();
      await session.signOut();
      Alert.alert("Your account is deleted", deletionSummary(result, gardenNames, memberNames));
    } catch (e) {
      setProblem(isApiError(e) ? e.message : "Your account wasn't deleted. Check your connection and try again.");
    }
  };

  return { typed, setTyped, ready: confirmed(typed), busy: remove.isPending, problem, run };
}
