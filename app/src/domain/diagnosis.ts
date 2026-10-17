import type { Diagnosis, OrganismRole, Prey } from "./types";

// Wording for uncertain results (screens.md 6). The diagnosis has no confidence of its own:
// its wording comes from status and low_evidence, never a number.

export interface DiagnosisWording {
  headline: string;
  body?: string;
  /** Problems to show under the headline; empty when none may be named. */
  named: { problem_id: string; name: string }[];
  tone: "healthy" | "watch" | "sick" | "neutral";
}

export function wordDiagnosis(d: Pick<Diagnosis, "status" | "low_evidence" | "primary" | "candidates">): DiagnosisWording {
  switch (d.status) {
    case "healthy":
      return { headline: "Looks healthy", named: [], tone: "healthy" };
    case "confirmed": {
      const p = d.primary ?? d.candidates[0];
      return {
        headline: p?.name ?? "Problem found",
        named: p ? [{ problem_id: p.problem_id, name: p.name }] : [],
        tone: "sick",
      };
    }
    case "uncertain": {
      if (d.low_evidence) {
        return {
          headline: "We couldn't tell what's wrong from these photos",
          body: "Take a closer photo of an affected leaf, or ask your local extension office.",
          named: [],
          tone: "neutral",
        };
      }
      const top = d.candidates.slice(0, 2);
      const names = top.map((c) => c.name);
      return {
        headline: names.length > 1 ? `Not sure yet: ${names[0]} or ${names[1]}` : `Not sure yet: ${names[0] ?? "a few possibilities"}`,
        named: top.map((c) => ({ problem_id: c.problem_id, name: c.name })),
        tone: "watch",
      };
    }
  }
}

export const PROVISIONAL_PLAN_NOTE = "We're not sure yet. These steps are safe for every likely problem.";

export function wordOrganism(o: { name: string; role: OrganismRole; preys_on?: Prey[] }): string {
  switch (o.role) {
    case "beneficial": {
      const prey = o.preys_on?.map((p) => p.name.toLowerCase()) ?? [];
      return prey.length > 0 ? `${o.name}: leave it, it eats ${joinWords(prey)}` : `${o.name}: beneficial, leave it`;
    }
    case "invasive":
      return `${o.name}: this may be an invasive species`;
    case "pest":
      return `${o.name}: pest`;
    default:
      return o.name;
  }
}

export function joinWords(words: string[]): string {
  if (words.length <= 1) return words.join("");
  return `${words.slice(0, -1).join(", ")} and ${words[words.length - 1]}`;
}
