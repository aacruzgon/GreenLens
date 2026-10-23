import { Redirect } from "expo-router";

import { devTools } from "@/config/env";
import { Garden3DSpikeScreen } from "@/features/garden3d";

// A development or evaluation build's screen (roadmap F5): a build for users has none, deep links included.
export default function Garden3DRoute() {
  return devTools ? <Garden3DSpikeScreen /> : <Redirect href="/" />;
}
