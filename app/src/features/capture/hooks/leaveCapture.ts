import type { useRouter } from "expo-router";

/**
 * Leaves a capture screen: back to where the user came from, or to the garden home when capture is the first screen
 * (opened from a link, a push notification or the web's address bar), where there is nothing to go back to.
 */
export function leaveCapture(router: ReturnType<typeof useRouter>): void {
  if (router.canGoBack()) router.back();
  else router.replace("/");
}
