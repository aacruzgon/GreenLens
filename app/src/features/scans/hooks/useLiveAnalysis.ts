// The live analysis's view model (roadmap F3; screens 3.3; flows 5–8): the stream's state in the screen's terms,
// the role checks and the actions. The components take its plain data as props.
import { useRouter } from "expo-router";
import { useEffect, useMemo, useRef, useState } from "react";

import { isApiError, useActionKey, useAnswerQuestion, useGarden, useSubmitScan, useUpdateTask } from "@/api";
import { diagnosisCopy } from "@/domain/diagnosis";
import { can } from "@/domain/permissions";
import type { Answer, GuidedKind, ScanImage } from "@/domain/types";
import { useScanJarTask, useStartCapture } from "@/features/capture";

import { hotspotsOf, readingImageId, speciesChip } from "../model/hotspots";
import { phaseLine, phasesOf } from "../model/phases";
import { useScanEvents } from "./useScanEvents";

export type Ending =
  | { kind: "running" }
  | { kind: "plan"; provisional: boolean }
  | { kind: "failed" }
  | { kind: "low_evidence" };

export function useLiveAnalysis(scanId: string) {
  const router = useRouter();
  const events = useScanEvents(scanId);
  const { detail, state } = events;
  const garden = useGarden(detail?.garden_id ?? "");
  const role = garden.data?.my_role;
  const canAnswer = can(role, "scan.answer");
  const canRetry = can(role, "scan.create");
  const canConfirmSpecies = can(role, "species.confirm");
  const canCheckSoil = can(role, "scan.guided_check");

  const answer = useAnswerQuestion(scanId);
  const submit = useSubmitScan();
  const startCapture = useStartCapture();
  const keyFor = useActionKey();
  const [sending, setSending] = useState<Answer>();
  const [answerProblem, setAnswerProblem] = useState<string>();
  const [lastAnswer, setLastAnswer] = useState<{ questionId: string; label: string }>();
  const [chosenImage, setChosenImage] = useState<string>();

  const images: ScanImage[] = useMemo(() => detail?.images ?? [], [detail?.images]);
  const phases = useMemo(() => (state ? phasesOf(state) : []), [state]);
  const line = useMemo(() => (state ? phaseLine(state, phases) : { now: "Loading the scan" }), [state, phases]);
  const hotspots = useMemo(() => (state ? hotspotsOf(state, images) : []), [state, images]);
  const liveKeys = useMemo(() => {
    const live = new Set(state?.liveFindingIds ?? []);
    return new Set(hotspots.filter((h) => h.observationIds.some((id) => live.has(id))).map((h) => h.key));
  }, [hotspots, state?.liveFindingIds]);
  const readingId = chosenImage ?? readingImageId(hotspots, images);
  const image = images.find((i) => i.id === readingId) ?? images[0];

  const plant = detail?.plant;
  const species = state && plant ? speciesChip(state, plant) : undefined;

  const diagnosis = state?.diagnosis;
  const ending: Ending =
    state?.status === "failed" && state.failure ? { kind: "failed" }
    : diagnosis?.low_evidence && state?.plan && state.status === "planned" ? { kind: "low_evidence" }
    : state?.plan && state.status === "planned" ? { kind: "plan", provisional: state.plan.provisional }
    : { kind: "running" };

  // "Not sure yet": while the indicator question is open, both candidates side by side (design 3.1).
  const question = state?.question;
  const unsure = useMemo(() => {
    if (!question || question.kind !== "indicator" || question.candidates.length < 2) return undefined;
    const named = question.candidates.slice(0, 2).map((id) => state?.candidates?.find((c) => c.problem_id === id));
    if (named.some((c) => !c)) return undefined;
    const [a, b] = named as NonNullable<(typeof named)[number]>[];
    return { title: `Not sure yet: ${a!.name} or ${b!.name}`, plates: [a!, b!] as const };
  }, [question, state?.candidates]);

  // A diagnosis left uncertain after the question (Not sure, or the scheduler went on): its words (screens 6).
  const uncertainLine = diagnosis && diagnosis.status === "uncertain" && !diagnosis.low_evidence ? diagnosisCopy(diagnosis).headline : undefined;

  // The jar test's task is done when its photo gave a texture (flows 8); an unsettled jar keeps its task.
  const jarTask = useScanJarTask(detail?.garden_id ?? "", scanId);
  const updateTask = useUpdateTask();
  const closedTask = useRef<string | undefined>(undefined);
  const jarDone = state?.guidedChecks.some((c) => c.kind === "jar_test" && c.status === "done" && c.result);
  useEffect(() => {
    const task = jarTask.task;
    if (!jarDone || !task || closedTask.current === task.id || !can(role, "task.write")) return;
    closedTask.current = task.id;
    updateTask.mutate({ id: task.id, body: { done: true } });
  }, [jarDone, jarTask.task, role, updateTask]);
  const checkingSoil = state?.guidedChecks.some((c) => c.status === "analysing") ?? false;

  const actions = {
    answer: (option: Answer) => {
      if (!question || sending) return;
      const body = { question_id: question.id, option_id: option };
      const label =
        option === "unsure" ? "Not sure" : (question.options?.find((o) => o.id === option)?.label ?? (option === "yes" ? "Yes" : "No"));
      setSending(option);
      setAnswerProblem(undefined);
      answer
        .mutateAsync(body, keyFor(body))
        .then(() => {
          setLastAnswer({ questionId: question.id, label });
          events.resume();
        })
        .catch((error: unknown) => {
          // Answered meanwhile, elsewhere: the card refetches the scan (flows 6).
          if (isApiError(error) && error.code === "invalid_state") events.resume();
          else setAnswerProblem("Your answer didn't arrive. Try again.");
        })
        .finally(() => setSending(undefined));
    },
    takePhoto: () => {
      if (!question) return;
      router.push({ pathname: "/capture/[scanId]/question/[questionId]", params: { scanId, questionId: question.id } });
    },
    retry: () => {
      submit.mutate(scanId, { onSuccess: () => events.resume() });
    },
    newScan: () => {
      if (plant) void startCapture.start({ plantId: plant.id });
    },
    /** Answers an open photo request with a closer photo, or starts a new scan (screens 3.3, low_evidence). */
    closerPhoto: () => {
      if (question?.kind === "photo_request") actions.takePhoto();
      else actions.newScan();
    },
    changeSpecies: () => {
      if (plant) router.push({ pathname: "/plants/[plantId]/species", params: { plantId: plant.id, scanId } });
    },
    checkSoil: (kind: GuidedKind) => {
      router.push({ pathname: "/capture/[scanId]/guided/[kind]", params: { scanId, kind } });
    },
    selectImage: setChosenImage,
    seeResult: () => router.push({ pathname: "/scans/[scanId]/result", params: { scanId } }),
    close: () => router.dismissAll(),
  };

  return {
    loading: !detail || !state,
    gone: events.stream === "gone",
    error: events.error,
    title: plant?.name ?? "Scan",
    state,
    phases,
    line,
    images,
    image,
    hotspots,
    liveKeys,
    species: species ? { name: species.name, onChange: canConfirmSpecies ? actions.changeSpecies : undefined } : undefined,
    question,
    unsure,
    uncertainLine,
    lastAnswer: lastAnswer && !question ? lastAnswer.label : undefined,
    sending,
    answerProblem,
    ending,
    checkingSoil,
    canAnswer,
    canRetry,
    canCheckSoil: canCheckSoil && state?.status !== "failed",
    retrying: submit.isPending,
    actions,
  };
}
