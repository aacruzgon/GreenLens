// Small helpers for the mock backend.
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** A ULID-shaped ID (contracts 1, IDs): sorts by creation time like the real ones. */
export function ulid(now = Date.now()): string {
  let time = "";
  let t = now;
  for (let i = 0; i < 10; i++) {
    time = ALPHABET[t % 32] + time;
    t = Math.floor(t / 32);
  }
  let rand = "";
  for (let i = 0; i < 16; i++) rand += ALPHABET[Math.floor(Math.random() * 32)];
  return time + rand;
}

/** RFC 3339 in UTC, whole seconds (contracts 1, timestamps). */
export function ts(date = new Date()): string {
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

export function localDate(offsetDays = 0, from = new Date()): string {
  const d = new Date(from.getTime() + offsetDays * 86_400_000);
  return d.toISOString().slice(0, 10);
}

export function band(c: number): "high" | "medium" | "low" {
  return c >= 0.8 ? "high" : c >= 0.5 ? "medium" : "low";
}
