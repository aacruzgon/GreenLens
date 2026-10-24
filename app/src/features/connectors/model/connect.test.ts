// The connect flow's device proof and redirect (docs/connectors/README.md 5; EON T-E10.1).
import { createHash } from "crypto";

import { base64url, deviceProof, parseRedirect, RETURN_URL } from "./connect";

const sha256 = async (d: Uint8Array) => new Uint8Array(createHash("sha256").update(d).digest());

describe("the device proof", () => {
  it("encodes base64url with no padding", () => {
    expect(base64url(new Uint8Array([0xfb, 0xff]))).toBe("-_8");
    expect(base64url(new Uint8Array([1, 2, 3, 4]))).toBe("AQIDBA");
    expect(base64url(new Uint8Array(32).fill(0xff))).toHaveLength(43);
  });

  it("hashes the verifier as PKCE's S256 does (RFC 7636 appendix B, the server's own vector)", async () => {
    const random = new Uint8Array([
      116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212, 37, 77, 105, 214, 191, 240,
      91, 88, 5, 88, 83, 132, 141, 121,
    ]);
    const proof = await deviceProof(random, sha256);
    expect(proof.verifier).toBe("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    expect(proof.hash).toBe("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
  });

  it("matches the contract's 43-character pattern, and refuses anything but 32 bytes", async () => {
    const proof = await deviceProof(new Uint8Array(32).map((_, i) => i * 7), sha256);
    expect(proof.verifier).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(proof.hash).toMatch(/^[A-Za-z0-9_-]{43}$/);
    await expect(deviceProof(new Uint8Array(16), sha256)).rejects.toThrow();
  });
});

describe("the redirect", () => {
  const id = "01JAX3K9T6R2B8V0QZ4M7D1E5C";

  it("reads the code and state, percent-decoded, from the connector's callback", () => {
    expect(parseRedirect(`${RETURN_URL}/${id}/callback?code=a%2Fb%20c&state=${id}.n.1.m`, id)).toEqual({
      kind: "code",
      code: "a/b c",
      state: `${id}.n.1.m`,
    });
  });

  it("reads a declined consent", () => {
    expect(parseRedirect(`${RETURN_URL}/${id}/callback?error=access_denied&state=s`, id)).toEqual({ kind: "declined", error: "access_denied" });
  });

  it("refuses another connector's callback, another scheme, or a missing state", () => {
    expect(parseRedirect(`${RETURN_URL}/other/callback?code=c&state=s`, id).kind).toBe("invalid");
    expect(parseRedirect(`https://evil.example/connectors/${id}/callback?code=c&state=s`, id).kind).toBe("invalid");
    expect(parseRedirect(`${RETURN_URL}/${id}/callback?code=c`, id).kind).toBe("invalid");
  });
});
