import { useId } from "react";

/** A gradient id unique to this component, safe inside url(#…). */
export function useSvgId(prefix: string): string {
  return `${prefix}-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
}
