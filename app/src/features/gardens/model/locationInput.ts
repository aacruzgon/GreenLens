import type { GardenLocationInput } from "@/domain/types";

/** What the user may choose with consent: exact, or about 1 km. */
export type ConsentPrecision = NonNullable<GardenLocationInput["precision"]>;

// screens.md 3.10: the field sends `postal_code` when it looks like a Canadian postal code, otherwise `city`.
const CANADIAN_POSTAL_CODE = /^[A-Za-z]\d[A-Za-z][ -]?\d[A-Za-z]\d$/;

export function locationFromText(text: string): GardenLocationInput | null {
  const value = text.trim();
  if (value.length === 0) return null;
  if (CANADIAN_POSTAL_CODE.test(value)) {
    return { postal_code: value.toUpperCase().replace(/[ -]/, " ").replace(/^(\w{3})(\w{3})$/, "$1 $2") };
  }
  return { city: value };
}

export function locationFromDevice(position: { lat: number; lon: number }, precision: ConsentPrecision): GardenLocationInput {
  return { lat: position.lat, lon: position.lon, precision };
}
