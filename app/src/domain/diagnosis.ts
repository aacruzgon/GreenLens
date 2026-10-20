// The wording of screens.md §6, from Diagnosis.status and low_evidence. The diagnosis has no
// confidence of its own, so no function here returns or formats a number.
import type { Diagnosis } from "./types";

export type DiagnosisCopy = {
  headline: string;
  /** Short line beneath the headline; absent when there is nothing to add. */
  subline?: string;
  /** Names to list beneath an uncertain headline (the top candidates). */
  candidates: string[];
};

export const LOW_EVIDENCE_HEADLINE = "We couldn't tell what's wrong from these photos.";
export const LOW_EVIDENCE_SUBLINE =
  "Take a closer photo of an affected leaf, or ask your local extension office.";
export const HEALTHY_HEADLINE = "Looks healthy";
export const HEALTHY_SUBLINE_ONE_PHOTO = "Only one photo was checked.";
export const PROVISIONAL_PLAN_NOTE =
  "We're not sure yet. These steps are safe for every likely problem.";
export const INVASIVE_NOTE = "This may be an invasive species.";
export const INVASIVE_ACTION = "How to report";
export const BENEFICIAL_FALLBACK = "Beneficial: leave it";

const TOP_CANDIDATES = 2;

type DiagnosisInput = Pick<Diagnosis, "status" | "low_evidence" | "primary" | "candidates">;

/**
 * @param imageCount photos the diagnosis rests on; "Looks healthy" gets a caveat for one photo.
 */
export function diagnosisCopy(diagnosis: DiagnosisInput, imageCount?: number): DiagnosisCopy {
  switch (diagnosis.status) {
    case "confirmed": {
      const name = diagnosis.primary?.name ?? diagnosis.candidates[0]?.name;
      return {
        headline: name ?? "Diagnosis confirmed",
        subline: "Matched on what we saw",
        candidates: [],
      };
    }
    case "uncertain": {
      if (diagnosis.low_evidence) {
        return {
          headline: LOW_EVIDENCE_HEADLINE,
          subline: LOW_EVIDENCE_SUBLINE,
          candidates: [],
        };
      }
      const names = diagnosis.candidates.slice(0, TOP_CANDIDATES).map((c) => c.name);
      return {
        headline: names.length > 0 ? `Not sure yet: ${names.join(" or ")}` : "Not sure yet",
        subline: "One more answer would help",
        candidates: names,
      };
    }
    case "healthy":
      return {
        headline: HEALTHY_HEADLINE,
        subline: imageCount === 1 ? HEALTHY_SUBLINE_ONE_PHOTO : undefined,
        candidates: [],
      };
  }
}

export function planNote(plan: { provisional: boolean }): string | undefined {
  return plan.provisional ? PROVISIONAL_PLAN_NOTE : undefined;
}

type OrganismInput = {
  name: string;
  role: "pest" | "beneficial" | "neutral" | "invasive" | "unknown";
  preys_on?: readonly { name: string }[];
};

/** Organism wording of screens.md §6. Nothing here suggests removing a beneficial organism. */
export function organismCopy(o: OrganismInput): { headline: string; action?: string } {
  switch (o.role) {
    case "beneficial": {
      const prey = o.preys_on?.[0]?.name;
      return {
        headline: prey
          ? `${o.name}: leave it, it eats ${prey.toLowerCase()}`
          : BENEFICIAL_FALLBACK,
      };
    }
    case "invasive":
      return { headline: INVASIVE_NOTE, action: INVASIVE_ACTION };
    default:
      return { headline: o.name };
  }
}
