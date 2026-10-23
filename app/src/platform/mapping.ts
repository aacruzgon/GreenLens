// The garden mapping session (architecture 6.8; flows 11), from modules/lidar. Present only where
// `capabilities().mapping` is true; elsewhere the view is null and the functions do nothing.
import {
  GardenMappingView,
  relocalise,
  removeWorldMaps,
  saveWorldMap,
  savedWorldMapVersion,
  type GardenMappingHandle,
  type GardenMappingViewProps,
  type TrackingState,
} from "@modules/lidar/src/GardenMapping";

export { GardenMappingView, type GardenMappingHandle, type GardenMappingViewProps, type TrackingState };

export const mapping = {
  /** The world map stays on the phone, tagged with the map version the upload produced. */
  saveWorldMap,
  savedWorldMapVersion,
  relocalise,
  /** On sign-out: no garden's space stays on the phone. */
  removeAll: removeWorldMaps,
};
