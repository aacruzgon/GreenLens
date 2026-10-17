import type { GardenLocationInput } from "@/domain/types";

// Location input from consent, a postal code or a city (docs/frontend 11, gardens model).
export type LocationChoice =
  | { kind: "device"; lat: number; lon: number }
  | { kind: "postal_code"; value: string }
  | { kind: "city"; value: string };

const CANADIAN_POSTAL = /^[A-Za-z]\d[A-Za-z][ -]?\d[A-Za-z]\d$/;

/** A typed place: a Canadian postal code or US ZIP is a postal code; anything else is a city. */
export function parsePlace(text: string): LocationChoice | null {
  const v = text.trim();
  if (!v) return null;
  if (CANADIAN_POSTAL.test(v) || /^\d{5}(-\d{4})?$/.test(v)) return { kind: "postal_code", value: v.toUpperCase() };
  return { kind: "city", value: v };
}

export function toLocationInput(choice: LocationChoice): GardenLocationInput {
  switch (choice.kind) {
    case "device":
      return { lat: choice.lat, lon: choice.lon, precision: "about_1_km" };
    case "postal_code":
      return { postal_code: choice.value };
    case "city":
      return { city: choice.value };
  }
}
