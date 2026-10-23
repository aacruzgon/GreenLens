// What this phone can do (docs/frontend/README.md 13, capabilities by target), asked once. Features ask this; they
// never check the platform or the module themselves.
import { getCapabilities } from "@modules/lidar/src/LidarCamera";

export interface Capabilities {
  /** A back LiDAR depth camera, in a build with the LiDAR module: depth with each photo and the distance hint. */
  depth: boolean;
  /** ARKit's scene reconstruction: the garden mapping session. */
  mapping: boolean;
}

const caps: Capabilities = getCapabilities();

export function capabilities(): Capabilities {
  return caps;
}

/** The same answer as a hook, for components. It never changes while the app runs. */
export function useCapabilities(): Capabilities {
  return caps;
}
