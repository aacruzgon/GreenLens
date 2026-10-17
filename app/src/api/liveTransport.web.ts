import type { Transport, UploadPart } from "./transport";

// The web build: the browser's fetch streams natively, and the picker hands over real File blobs.
export const liveTransport: Transport = {
  fetch: (input, init) => window.fetch(input, init),
  async upload(url, parts: UploadPart[], headers) {
    const form = new FormData();
    for (const p of parts) {
      if ("value" in p) form.append(p.name, p.value);
      else form.append(p.name, p.file.blob ?? (await (await window.fetch(p.file.uri)).blob()), p.file.name);
    }
    return window.fetch(url, { method: "POST", body: form, headers });
  },
};
