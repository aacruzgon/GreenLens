// The web build (F9) has no file system: a picked photo is a blob: URL that lives as long as the tab, so a
// draft doesn't survive a reload there (docs/frontend/README.md 11, capture; open decision 19.9).
export const files = {
  keep(_scanId: string, uri: string, _name: string): string {
    return uri;
  },

  async toUploadPart(uri: string): Promise<Blob> {
    return (await fetch(uri)).blob();
  },

  /** Browsers send the original; an unsupported_media_type answer asks for a JPEG (README 11, picked photos). */
  async reencode(uri: string, _size: { width: number; height: number }): Promise<string> {
    return uri;
  },

  remove(_uri: string): void {},

  drop(_scanId: string): void {},

  dropAll(): void {},
};
