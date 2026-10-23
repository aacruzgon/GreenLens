// The LiDAR day-1 check's ruler (architecture 10; evaluation, LiDAR row): two points tapped on the photo as shown,
// turned into the sensor frame the depth map is in, lifted to 3D with the intrinsics and their depth, and measured.
// Pure. It ignores lens distortion, which the check also records, so a ruler at the frame's edge reads a little off.

export interface Intrinsics {
  fx: number;
  fy: number;
  cx: number;
  cy: number;
  ref_width: number;
  ref_height: number;
}

/** A point as a fraction of the photo's width and height: as shown, or in the sensor frame. */
export type Norm = { x: number; y: number };

/**
 * A point on the photo as shown to the sensor frame, for the rear camera's EXIF orientations: 1 as stored, 3 turned
 * half, 6 shown turned a quarter clockwise (the iPhone held upright), 8 a quarter anticlockwise. Mirrored ones
 * can't come from the rear camera (contracts 3).
 */
export function toSensor(p: Norm, orientation: number): Norm | undefined {
  switch (orientation) {
    case 1:
      return p;
    case 3:
      return { x: 1 - p.x, y: 1 - p.y };
    case 6:
      return { x: p.y, y: 1 - p.x };
    case 8:
      return { x: 1 - p.y, y: p.x };
    default:
      return undefined;
  }
}

/** A sensor-frame point at a depth, in metres, in camera coordinates (x right, y down, z forward). */
export function unproject(p: Norm, depthM: number, k: Intrinsics): [number, number, number] {
  const u = p.x * k.ref_width;
  const v = p.y * k.ref_height;
  return [((u - k.cx) / k.fx) * depthM, ((v - k.cy) / k.fy) * depthM, depthM];
}

export function distance(a: readonly number[], b: readonly number[]): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

/** The error of a measured length against the known one, as a signed fraction: 0.02 is 2% long. */
export function relativeError(measuredMm: number, knownMm: number): number {
  return (measuredMm - knownMm) / knownMm;
}

/** The check's bar: a scale card on a leaf at 30 cm measured within 3% (evaluation, LiDAR row). */
export const RULER_TOLERANCE = 0.03;

/**
 * How far the camera looks below the horizon, in degrees, from gravity in the camera frame: 90 straight down, 0
 * level, negative above. The spirit-level part of the day-1 check compares it with the phone's own Level.
 */
export function pitchDeg(gravity: readonly number[]): number {
  return (Math.asin(Math.max(-1, Math.min(1, gravity[2]))) * 180) / Math.PI;
}
