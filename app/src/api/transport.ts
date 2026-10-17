// docs/frontend 9. The one door to the network: live (expo/fetch, window.fetch on the web) or mock.
// openapi-fetch calls fetch with a Request object; expo/fetch accepts one.

/** A file part of a multipart upload. Native passes a file URI; the web build also passes its Blob. */
export interface UploadFile {
  uri: string;
  name: string;
  type: string;
  blob?: Blob;
}

export type UploadPart = { name: string; value: string } | { name: string; file: UploadFile };

export interface Transport {
  fetch(input: string | Request, init?: RequestInit): Promise<Response>;
  /** multipart/form-data POST. Kept apart so each transport builds file parts its own way. */
  upload(url: string, parts: UploadPart[], headers: Record<string, string>): Promise<Response>;
}

let current: Transport | null = null;

export function setTransport(t: Transport) {
  current = t;
}

export function transport(): Transport {
  if (!current) throw new Error("No transport: call setTransport() at start-up");
  return current;
}
