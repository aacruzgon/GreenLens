import { Redirect } from "expo-router";

import { devTools } from "@/config/env";
import { DepthCheckScreen } from "@/features/depthcheck";

// A development or evaluation build's screen (roadmap F5): a build for users has none, deep links included.
export default function DepthCheckRoute() {
  return devTools ? <DepthCheckScreen /> : <Redirect href="/" />;
}
