import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useEffect, useReducer, useRef, useState } from "react";

import { flattenPages, isApiError, newKey, qk, scans, useCreateTask, useGarden, useScan, useTasks, type IdempotencyKey } from "@/api";
import { files, haptics, type CapturedShot } from "@/platform";

import { uploadOutcome } from "../model/draft";
import {
  JAR_TASK_TITLE,
  initialOneShot,
  jarTaskDue,
  jarTaskOf,
  oneShotReducer,
  oneShotTarget,
  type OneShotPurpose,
} from "../model/oneShot";
import type { ShotFile } from "../model/slot";

function messageOf(error: unknown): string {
  return isApiError(error) ? error.message : "Something went wrong. Try again.";
}

// The view-model hook of question and guided capture (roadmap F2 item 7; flows 6, photo request, and 8, guided
// checks). One photo, sent on Send; once stored, the scan screen takes over and shows it being checked. Nothing is
// persisted: a single photo is quicker to take again than to resume.
export function useOneShotSession(scanId: string, purpose: OneShotPurpose) {
  const router = useRouter();
  const qc = useQueryClient();
  const scan = useScan(scanId);
  const gardenId = scan.data?.garden_id ?? "";
  const garden = useGarden(gardenId);
  const isJar = purpose.type === "guided" && purpose.kind === "jar_test";
  // The scan's jar-test task, if one was started: opened from that task, the instructions are already done.
  const openTasks = useTasks(isJar ? gardenId : "", { state: "open" });
  const jarTask = isJar ? jarTaskOf(flattenPages(openTasks.data), scanId) : undefined;
  const createTask = useCreateTask();
  const [state, dispatch] = useReducer(oneShotReducer, purpose, (p) => initialOneShot(p));
  const [problem, setProblem] = useState<string>();
  const sending = useRef(false);

  // Opened from the jar task (on any device), the jar is already settling: straight to the camera (flows 8).
  const skipped = useRef(false);
  useEffect(() => {
    if (jarTask && !skipped.current) {
      skipped.current = true;
      dispatch({ type: "start" });
    }
  }, [jarTask]);

  const target = scan.data ? oneShotTarget(scan.data, purpose) : undefined;
  // The task's key is kept while Start is retried, so a lost response doesn't add a second task (contracts 1).
  const taskKey = useRef<IdempotencyKey>(newKey());

  async function start() {
    setProblem(undefined);
    if (isJar && !jarTask && scan.data) {
      try {
        await createTask.mutateAsync(
          {
            garden_id: scan.data.garden_id,
            plant_id: scan.data.plant_id,
            scan_id: scanId,
            title: JAR_TASK_TITLE,
            ...(garden.data ? { due: jarTaskDue(garden.data.time_zone) } : {}),
          },
          taskKey.current,
        );
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
    };
    setProblem(undefined);
    dispatch({ type: "take", file, key: newKey() });
  }

  async function send(file: ShotFile, key: string) {
    if (!target?.ok || sending.current) return;
    sending.current = true;
    try {
      const part = await files.toUploadPart(file.uri);
      const res = await scans.uploadImage(scanId, { kind: target.kind, file: part, questionId: target.questionId }, key as IdempotencyKey);
      dispatch({ type: "stored", imageId: res.image.id });
      void haptics.success();
      void qc.invalidateQueries({ queryKey: qk.scan(scanId) });
      // The scan screen shows the photo being checked (flows 6) or the check analysing (flows 8).
      router.replace({ pathname: "/scans/[scanId]", params: { scanId } });
    } catch (error) {
      const outcome = uploadOutcome(isApiError(error) ? error : undefined, file);
      if (outcome.type === "reencode") {
        try {
          const uri = await files.reencode(file.uri, file);
          const jpeg: ShotFile = { ...file, uri, name: `${file.name.replace(/\.\w+$/, "")}.jpg`, type: "image/jpeg", reencoded: true };
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
        if (isApiError(error) && error.code === "invalid_state") void scan.refetch();
      }
      void haptics.warning();
    } finally {
      sending.current = false;
    }
  }

  return {
    scan,
    target,
    state,
    problem,
    jarTask,
    busy: { starting: createTask.isPending },
    actions: {
      start: () => void start(),
      take,
      retake: () => dispatch({ type: "retake" }),
      send: () => {
        if (state.step !== "review") return;
        dispatch({ type: "send" });
        void send(state.file, state.key);
      },
      close: () => router.back(),
    },
  };
}

export type OneShotSession = ReturnType<typeof useOneShotSession>;
