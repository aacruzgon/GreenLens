import { ssoOutcome } from "./sso";

describe("ssoOutcome", () => {
  it("is quiet when signed in or when the user closed the browser", () => {
    expect(ssoOutcome({ createdSessionId: "sess_1", authSessionResult: { type: "success" } })).toBeNull();
    expect(ssoOutcome({ createdSessionId: null, authSessionResult: { type: "cancel" } })).toBeNull();
    expect(ssoOutcome({ createdSessionId: null, authSessionResult: { type: "dismiss" } })).toBeNull();
  });

  it("explains a new account that needs a phone", () => {
    const words = ssoOutcome({ createdSessionId: null, authSessionResult: { type: "success" }, signUp: { status: "missing_requirements", missingFields: ["phone_number"] } });
    expect(words).toMatch(/phone number/);
  });

  it("asks to try again otherwise", () => {
    expect(ssoOutcome({ createdSessionId: null, authSessionResult: { type: "success" } })).toBe("The sign-in didn't finish. Try again.");
  });
});
