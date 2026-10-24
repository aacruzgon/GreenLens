// The diagnosis and plan's view model (roadmap F4; screens 3.3–3.4; design 3.1, the diagnosis): the scan's result
// in the screen's words, the plan's steps, the role checks and the actions. It holds the scan's stream, so a
// provisional plan's late answer, or a rerun from elsewhere, updates the screen with "Updated after your answer".
import { useRouter } from "expo-router";
import { useMemo, useState } from "react";

import { imageSource, isApiError, useActionKey, useAnswerQuestion, useConnectors, useGarden } from "@/api";
import { planNote } from "@/domain/diagnosis";
import { can } from "@/domain/permissions";
import type { Answer, FarmCitation } from "@/domain/types";
import { useStartCapture } from "@/features/capture";
import { isVendorRecord, openFarmCitation } from "@/features/chat";
import { links } from "@/platform";

import { supersededNote } from "../model/plan";
import {
  evidenceRows,
  featureCards,
  header,
  heroImage,
  measurementRows,
  organismRows,
  shownCandidate,
  soilRows,
  weatherRows,
} from "../model/result";
import { usePhotoTint } from "./usePhotoTint";
import { usePlanSteps } from "./usePlanSteps";
import { useScanEvents } from "./useScanEvents";

export function useDiagnosis(scanId: string) {
  const router = useRouter();
  const events = useScanEvents(scanId);
  const { detail, state } = events;
  const garden = useGarden(detail?.garden_id ?? "");
  const role = garden.data?.my_role;
  const connectorList = useConnectors(detail?.garden_id ?? "").data?.connectors;
  const plan = usePlanSteps(detail);
  const answer = useAnswerQuestion(scanId);
  const startCapture = useStartCapture();
  const keyFor = useActionKey();
  const [sending, setSending] = useState<Answer>();
  const [answerProblem, setAnswerProblem] = useState<string>();
  // A farm record opened in the sheet (an `era` record, or one whose connector isn't known to be a vendor's).
  const [farmSheet, setFarmSheet] = useState<FarmCitation>();

  const photo = detail ? heroImage(detail) : undefined;
  const tint = usePhotoTint(photo);
  const diagnosis = detail?.diagnosis;
  const plantName = detail?.plant.name ?? "Your plant";
  const observations = useMemo(() => detail?.observations.filter((o) => o.run === detail.run) ?? [], [detail]);

  const view = useMemo(() => {
    if (!detail || !diagnosis) return undefined;
    return {
      header: header(plantName, diagnosis, detail.images.length),
      lowEvidence: diagnosis.status === "uncertain" && diagnosis.low_evidence,
      cards: featureCards(diagnosis),
      evidence: evidenceRows(shownCandidate(diagnosis), observations),
      weather: weatherRows(detail.weather),
      soil: soilRows(diagnosis.soil),
      measurements: measurementRows(observations),
      organisms: organismRows(diagnosis, observations),
      citations: diagnosis.citations,
    };
  }, [detail, diagnosis, observations, plantName]);

  const p = plan.plan;
  // The open question of a provisional plan sits at the plan's top (screens 3.4).
  const question = p?.provisional ? (state?.question ?? detail?.open_question) : undefined;
  // The plan's own sources, numbered as its steps cite them; the diagnosis's others follow.
  const sources = useMemo(() => {
    const own = p?.citations ?? [];
    const seen = new Set(own.map((c) => c.source_id));
    return [...own, ...(view?.citations ?? []).filter((c) => !seen.has(c.source_id))];
  }, [p?.citations, view?.citations]);

  const actions = {
    back: () => (router.canGoBack() ? router.back() : router.dismissAll()),
    /** The garden in 3D (or on the map), on this plant (roadmap F7). */
    inGarden: () =>
      detail?.plant_id ? router.push({ pathname: "/garden", params: { plantId: detail.plant_id } }) : router.dismissAll(),
    stepByStep: (start?: number) =>
      router.push({ pathname: "/scans/[scanId]/steps", params: { scanId, ...(start !== undefined ? { start: String(start) } : {}) } }),
    feedback: () => router.push({ pathname: "/scans/[scanId]/feedback", params: { scanId } }),
    openSource: (url: string) => void links.openInBrowser(url),
    /** A vendor's record opens in Slack or Drive; anything else shows its text in a sheet, never its link (EON decision 10). */
    openFarmRecord: (record: FarmCitation) => {
      const open = openFarmCitation(record, connectorList);
      if (open.kind === "external") void links.openExternal(open.url);
      else setFarmSheet(open.citation);
    },
    closeFarmRecord: () => setFarmSheet(undefined),
    /** `low_evidence`: answers an open photo request, otherwise starts a new scan (screens 3.3). */
    closerPhoto: () => {
      const q = detail?.open_question;
      if (q?.kind === "photo_request") {
        router.push({ pathname: "/capture/[scanId]/question/[questionId]", params: { scanId, questionId: q.id } });
      } else if (detail) {
        void startCapture.start({ plantId: detail.plant_id });
      }
    },
    answer: (option: Answer) => {
      if (!question || sending) return;
      const body = { question_id: question.id, option_id: option };
      setSending(option);
      setAnswerProblem(undefined);
      answer
        .mutateAsync(body, keyFor(body))
        .then(() => events.resume())
        .catch((error: unknown) => {
          if (isApiError(error) && error.code === "invalid_state") events.resume();
          else setAnswerProblem("Your answer didn't arrive. Try again.");
        })
        .finally(() => setSending(undefined));
    },
  };

  return {
    loading: !detail,
    gardenId: detail?.garden_id ?? "",
    timeZone: garden.data?.time_zone ?? "UTC",
    error: events.error,
    gone: events.stream === "gone",
    /** The scan has no diagnosis yet (opened from a timeline while it still runs). */
    pending: Boolean(detail && !diagnosis),
    plantName,
    photo: photo ? { source: imageSource(photo.url, photo.id), label: `${plantName}, the photo the diagnosis is from` } : undefined,
    tint,
    view,
    plan: p
      ? {
          summary: p.summary,
          provisional: planNote(p),
          updated: supersededNote(p, state?.plan?.notice),
          watchFor: p.watch_for,
          constraints: p.constraints.map((c) => c.text),
          steps: plan.steps,
          next: plan.next,
          canCheck: plan.canCheck && plan.tasksLoaded,
          checkProblem: plan.failed ? "That step didn't save. Try again." : undefined,
          check: plan.check,
          farmRecords: p.farm_records ?? [],
        }
      : undefined,
    sources,
    /** The record in the sheet, and whether it is a vendor's (else Era's demo data). */
    farmSheet: farmSheet ? { record: farmSheet, vendor: isVendorRecord(farmSheet, connectorList) } : undefined,
    /** Shown as demo data once the connectors are known and the record's isn't a vendor's. */
    isDemoRecord: (record: FarmCitation) => (connectorList ? !isVendorRecord(record, connectorList) : false),
    question,
    canAnswer: can(role, "scan.answer"),
    sending,
    answerProblem,
    canScan: can(role, "scan.create"),
    // Reviewers and up, once the scan is submitted (screens 3.3).
    canFeedback: can(role, "scan.feedback") && detail?.status !== "pending",
    actions,
  };
}

export type DiagnosisView = ReturnType<typeof useDiagnosis>;
