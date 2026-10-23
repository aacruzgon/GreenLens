// What the camera screens say while the LiDAR camera is taken away (README 13; HIG, camera interruptions). An
// interrupted session still counts as running, and a photo taken then is black with no depth, so the shutter
// waits and the screen says why. Pure.

/** Why the camera was taken away, as the LiDAR module reports it. */
export type CameraPause = "in_use" | "multitasking" | "system_pressure" | "background" | "other";

export function pauseText(reason: CameraPause): string {
  switch (reason) {
    case "in_use":
      return "Another app is using the camera. It comes back when that ends.";
    case "multitasking":
      return "The camera pauses while another app shares the screen.";
    case "system_pressure":
      return "The iPhone is too warm for the camera. Let it cool for a moment.";
    default:
      return "The camera is paused. It comes back on its own.";
  }
}
