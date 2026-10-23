// `DepthMeta` (contracts 3, the `depth_meta` part of POST /scans/{id}/images), as the Rust type in
// backend/crates/domain/src/scan.rs has it. It isn't in the OpenAPI spec (a multipart part's JSON), so it is written
// out here. The server refuses a meta without `intrinsics` (`invalid_meta`), so the LiDAR module sends no depth
// without them.
export interface DepthMeta {
  source: "lidar";
  /** Pixels of a reference size, in the sensor frame. */
  intrinsics: { fx: number; fy: number; cx: number; cy: number; ref_width: number; ref_height: number };
  /** Sent only if the day-1 check asks for it (contracts open question 11). */
  distortion?: { center: [number, number]; inverse_lookup: number[] };
  /** A unit vector towards the ground in camera axes (x right, y down, z forward). */
  gravity?: [number, number, number];
  /** "iPhone16,1". */
  device_model: string;
  filtered: boolean;
}
