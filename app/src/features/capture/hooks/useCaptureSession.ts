import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useEffect, useRef, useState } from "react";

import { isApiError, newKey, qk, scans, useDeleteScan, usePlant, useScan, useSubmitScan, type IdempotencyKey } from "@/api";
import { backoffMs } from "@/lib/backoff";
import { files, haptics, type CapturedShot } from "@/platform";

import {
  applySlot,
  canAnalyse,
  coverage,
  createDraft,
  hasStored,
  modeMark,
  pendingWork,
  reconcileDraft,
  shutterTarget,
  slotsOf,
  takeShot,
  uploadOutcome,
  type CaptureDraft,
} from "../model/draft";
import { MODES, NEEDED, modeOf, type CaptureKind } from "../model/shotPlan";
import type { ShotFile, Slot, SlotEvent } from "../model/slot";
import { useDraftStore } from "./draftStore";

/** Retries of one upload with the same key before the slot shows "failed" and a Retry (flows 14). */
const AUTO_RETRIES = 2;

const EXTENSION: Record<CapturedShot["file"]["mimeType"], string> = {
  "image/jpeg": "jpg",
  "image/png": "png",
  "image/webp": "webp",
  "image/heic": "heic",
};

function messageOf(error: unknown): string {
  return isApiError(error) ? error.message : "Something went wrong. Try again.";
}

export type SessionProblem = { message: string; newScan?: boolean } | undefined;

// The capture screen's view-model hook (README 5; flows 4, 14, 15): the scan, its draft, the upload runner and
// every action. The runner sends what the draft says is pending, so a draft restored after the app closed picks
// up where it stopped. Components below take its plain data.
export function useCaptureSession(scanId: string) {
  const router = useRouter();
  const qc = useQueryClient();
  const scan = useScan(scanId);
  const plant = usePlant(scan.data?.plant_id ?? "");
  const draft = useDraftStore((s) => s.drafts[scanId]);
  const submit = useSubmitScan();
  const deleteScan = useDeleteScan();
  const [problem, setProblem] = useState<SessionProblem>();
  const inFlight = useRef(new Set<string>());
  const attempts = useRef(new Map<string, number>());
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  // A pending scan with no draft gets one, seeded from the shots already on the server (flows 16). A saved draft
  // is checked against the scan once per visit, on data fetched after capture opened, since the app can't see a
  // shot removed elsewhere or a scan that's gone. The images it held as stored when capture opened are the ones
  // the server must still have; shots stored during the visit are newer than any fetch. A scan that's gone or no
  // longer pending takes its draft with it.
  const s = scan.data;
  const [known] = useState<ReadonlySet<string>>(
    () => new Set((useDraftStore.getState().drafts[scanId]?.slots ?? []).flatMap((x) => (x.state === "stored" && x.imageId ? [x.imageId] : []))),
  );
  const checked = useRef(false);
  useEffect(() => {
    // A cached scan may predate the draft's last change: fetch it again before checking.
    if (qc.getQueryData(qk.scan(scanId))) void scan.refetch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scanId]);
  useEffect(() => {
    const store = useDraftStore.getState();
    if (isApiError(scan.error) && scan.error.status === 404) {
      if (store.drafts[scanId]) store.drop(scanId);
      return;
    }
    if (!s) return;
    if (s.status !== "pending") {
      if (store.drafts[s.id]) store.drop(s.id);
      return;
    }
    if (!store.drafts[s.id]) {
      store.put(createDraft(s));
      checked.current = scan.isFetchedAfterMount;
      return;
    }
    if (checked.current || !scan.isFetchedAfterMount) return;
    checked.current = true;
    let unused: string[] = [];
    store.update(s.id, (d) => {
      const result = reconcileDraft(d, s, known);
      unused = result.unused;
      return result.draft;
    });
    for (const uri of unused) files.remove(uri);
  }, [s, scan.error, scan.isFetchedAfterMount, scanId, known]);

  const current = () => useDraftStore.getState().drafts[scanId];
  const dispatch = (slotId: string, event: SlotEvent) =>
    useDraftStore.getState().update(scanId, (d) => applySlot(d, slotId, event));
  const slotById = (slotId: string) => current()?.slots.find((x) => x.id === slotId);

  async function upload(slotId: string) {
    const slot = slotById(slotId);
    if (!slot?.file || !slot.key || inFlight.current.has(slotId)) return;
    inFlight.current.add(slotId);
    dispatch(slotId, { type: "upload" });
    const { file, key } = slot;
    let reencodeNext = false;
    try {
      const part = await files.toUploadPart(file.uri);
      const res = await scans.uploadImage(scanId, { kind: slot.kind, file: part }, key as IdempotencyKey);
      attempts.current.delete(slotId);
      dispatch(slotId, { type: "stored", imageId: res.image.id, noDepth: res.depth?.stored === false });
      void haptics.success();
      void qc.invalidateQueries({ queryKey: qk.scan(scanId) });
    } catch (error) {
      const outcome = uploadOutcome(isApiError(error) ? error : undefined, file);
      if (outcome.type === "rejected") {
        dispatch(slotId, { type: "rejected", message: outcome.message, hint: outcome.hint });
        void haptics.warning();
      } else if (outcome.type === "retry") {
        dispatch(slotId, { type: "failed", message: outcome.message });
        const n = attempts.current.get(slotId) ?? 0;
        if (n < AUTO_RETRIES) {
          attempts.current.set(slotId, n + 1);
          setTimeout(() => alive.current && void retry(slotId), Math.max(outcome.afterMs, backoffMs(n)));
        }
      } else if (outcome.type === "reencode") {
        reencodeNext = true;
      } else {
        dispatch(slotId, { type: "refused", message: outcome.message });
        files.remove(file.uri);
        void haptics.warning();
      }
    } finally {
      inFlight.current.delete(slotId);
    }
    // After the in-flight mark is gone, so the runner sends the re-encoded file when the draft changes.
    if (reencodeNext) await reencode(slot, file);
  }

  async function reencode(slot: Slot, file: ShotFile) {
    try {
      const name = `${slot.id.replace(":", "-")}-${Date.now()}.jpg`;
      const uri = files.keep(scanId, await files.reencode(file.uri, file), name);
      files.remove(file.uri);
      const jpeg: ShotFile = { ...file, uri, name, type: "image/jpeg", reencoded: true };
      // A changed body is a new action: a new key (flows 4, re-encoding), with its own retries.
      attempts.current.delete(slot.id);
      dispatch(slot.id, { type: "reencoded", file: jpeg, key: newKey() });
    } catch (error) {
      dispatch(slot.id, { type: "refused", message: messageOf(error) });
    }
  }

  async function remove(slotId: string) {
    const slot = slotById(slotId);
    if (slot?.state !== "deleting" || !slot.imageId || inFlight.current.has(slotId)) return;
    inFlight.current.add(slotId);
    const old = slot.file?.uri;
    const tries = `delete:${slotId}`;
    try {
      await scans.deleteImage(scanId, slot.imageId);
    } catch (error) {
      // Already gone (a retry after a lost response, or after the app closed) is what we wanted.
      if (!(isApiError(error) && error.status === 404)) {
        inFlight.current.delete(slotId);
        const n = attempts.current.get(tries) ?? 0;
        if (n < AUTO_RETRIES && (!isApiError(error) || error.retryable || error.status >= 500)) {
          attempts.current.set(tries, n + 1);
          setTimeout(() => alive.current && void remove(slotId), backoffMs(n));
          return;
        }
        // The shot stays stored, with the message and Try again; a retake waiting on the delete is dropped.
        attempts.current.delete(tries);
        dispatch(slotId, { type: "deleteFailed", message: messageOf(error) });
        if (slot.next) files.remove(slot.next.file.uri);
        void haptics.warning();
        return;
      }
    }
    attempts.current.delete(tries);
    inFlight.current.delete(slotId);
    dispatch(slotId, { type: "deleted" });
    if (old) files.remove(old);
    void qc.invalidateQueries({ queryKey: qk.scan(scanId) });
  }

  async function retry(slotId: string) {
    const slot = slotById(slotId);
    if (slot?.state === "failed") await upload(slotId);
    // A delete that didn't go through: send it again.
    else if (slot?.state === "stored" && slot.problem) dispatch(slotId, { type: "replace" });
  }

  // The runner: whatever the draft says is pending goes out, once (in-flight slots are skipped). It follows the
  // store, so it sees every change, the restored draft's included, and reads the latest draft when it sends.
  useEffect(() => {
    const run = (d: CaptureDraft | undefined) => {
      if (!d) return;
      const work = pendingWork(d);
      for (const slot of work.uploads) void upload(slot.id);
      for (const slot of work.deletes) void remove(slot.id);
    };
    run(current());
    return useDraftStore.subscribe((state, prev) => {
      if (state.drafts[scanId] !== prev.drafts[scanId]) run(state.drafts[scanId]);
    });
    // upload and remove read the store, not a render's values; the runner is set up once per scan.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scanId]);

  /** Copies a shot into the draft's folder and puts it in the chosen mode's slot. */
  function addShot(shot: CapturedShot, picked: boolean) {
    const d = current();
    if (!d) return;
    const target = shutterTarget(d, d.mode);
    if (!target) return;
    const name = `${target.slotId.replace(":", "-")}-${Date.now()}.${EXTENSION[shot.file.mimeType]}`;
    const uri = files.keep(scanId, shot.file.uri, name);
    const file: ShotFile = { uri, name, type: shot.file.mimeType, picked, width: shot.file.width, height: shot.file.height };
    // A retake of a rejected or failed shot replaces its photo; a stored one is deleted first and keeps it till then.
    const old = target.replace ? undefined : d.slots.find((x) => x.id === target.slotId)?.file?.uri;
    setProblem(undefined);
    useDraftStore.getState().update(scanId, (x) => takeShot(x, x.mode, file, newKey()));
    // A new shot is a new action, with its own retries.
    attempts.current.delete(target.slotId);
    if (old) files.remove(old);
  }

  function selectMode(kind: CaptureKind) {
    useDraftStore.getState().update(scanId, (d) => ({ ...d, mode: kind }));
    void haptics.selection();
  }

  async function analyse() {
    const d = current();
    if (!d || !canAnalyse(d)) return;
    setProblem(undefined);
    try {
      await submit.mutateAsync(scanId);
      useDraftStore.getState().drop(scanId);
      router.replace({ pathname: "/scans/[scanId]", params: { scanId } });
    } catch (error) {
      // missing_required_shot, a failed scan that needs a new one (hint new_scan), or the network: back to capturing.
      const newScan = isApiError(error) && error.code === "invalid_state" && error.details?.hint === "new_scan";
      setProblem({ message: messageOf(error), newScan });
    }
  }

  async function discard() {
    const pending = s;
    if (!pending) return;
    setProblem(undefined);
    try {
      await deleteScan.mutateAsync(pending);
      useDraftStore.getState().drop(scanId);
      router.back();
    } catch (error) {
      if (isApiError(error) && error.code === "invalid_state") {
        // Submitted meanwhile, from another device: open it instead (flows 4, discard).
        useDraftStore.getState().drop(scanId);
        router.replace({ pathname: "/scans/[scanId]", params: { scanId } });
        return;
      }
      setProblem({ message: messageOf(error) });
    }
  }

  return {
    scan,
    view: draft ? sessionView(draft, plant.data?.name) : undefined,
    problem,
    busy: { submitting: submit.isPending, discarding: deleteScan.isPending },
    actions: {
      selectMode,
      addShot,
      retry: (slotId: string) => void retry(slotId),
      /** Removes a stored shot; the slot empties for the shutter. */
      removeShot: (slotId: string) => dispatch(slotId, { type: "replace" }),
      analyse: () => void analyse(),
      discard: () => void discard(),
      /** Leaves capture and keeps the draft for later (flows 15, suspended). */
      close: () => router.back(),
      dismissProblem: () => setProblem(undefined),
    },
  };
}

/** What the screen draws from the draft: plain data, no functions. */
export function sessionView(draft: CaptureDraft, plantName: string | undefined) {
  const mode = modeOf(draft.mode);
  const dial = coverage(draft);
  const shots = slotsOf(draft, draft.mode);
  const problemSlot = shots.find(
    (x) => x.problem && (x.state === "rejected" || x.state === "failed" || x.state === "empty" || x.state === "stored"),
  );
  const target = shutterTarget(draft, draft.mode);
  // A missing needed shot comes before the optional ones, so the pill and the dial ask for the same thing.
  const missingNeeded = NEEDED.find((kind) => !hasStored(draft, kind));
  const nextOptional = MODES.find((m) => !NEEDED.includes(m.kind) && !hasStored(draft, m.kind));
  return {
    title: plantName ? `Scan · ${plantName}` : "Scan",
    mode,
    modes: MODES.map((m) => ({ kind: m.kind, label: m.label, mark: modeMark(draft, m.kind), selected: m.kind === draft.mode })),
    dial,
    neededText: `${dial.filter((d) => d.stored).length} of ${dial.length} needed shots`,
    // Until the whole plant is stored, say what Analyse waits for (HIG Feedback: show why a command can't run).
    suggestion: !hasStored(draft, "whole_plant")
      ? "Start with the whole plant"
      : missingNeeded
        ? `Add a ${modeOf(missingNeeded).title.toLowerCase()}`
        : nextOptional?.instruction,
    /** The chosen mode's shots, for the drop card's thumbnails. */
    shots: shots
      .filter((x) => x.state !== "empty" || x.file)
      .map((x) => ({ id: x.id, uri: x.file?.uri, state: x.state, noDepth: x.noDepth })),
    problem: problemSlot?.problem
      ? { slotId: problemSlot.id, retryable: problemSlot.state === "failed" || problemSlot.state === "stored", ...problemSlot.problem }
      : undefined,
    shutter: target ? (target.replace ? ("retake" as const) : ("take" as const)) : ("busy" as const),
    canAnalyse: canAnalyse(draft),
  };
}

export type CaptureSessionView = ReturnType<typeof sessionView>;
