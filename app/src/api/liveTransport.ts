import { fetch as expoFetch } from "expo/fetch";
import { File } from "expo-file-system";

import type { Transport, UploadPart } from "./transport";

// Native: expo/fetch gives streaming bodies, and rejects React Native's { uri } form parts,
// so files go in as expo-file-system File objects (docs/frontend 4).
export const liveTransport: Transport = {
  fetch: (input, init) => expoFetch(input as never, init as never) as unknown as Promise<Response>,
  upload(url, parts: UploadPart[], headers) {
    const form = new FormData();
    for (const p of parts) {
      if ("value" in p) form.append(p.name, p.value);
      else form.append(p.name, new File(p.file.uri) as unknown as Blob, p.file.name);
    }
    return expoFetch(url, { method: "POST", body: form as never, headers }) as unknown as Promise<Response>;
  },
};
