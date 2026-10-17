import { useEffect } from "react";

import { me as meApi, useGardens, useMe } from "@/api";
import { useSession } from "@/features/auth";

export type Gate = { state: "loading" } | { state: "error"; error: unknown; retry: () => void } | { state: "onboarding" } | { state: "ready" };

/** SessionGate (docs/frontend 7): GET /me and GET /gardens decide onboarding or the app. */
export function useSessionGate(): Gate {
  const session = useSession();
  const me = useMe();
  const gardens = useGardens();

  // First sign-in: send Clerk's name when the API has none (docs/frontend 8, session lifecycle).
  const needsName = me.data && !me.data.display_name && session.suggestedName;
  useEffect(() => {
    // A failure here is harmless: the name stays empty, and onboarding or settings can set it.
    if (needsName) {
      meApi
        .update({ display_name: session.suggestedName! })
        .then(() => me.refetch())
        .catch(() => undefined);
    }
  }, [needsName]); // eslint-disable-line react-hooks/exhaustive-deps

  if (me.isLoading || gardens.isLoading) return { state: "loading" };
  if (me.error || gardens.error) {
    return { state: "error", error: me.error ?? gardens.error, retry: () => void Promise.all([me.refetch(), gardens.refetch()]) };
  }
  if ((gardens.data?.items.length ?? 0) === 0) return { state: "onboarding" };
  return { state: "ready" };
}
