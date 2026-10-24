import { useEffect, useRef, useState } from "react";

import { isApiError, useAcceptInvite } from "@/api";
import type { Garden } from "@/domain/types";

import { acceptProblem } from "../model/members";
import { useActiveGardenStore } from "./activeGardenStore";
import { usePendingInviteStore } from "./pendingInviteStore";

export type AcceptState =
  | { kind: "joining" }
  | { kind: "joined"; garden: Garden }
  | { kind: "failed"; message: string; canRetry: boolean };

// invite/[code] signed in (flows 3): accept once, make the garden active, and say why when it can't.
export function useAcceptInviteScreen(code: string): { state: AcceptState; retry: () => void } {
  const accept = useAcceptInvite();
  const setGardenId = useActiveGardenStore((s) => s.setGardenId);
  const clearPending = usePendingInviteStore((s) => s.setCode);
  const [state, setState] = useState<AcceptState>({ kind: "joining" });
  const started = useRef<string | null>(null);

  const run = () => {
    setState({ kind: "joining" });
    accept.mutateAsync(code).then(
      (garden) => {
        clearPending(null);
        setGardenId(garden.id);
        setState({ kind: "joined", garden });
      },
      (e: unknown) => {
        const status = isApiError(e) ? e.status : undefined;
        // A dead code stays dead; drop it so it isn't opened again at the next sign-in.
        if (status === 404) clearPending(null);
        setState({ kind: "failed", message: acceptProblem(status), canRetry: status !== 404 });
      },
    );
  };

  useEffect(() => {
    if (started.current === code) return;
    started.current = code;
    run();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- one accept per code; `run` changes every render
  }, [code]);

  return { state, retry: run };
}
