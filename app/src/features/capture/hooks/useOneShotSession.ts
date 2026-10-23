import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useEffect, useReducer, useRef, useState } from "react";

import { isApiError, newKey, qk, scans, useCreateTask, useGarden, useScan, type IdempotencyKey } from "@/api";
import type { CreateTaskRequest } from "@/domain/types";
import { files, haptics, type CapturedShot } from "@/platform";

import { uploadOutcome } from "../model/draft";
import { initialOneShot, jarTaskRequest, oneShotReducer, oneShotTarget, type OneShotPurpose } from "../model/oneShot";
import type { ShotFile } from "../model/slot";
import { leaveCapture } from "./leaveCapture";
import { shotUpload } from "./shotUpload";
import { useScanJarTask } from "./useScanJarTask";

function messageOf(error: unknown): string {
  return isApiError(error) ? error.message : "Something went wrong. Try again.";
}

/**
 * What question and guided capture need before the first step can be chosen: the scan, and for the jar test the
 * garden (its time zone dates the task) and the scan's jar-test task, looked up to the last page. Until `ready`, the
 * screen shows nothing to tap, so Start can't run before it knows whether the task exists (flows 8).
 */
export function useOneShotData(scanId: string, purpose: OneShotPurpose) {
  const scan = useScan(scanId);
  const isJar = purpose.type === "guided" && purpose.kind === "jar_test";
  const gardenId = isJar ? (scan.data?.garden_id ?? "") : "";
  const garden = useGarden(gardenId);
  const jar = useScanJarTask(gardenId, scanId);
  const target = scan.data ? oneShotTarget(scan.data, purpose) : undefined;
  return {
    scan,
    target,
    isJar,
    timeZone: garden.data?.time_zone,
    jarTask: isJar ? jar.task : undefined,
    ready: Boolean(scan.data) && (!isJar || (jar.settled && Boolean(garden.data))),
    loadError: scan.error ?? (isJar ? (garden.error ?? jar.error) : null),
    retryLoad: () => {
      void scan.refetch();
      if (isJar) {
        void garden.refetch();
        jar.retry();
      }
    },
  };
}

export type OneShotData = ReturnType<typeof useOneShotData>;

// The view-model hook of question and guided capture (roadmap F2 item 7; flows 6, photo request, and 8, guided
// checks), used once the data is ready. One photo, sent on Send; once stored, the scan screen takes over and shows
// it being checked. Nothing is persisted: a single photo is quicker to take again than to resume.
export function useOneShotSession(scanId: string, purpose: OneShotPurpose, data: OneShotData) {
  const router = useRouter();
  const qc = useQueryClient();
  const createTask = useCreateTask();
  // Opened from the jar task (on any device), the jar is already settling: straight to the camera (flows 8).
  const [state, dispatch] = useReducer(oneShotReducer, purpose, (p) => initialOneShot(p, Boolean(data.jarTask)));
  const [problem, setProblem] = useState<string>();
  const sending = useRef(false);
  // The scan screen replaces this one only while it is still showing: ✕ is off during Send, and a send that ends
  // after the screen has gone leaves whatever is on top alone.
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const { target } = data;

  // One Start is one action: its body (with the due date worked out once) and key are kept for every retry, so a
  // retry never sends the same key with another body (contracts 1, 2: idempotency_conflict).
  const startAction = useRef<{ body: CreateTaskRequest; key: IdempotencyKey }>(undefined);

  async function start() {
    setProblem(undefined);
    const s = data.scan.data;
    if (data.isJar && !data.jarTask && s && data.timeZone) {
      startAction.current ??= { body: jarTaskRequest(s, data.timeZone), key: newKey() };
      try {
        await createTask.mutateAsync(startAction.current.body, startAction.current.key);
      } catch (error) {
        setProblem(messageOf(error));
        return;
      }
    }
    dispatch({ type: "start" });
    void haptics.selection();
  }

  function take(shot: CapturedShot, picked: boolean) {
    if (!target?.ok) return;
    const file: ShotFile = {
      uri: shot.file.uri,
      name: shot.file.uri.split("/").pop() ?? "photo.jpg",
      type: shot.file.mimeType,
      picked,
      width: shot.file.width,
      height: shot.file.height,
      ...(shot.depth ? { depth: { uri: shot.depth.depthUri, name: shot.depth.depthUri.split("/").pop() ?? "depth.png", meta: shot.depth.meta } } : {}),
    };
    setProblem(undefined);
    dispatch({ type: "take", file, key: newKey() });
  }

  async function send(file: ShotFile, key: string) {
    if (!target?.ok || sending.current) return;
    sending.current = true;
    try {
      const res = await scans.uploadImage(scanId, await shotUpload(target.kind, file, target.questionId), key as IdempotencyKey);
      dispatch({ type: "stored", imageId: res.image.id });
      void haptics.success();
      void qc.invalidateQueries({ queryKey: qk.scan(scanId) });
      // The scan screen shows the photo being checked (flows 6) or the check analysing (flows 8): back to it when it
      // opened this screen, in its place when a task or a link did.
      if (mounted.current) router.dismissTo({ pathname: "/scans/[scanId]", params: { scanId } });
    } catch (error) {
      const outcome = uploadOutcome(isApiError(error) ? error : undefined, file);
      if (outcome.type === "reencode") {
        try {
          const uri = await files.reencode(file.uri, file);
          const jpeg: ShotFile = { ...file, uri, name: `${file.name.replace(/\.\w+$/, "")}.jpg`, type: "image/jpeg", reencoded: true, depth: undefined };
          // A changed body is a new action with a new key (flows 4, re-encoding).
          const next = newKey();
          dispatch({ type: "reencoded", file: jpeg, key: next });
          sending.current = false;
          await send(jpeg, next);
          return;
        } catch (reencodeError) {
          dispatch({ type: "rejected", message: messageOf(reencodeError) });
        }
      } else if (outcome.type === "retry") {
        dispatch({ type: "failed", message: outcome.message });
      } else {
        // image_quality or another refusal: take it again. A question that closed meanwhile gets 409 and the
        // refetched scan says so.
        dispatch({ type: "rejected", message: outcome.message, hint: outcome.type === "rejected" ? outcome.hint : undefined });
        if (isApiError(error) && error.code === "invalid_state") void data.scan.refetch();
      }
      void haptics.warning();
    } finally {
      sending.current = false;
    }
  }

  return {
    target,
    state,
    problem,
    jarTask: data.jarTask,
    busy: { starting: createTask.isPending, sending: state.step === "sending" },
    actions: {
      start: () => void start(),
      take,
      retake: () => dispatch({ type: "retake" }),
      send: () => {
        if (state.step !== "review") return;
        dispatch({ type: "send" });
        void send(state.file, state.key);
      },
      close: () => {
        if (state.step !== "sending") leaveCapture(router);
      },
    },
  };
}

export type OneShotSession = ReturnType<typeof useOneShotSession>;
