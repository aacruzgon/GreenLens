// Secure random bytes and SHA-256 from the platform (expo-crypto), for the connect flow's device_verifier
// (docs/connectors/README.md 5). The encoding lives in features/connectors/model, which stays pure.
import * as Crypto from "expo-crypto";

export const deviceCrypto = {
  randomBytes(count: number): Uint8Array {
    return Crypto.getRandomBytes(count);
  },
  async sha256(data: Uint8Array): Promise<Uint8Array> {
    return new Uint8Array(await Crypto.digest(Crypto.CryptoDigestAlgorithm.SHA256, Uint8Array.from(data)));
  },
};
