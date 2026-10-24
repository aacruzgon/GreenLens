// The vendor's consent page in an auth session (docs/connectors/README.md 5): openAuthSessionAsync uses
// ASWebAuthenticationSession on iOS, which closes on the redirect to the app's scheme and hands back its URL to the
// app that opened it, and to no other. Mock mode sets a fake that answers at once, so the connect flow runs in the
// simulator without a vendor (docs/frontend/README.md 15).
import * as WebBrowser from "expo-web-browser";

/** `success` with the redirect's URL, or the session closed without one. */
export type AuthSessionResult = { type: "success"; url: string } | { type: "cancel" };

export type AuthSessionFake = (url: string, redirectUrl: string) => Promise<AuthSessionResult>;

let fake: AuthSessionFake | null = null;

export const authSession = {
  async open(url: string, redirectUrl: string): Promise<AuthSessionResult> {
    if (fake) return fake(url, redirectUrl);
    const result = await WebBrowser.openAuthSessionAsync(url, redirectUrl);
    return result.type === "success" ? { type: "success", url: result.url } : { type: "cancel" };
  },
  /** Mock mode and tests: answer every session with this instead of the browser; null restores it. */
  setFake(f: AuthSessionFake | null): void {
    fake = f;
  },
};
