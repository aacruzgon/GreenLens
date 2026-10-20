import { randomUUID } from "expo-crypto";

// A v4 UUID from the platform's secure random source (expo-crypto), used for idempotency keys.
export function uuid(): string {
  return randomUUID();
}
