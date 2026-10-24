import { useState } from "react";

import type { GardenLocationInput } from "@/domain/types";
import { requestGardenLocation } from "@/platform";

import { locationFromDevice, locationFromText, type ConsentPrecision } from "../model/locationInput";

export interface LocationInputState {
  /** What to send, or null until a place is given. */
  location: GardenLocationInput | null;
  usingDevice: boolean;
  precision: ConsentPrecision;
  setPrecision: (p: ConsentPrecision) => void;
  place: string;
  setPlace: (text: string) => void;
  locating: boolean;
  refused: boolean;
  useMyLocation: () => Promise<void>;
  typeInstead: () => void;
}

// Where a garden is (screens 3.10): the phone's location with consent, exact or about 1 km, or a postal code or
// city. Onboarding, a new garden and a garden's location change share it.
export function useLocationInput(): LocationInputState {
  const [place, setPlace] = useState("");
  const [precision, setPrecision] = useState<ConsentPrecision>("about_1_km");
  const [device, setDevice] = useState<GardenLocationInput | null>(null);
  const [locating, setLocating] = useState(false);
  const [refused, setRefused] = useState(false);

  async function useMyLocation() {
    setLocating(true);
    const position = await requestGardenLocation();
    setLocating(false);
    if (!position) return setRefused(true);
    setDevice(locationFromDevice(position, precision));
  }

  return {
    location: device ? { ...device, precision } : locationFromText(place),
    usingDevice: device !== null,
    precision,
    setPrecision,
    place,
    setPlace,
    locating,
    refused,
    useMyLocation,
    typeInstead: () => setDevice(null),
  };
}
