// The mock's timeline and measurement routes with what Insights reads (roadmap F7 item 3; design 8; analytics 5):
// timeline entries with `low_evidence`, the primary's `kind` and `weather_fit`, `weather_7d` and `organisms`, and
// measurement points of every quantity with `min`, `max`, `n` and `image_kind`. These are the app's proposals in
// domain/proposed.ts (`InsightTimelineEntry`, `RangePoint`); every field is added to the contract's shape, so a
// reader of the plain `TimelineEntry` and `MeasurementPoint` sees what it saw before.
import type { InsightTimelineEntry, RangePoint } from "@/domain/proposed";

import type { MockStore, StoredScan } from "./store";
import { errorResponse, jsonResponse, paginate, type S } from "./util";

interface InsightRouteCtx {
  params: Record<string, string>;
  query: URLSearchParams;
  store: MockStore;
}

type Register = (method: string, pattern: string, handler: (c: InsightRouteCtx) => Response | Promise<Response>) => void;

const QUANTITIES: readonly S["MeasureQuantity"][] = [
  "plant_height_cm", "canopy_width_cm", "leaf_length_cm", "leaf_area_cm2", "lesion_diameter_mm", "lesion_count",
  "affected_area_pct", "hole_diameter_mm", "hole_count",
];
const GROWTH: readonly S["MeasureQuantity"][] = ["plant_height_cm", "canopy_width_cm"];

/** The store's timeline (newest first), each entry with the proposed Insights fields from its scan. */
export function insightTimeline(store: MockStore, plantId: string): InsightTimelineEntry[] {
  return store.timelineOf(plantId).map((e): InsightTimelineEntry => {
    const st = store.scan(e.scan_id);
    const out: InsightTimelineEntry = { ...e };
    const d = st?.diagnosis;
    if (d?.low_evidence) out.low_evidence = true;
    if (e.primary && d?.primary) out.primary = { ...e.primary, kind: d.primary.kind, weather_fit: d.primary.weather_fit };
    const w = st?.weather?.past_7_days;
    if (w) out.weather_7d = { hours_rh_over_90: w.hours_rh_over_90, longest_humid_run_days: w.longest_humid_run_days, precip_mm: w.precip_mm };
    if (d?.organisms?.length) out.organisms = d.organisms.map((o) => ({ name: o.name, role: o.role }));
    return out;
  });
}

/** The highest-confidence measurement of `q` on the scan, medium or more, as the growth series picks it. */
function best(st: StoredScan, q: S["MeasureQuantity"]): RangePoint | undefined {
  let pick: S["Observation"] | undefined;
  for (const o of st.observations) {
    if (o.detail?.measurement?.quantity !== q || o.confidence_band === "low") continue;
    if (!pick || o.confidence > pick.confidence) pick = o;
  }
  const m = pick?.detail?.measurement;
  if (!pick || !m) return undefined;
  const kind = st.scan.images.find((i) => i.id === pick.region?.image_id)?.kind;
  return {
    scan_id: st.scan.id,
    quantity: q,
    value: m.value,
    confidence_band: pick.confidence_band,
    created_at: st.scan.created_at,
    min: m.min,
    max: m.max,
    n: m.n,
    ...(kind ? { image_kind: kind } : {}),
  };
}

/** Oldest first, one point per scan and quantity; no quantity means the growth pair, as the contract has it. */
export function rangePoints(store: MockStore, plantId: string, quantity?: S["MeasureQuantity"]): RangePoint[] {
  const out: RangePoint[] = [];
  for (const st of store.scansOf(plantId)) {
    if (st.scan.status === "pending" || st.scan.status === "failed") continue;
    for (const q of quantity ? [quantity] : GROWTH) {
      const p = best(st, q);
      if (p) out.push(p);
    }
  }
  return out;
}

export function registerInsightRoutes(route: Register): void {
  route("GET", "/plants/:id/timeline", (c) =>
    c.store.plants.has(c.params.id)
      ? jsonResponse(200, paginate(insightTimeline(c.store, c.params.id), c.query))
      : errorResponse("not_found"),
  );

  route("GET", "/plants/:id/measurements", (c) => {
    if (!c.store.plants.has(c.params.id)) return errorResponse("not_found");
    const q = c.query.get("quantity");
    if (q && !QUANTITIES.includes(q as S["MeasureQuantity"])) {
      return errorResponse("invalid_request", { field: "quantity", problem: "unknown quantity" });
    }
    return jsonResponse(200, paginate(rangePoints(c.store, c.params.id, (q as S["MeasureQuantity"] | null) ?? undefined), c.query));
  });
}
