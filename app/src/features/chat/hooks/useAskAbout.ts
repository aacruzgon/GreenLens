// "Ask about this result" and Ask on the 3D garden's plant (flows 9; README 11, scan results): opens the newest
// conversation anchored to the scan, plant or garden, or starts one, then the conversation. Any member may chat.
import { useRouter } from "expo-router";
import { useCallback, useRef, useState } from "react";

import { chat, isApiError, useCreateConversation } from "@/api";
import type { Anchor } from "@/domain/types";

export function useAskAbout(gardenId: string) {
  const router = useRouter();
  const create = useCreateConversation();
  const [opening, setOpening] = useState(false);
  const [problem, setProblem] = useState<string>();
  // One open at a time: a quick second tap would start a second conversation.
  const busy = useRef(false);

  const ask = useCallback(
    async (anchor: Anchor) => {
      if (busy.current || !gardenId) return;
      busy.current = true;
      setOpening(true);
      setProblem(undefined);
      try {
        const page = await chat.list(gardenId, { type: anchor.type, id: anchor.id });
        const id = page.items[0]?.id ?? (await create.mutateAsync({ garden_id: gardenId, anchor })).id;
        router.push({ pathname: "/chat/[conversationId]", params: { conversationId: id } });
      } catch (e) {
        setProblem(isApiError(e) ? e.message : "The chat couldn't open. Check your network.");
      } finally {
        busy.current = false;
        setOpening(false);
      }
    },
    [gardenId, create, router],
  );

  return { ask, opening, problem };
}
