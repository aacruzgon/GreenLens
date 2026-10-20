// Clerk takes phone numbers in E.164. North American numbers may be typed without +1.
export function toE164(input: string): string | null {
  const trimmed = input.trim();
  const digits = trimmed.replace(/\D/g, "");
  if (trimmed.startsWith("+")) return digits.length >= 8 && digits.length <= 15 ? `+${digits}` : null;
  if (digits.length === 10) return `+1${digits}`;
  if (digits.length === 11 && digits.startsWith("1")) return `+${digits}`;
  return null;
}

/** Clerk's test numbers on a development instance: +1 (XXX) 555-0100 to 0199; no SMS is sent, the code is 424242. */
export const TEST_PHONE_HINT = "For testing: +1 555 555 0100 to 0199, code 424242. No text is sent.";
