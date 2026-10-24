// The connect flow's pure half (docs/connectors/README.md 5; EON E10.2): the device_verifier and its hash, and the
// redirect the API's OAuth callback sends back into the app. The verifier is 32 random bytes in base64url, 43
// characters; the hash is SHA-256 of those 43 characters, in base64url too, as PKCE's S256 (RFC 7636) and the
// server's `device_matches` compute it.

/** `CONNECTOR_RETURN_URL`'s default: the callback answers `302 pdm://connectors/{id}/callback?code&state`. */
export const RETURN_URL = "pdm://connectors";

const B64URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/** Base64url with no padding (RFC 4648 5). */
export function base64url(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const n = (bytes[i]! << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    const chars = i + 2 < bytes.length ? 4 : i + 1 < bytes.length ? 3 : 2;
    for (let j = 0; j < chars; j++) out += B64URL[(n >> (18 - 6 * j)) & 63];
  }
  return out;
}

export interface DeviceProof {
  /** Stays on this device until `complete`. */
  verifier: string;
  /** Sent with the create. */
  hash: string;
}

/** The verifier from 32 random bytes, and its hash. `sha256` is the platform's (src/platform, deviceCrypto). */
export async function deviceProof(random: Uint8Array, sha256: (data: Uint8Array) => Promise<Uint8Array>): Promise<DeviceProof> {
  if (random.length !== 32) throw new Error("device_verifier needs 32 random bytes");
  const verifier = base64url(random);
  const ascii = Uint8Array.from(verifier, (ch) => ch.charCodeAt(0));
  return { verifier, hash: base64url(await sha256(ascii)) };
}

export type Redirect =
  | { kind: "code"; code: string; state: string }
  /** The owner declined, or the vendor stopped (`access_denied`, or `vendor_error` for a code that isn't plain). */
  | { kind: "declined"; error: string }
  /** Not this connector's callback. */
  | { kind: "invalid" };

function decode(v: string): string {
  try {
    return decodeURIComponent(v);
  } catch {
    return v;
  }
}

/** Reads `{RETURN_URL}/{id}/callback?code&state` or `?error&state` for the connector the app started. */
export function parseRedirect(url: string, connectorId: string, returnUrl: string = RETURN_URL): Redirect {
  const [path = "", query = ""] = url.split("#")[0]!.split("?", 2);
  if (path.replace(/\/+$/, "") !== `${returnUrl}/${encodeURIComponent(connectorId)}/callback`) return { kind: "invalid" };
  const params = new Map<string, string>();
  for (const part of query.split("&")) {
    if (!part) continue;
    const eq = part.indexOf("=");
    const k = decode(eq === -1 ? part : part.slice(0, eq));
    if (!params.has(k)) params.set(k, decode(eq === -1 ? "" : part.slice(eq + 1)));
  }
  const error = params.get("error");
  if (error) return { kind: "declined", error };
  const code = params.get("code");
  const state = params.get("state");
  return code && state ? { kind: "code", code, state } : { kind: "invalid" };
}
