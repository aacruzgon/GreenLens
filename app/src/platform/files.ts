import { Directory, File, Paths } from "expo-file-system";
import { ImageManipulator, SaveFormat } from "expo-image-manipulator";

/** The long side a re-encoded photo is cut to: well under the 12 MB upload limit as a JPEG (contracts 2). */
const REENCODE_LONG_SIDE = 3000;

// A capture draft's photos (docs/frontend/README.md 11, capture). The camera and the picker write to the cache,
// which iOS may empty, so each shot is copied into the documents folder, one folder per scan, and the draft keeps
// that copy's URI: the exact file it uploads, again after the app closes. The folder goes with the draft.
const root = () => new Directory(Paths.document, "capture");

export const files = {
  /** Copies a photo into the scan's folder under `name` and returns the copy's URI. */
  keep(scanId: string, uri: string, name: string): string {
    const dir = new Directory(root(), scanId);
    dir.create({ intermediates: true, idempotent: true });
    const copy = new File(dir, name);
    if (copy.exists) copy.delete();
    new File(uri).copySync(copy);
    return copy.uri;
  },

  /** Where a kept photo is now. The app's folder can move when iOS updates the app, so a saved URI can go stale. */
  locate(scanId: string, name: string): string | undefined {
    return new File(new Directory(root(), scanId), name).uri;
  },

  /** The multipart file part (README 13, platform/files): expo/fetch takes an expo-file-system File, a Blob. */
  async toUploadPart(uri: string): Promise<Blob> {
    return new File(uri);
  },

  /**
   * A picked photo the server refused (413 or 415) as a JPEG at 80%, its long side cut to 3000 px when longer
   * (flows 4, re-encoding). Returns the new file's URI, in the cache: keep() it like any shot.
   */
  async reencode(uri: string, size: { width: number; height: number }): Promise<string> {
    const ctx = ImageManipulator.manipulate(uri);
    const long = Math.max(size.width, size.height);
    if (long > REENCODE_LONG_SIDE) {
      ctx.resize(size.width >= size.height ? { width: REENCODE_LONG_SIDE } : { height: REENCODE_LONG_SIDE });
    }
    const image = await ctx.renderAsync();
    const saved = await image.saveAsync({ compress: 0.8, format: SaveFormat.JPEG });
    return saved.uri;
  },

  /** Deletes one kept photo, if it is still there. */
  remove(uri: string): void {
    const f = new File(uri);
    if (f.exists) f.delete();
  },

  /** Deletes a scan's folder with every photo in it. */
  drop(scanId: string): void {
    const dir = new Directory(root(), scanId);
    if (dir.exists) dir.delete();
  },

  /** Deletes every draft's photos: on sign-out. */
  dropAll(): void {
    const dir = root();
    if (dir.exists) dir.delete();
  },
};
