import { useEffect, useRef } from "react";

import { flattenPages, useGardens, useMe, useUpdateMe } from "@/api";
import { useSession } from "@/features/auth";

import { pickActiveGarden } from "../model/activeGarden";
import { useActiveGardenStore } from "./activeGardenStore";

export type GateState =
  | { kind: "loading" }
  | { kind: "error"; error: unknown; retry: () => void }
  | { kind: "onboarding" }
  | { kind: "ready"; gardenId: string };

// SessionGate (docs/frontend/README.md §7–8): GET /me and GET /gardens, then the active garden or onboarding.
export function useSessionGate(): GateState {
  const session = useSession();
  const me = useMe();
  const gardens = useGardens();
  const updateMe = useUpdateMe();
  const remembered = useActiveGardenStore((s) => s.gardenId);
  const setGardenId = useActiveGardenStore((s) => s.setGardenId);
  const namedOnce = useRef(false);

  // First sign-in: send Clerk's name when the account has none; onboarding asks otherwise.
  useEffect(() => {
    if (namedOnce.current || !me.data || me.data.display_name || !session.profileName) return;
    namedOnce.current = true;
    updateMe.mutate({ display_name: session.profileName }, { onError: () => {} });
  }, [me.data, session.profileName, updateMe]);

  const list = gardens.data ? flattenPages(gardens.data) : null;
  const active = list ? pickActiveGarden(list, remembered) : null;

  useEffect(() => {
    if (active && active !== remembered) setGardenId(active);
  }, [active, remembered, setGardenId]);

  if (me.error || gardens.error) {
    return {
      kind: "error",
      error: me.error ?? gardens.error,
      retry: () => {
        void me.refetch();
        void gardens.refetch();
      },
    };
  }
  if (!me.data || !list) return { kind: "loading" };
  if (!active) return { kind: "onboarding" };
  return { kind: "ready", gardenId: active };
}
