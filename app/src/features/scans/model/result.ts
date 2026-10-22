// The diagnosis screen's words (roadmap 10 items 2–4; screens 3.3 and 6), from ScanDetail alone. Pure: the
// screen's hook joins the scan with the plant's name and passes plain rows to the components.
import { formatMeasurement, unitOf } from "@/domain/format";
import { diagnosisCopy, organismCopy } from "@/domain/diagnosis";
import type {
  CandidateScore,
  ConfidenceBand,
  Diagnosis,
  IndicatorEvidence,
  Observation,
  Risk,
  ScanDetail,
  SoilAspect,
  WeatherSummary,
} from "@/domain/types";

export interface Header {
  headline: string;
  /** "Tomato 2 · from 3 photos, your answer and the weather". */
  basis: string;
  /** The top candidates under an uncertain headline. */
  candidates: string[];
  /** The words of screens 6 under a `low_evidence` or healthy headline. */
  subline?: string;
}

/** A sideways card: the risk of spread, or something else the photos found (design 3.1). */
export interface FeatureCard {
  key: string;
  kind: "risk" | "organism";
  eyebrow: string;
  title: string;
  detail?: string;
  /** Watch-coloured for a risk or a pest; healthy-coloured for a beneficial organism. */
  tone: "watch" | "healthy" | "neutral";
  /** An invasive organism's reporting page; the app never reports anything itself. */
  reportUrl?: string;
}

export interface EvidenceRow {
  key: string;
  text: string;
  /** Where it came from: "Your answer", "The photos", "LiDAR depth". */
  source: string;
  /** Answered by a LiDAR measurement. */
  measured: boolean;
  matched: boolean;
}

export interface ValueRow {
  key: string;
  label: string;
  value: string;
  band?: ConfidenceBand;
}

/** "Tomato 2 · from 3 photos, your answer and the weather" (design 3.1). */
export function basisLine(plantName: string, diagnosis: Pick<Diagnosis, "evidence_basis">, photoCount: number): string {
  const parts: string[] = [];
  const b = diagnosis.evidence_basis;
  if (b.photo.length > 0 && photoCount > 0) parts.push(photoCount === 1 ? "1 photo" : `${photoCount} photos`);
  if (b.user_input.length > 0) parts.push("your answer");
  if (b.modelled.length > 0) parts.push("the weather");
  if (parts.length === 0) return plantName;
  const list = parts.length === 1 ? parts[0] : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
  return `${plantName} · from ${list}`;
}

export function header(plantName: string, diagnosis: Diagnosis, photoCount: number): Header {
  const copy = diagnosisCopy(diagnosis, photoCount);
  // The healthy and confirmed sublines say what the basis line already says; keep only what adds to it.
  const subline = diagnosis.status === "uncertain" && diagnosis.low_evidence ? copy.subline : diagnosis.status === "healthy" ? copy.subline : undefined;
  return { headline: copy.headline, basis: basisLine(plantName, diagnosis, photoCount), candidates: copy.candidates, subline };
}

const RISK_WORD: Record<string, string> = {
  fungal_spread: "Risk of spread",
  root_rot: "Risk of root rot",
  frost_damage: "Risk of frost damage",
  growth_stalled: "Growth may stall",
};

/** "fungal_spread" → "Risk of spread"; an unknown slug reads as words. */
export function riskWord(risk: string): string {
  const known = RISK_WORD[risk];
  if (known) return known;
  const words = risk.replace(/_/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function riskCard(r: Risk, i: number): FeatureCard {
  // The crowding driver is the one the user can act on, so it leads (contracts 7, crowding).
  const drivers = [...r.drivers].sort((a, b) => Number(b.startsWith("Crowded")) - Number(a.startsWith("Crowded")));
  const [first, ...rest] = drivers;
  return {
    key: `risk-${r.risk}-${i}`,
    kind: "risk",
    eyebrow: `${riskWord(r.risk)}: ${r.level}`,
    title: first ?? riskWord(r.risk),
    detail: rest.length > 0 ? rest.join(" · ") : undefined,
    tone: r.level === "low" ? "neutral" : "watch",
  };
}

function organismCard(o: Diagnosis["organisms"][number]): FeatureCard {
  const copy = organismCopy(o);
  const base = { key: `org-${o.observation_id}`, kind: "organism" as const, eyebrow: "Also found", title: o.name };
  switch (o.role) {
    case "beneficial": {
      const prey = o.preys_on?.[0]?.name;
      return { ...base, tone: "healthy", detail: prey ? `Leave it: it eats ${prey.toLowerCase()}` : copy.headline };
    }
    case "invasive":
      return { ...base, tone: "watch", detail: copy.headline, reportUrl: o.reporting_url };
    case "pest":
      return { ...base, tone: "watch", detail: "A pest" };
    default:
      return { ...base, tone: "neutral" };
  }
}

/** The risks, then what else was found: the cards that scroll sideways under the header. */
export function featureCards(diagnosis: Diagnosis): FeatureCard[] {
  return [...diagnosis.risks.map(riskCard), ...diagnosis.organisms.map(organismCard)];
}

function sourceWord(e: IndicatorEvidence): string {
  if (e.measured || e.source === "measurement") return "LiDAR depth";
  if (e.source === "user") return "Your answer";
  return "The photos";
}

function measuredText(e: IndicatorEvidence, observations: Observation[]): string {
  if (!e.measured) return e.text;
  // The measurement's range, when the scan has it, reads better than one value ("8–12 mm spots").
  const m = observations.find((o) => o.detail?.measurement?.quantity === e.measured?.quantity)?.detail?.measurement;
  const unit = unitOf(e.measured.quantity);
  if (m && m.min !== m.max && unit) return `${Math.round(m.min)}–${Math.round(m.max)} ${unit} ${e.text.charAt(0).toLowerCase()}${e.text.slice(1)}`;
  return e.text;
}

/** What matched and what was contradicted, for the confirmed problem (or the leading one when not sure). */
export function evidenceRows(candidate: CandidateScore | undefined, observations: Observation[]): EvidenceRow[] {
  if (!candidate) return [];
  const row = (e: IndicatorEvidence, matched: boolean): EvidenceRow => ({
    key: `${matched ? "m" : "c"}-${e.indicator_id}`,
    text: measuredText(e, observations),
    source: matched ? sourceWord(e) : e.source === "user" ? "You said no" : "Not seen",
    measured: Boolean(e.measured) || e.source === "measurement",
    matched,
  });
  return [...candidate.matched.map((e) => row(e, true)), ...candidate.contradicted.map((e) => row(e, false))];
}

const QUANTITY_WORD: Record<string, string> = {
  plant_height_cm: "Height",
  canopy_width_cm: "Width",
  leaf_length_cm: "Leaf length",
  leaf_area_cm2: "Leaf area",
  lesion_diameter_mm: "Spot size",
  lesion_count: "Spots",
  affected_area_pct: "Leaf affected",
  hole_diameter_mm: "Hole size",
  hole_count: "Holes",
};

/** Measurements with their units and bands; a range when the value carries one (screens 6: no false precision). */
export function measurementRows(observations: Observation[]): ValueRow[] {
  return observations.flatMap((o) => {
    const m = o.detail?.measurement;
    if (!m) return [];
    const unit = unitOf(m.quantity);
    const value =
      m.min !== m.max && unit !== ""
        ? `${formatMeasurement(m.quantity, m.min).replace(` ${unit}`, "")}–${formatMeasurement(m.quantity, m.max)}`
        : formatMeasurement(m.quantity, m.value);
    return [{ key: o.id, label: QUANTITY_WORD[m.quantity] ?? o.label, value, band: o.confidence_band }];
  });
}

const ASPECT_WORD: Record<SoilAspect, string> = {
  moisture: "Moisture",
  surface: "Surface",
  structure: "Structure",
  colour: "Colour",
  cover: "Cover",
  deposits: "Deposits",
  erosion: "Erosion",
  weeds: "Weeds",
  texture: "Texture",
};

/** The soil as a list, never a score (screens 3.3). */
export function soilRows(soil: Observation[]): ValueRow[] {
  return soil.map((o) => {
    const aspect = o.detail?.aspect;
    const value = o.detail?.value ?? o.label;
    return {
      key: o.id,
      label: aspect ? ASPECT_WORD[aspect] : "Soil",
      value: value.charAt(0).toUpperCase() + value.slice(1),
      band: o.confidence_band,
    };
  });
}

/** Organisms the diagnosis names, with their finding's band (screens 6: bands on findings, never the diagnosis). */
export function organismRows(diagnosis: Diagnosis, observations: Observation[]): ValueRow[] {
  return diagnosis.organisms.map((o) => {
    const obs = observations.find((x) => x.id === o.observation_id);
    const copy = organismCopy(o);
    const value = o.role === "beneficial" ? "Beneficial" : o.role === "pest" ? "Pest" : o.role === "invasive" ? copy.headline : "";
    return { key: o.observation_id, label: o.name, value, band: obs?.confidence_band };
  });
}

/** The weather at the scan, in a few plain rows. */
export function weatherRows(w: WeatherSummary | undefined): ValueRow[] {
  if (!w) return [];
  const rows: ValueRow[] = [
    { key: "now", label: "At the scan", value: `${Math.round(w.current.temp_c)} °C · ${Math.round(w.current.rh_pct)}% humidity` },
    { key: "rain", label: "Rain, last 24 h", value: `${Math.round(w.current.precip_mm_last_24h)} mm` },
  ];
  if (w.past_7_days.hours_rh_over_90 > 0) {
    rows.push({ key: "humid", label: "Past 7 days", value: `${Math.round(w.past_7_days.hours_rh_over_90)} h over 90% humidity` });
  }
  if (w.past_7_days.longest_humid_run_days > 0) {
    const d = w.past_7_days.longest_humid_run_days;
    rows.push({ key: "run", label: "Humid stretch", value: d === 1 ? "1 day" : `${d} days in a row` });
  }
  return rows;
}

/** The candidate whose evidence the lists show: the confirmed one, or the leading one when not sure. */
export function shownCandidate(diagnosis: Diagnosis): CandidateScore | undefined {
  if (diagnosis.status === "healthy" || diagnosis.low_evidence) return undefined;
  return diagnosis.primary ?? diagnosis.candidates[0];
}

/** The photo the header shows: the close-up the finding was on, else the whole plant, else the first. */
export function heroImage(scan: Pick<ScanDetail, "images">): ScanDetail["images"][number] | undefined {
  return scan.images.find((i) => i.kind === "leaf_closeup") ?? scan.images.find((i) => i.kind === "whole_plant") ?? scan.images[0];
}
