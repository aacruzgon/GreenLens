// The plain data the garden home's components take as props (README 5: the view model lives in model/ and
// hooks/, components import their types from here).
import type { ImageSource } from "expo-image";

import type { PlantStatus } from "@/ui";

import type { WeatherView } from "./weather";

export type Loadable<T> =
  | { status: "loading" }
  | { status: "error"; message?: string; retry: () => void }
  | ({ status: "ready" } & T);

export interface PlantCardView {
  id: string;
  name: string;
  status: PlantStatus;
  photo: ImageSource | number | null;
  /** "Scan not submitted". */
  note?: string;
}

export type WeatherCardState =
  | { status: "loading" }
  | { status: "unavailable"; onRetry?: () => void }
  | { status: "error"; message?: string; onRetry: () => void }
  | { status: "ready"; view: WeatherView };

export interface TaskRowView {
  id: string;
  title: string;
  meta?: string;
  done: boolean;
  /** "Starts a scan of Tomato 2", when the row leads to a scan. */
  scanHint?: string;
}

export interface AlertView {
  id: string;
  title: string;
  detail?: string;
}

export interface GardenChoice {
  id: string;
  name: string;
}
