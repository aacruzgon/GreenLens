import { clerkErrorCode, clerkErrorMessage, isEmailTaken } from "./clerkErrors";

describe("clerkErrorMessage", () => {
  it("prefers the long message", () => {
    expect(clerkErrorMessage({ message: "Invalid", longMessage: "That password is incorrect." })).toBe(
      "That password is incorrect.",
    );
  });

  it("reads the first error of an API response", () => {
    expect(clerkErrorMessage({ errors: [{ message: "Code expired" }] })).toBe("Code expired");
  });

  it("falls back to a generic line", () => {
    expect(clerkErrorMessage(undefined)).toBe("Something went wrong. Try again.");
  });
});

describe("isEmailTaken", () => {
  it("is true for a taken email, on the error or the first of a list", () => {
    expect(isEmailTaken({ code: "form_identifier_exists", meta: { paramName: "email_address" } })).toBe(true);
    expect(isEmailTaken({ errors: [{ code: "form_identifier_exists", meta: { paramName: "email_address" } }] })).toBe(
      true,
    );
    expect(isEmailTaken({ code: "form_identifier_exists" })).toBe(true);
  });

  it("is false for a taken phone number and for other errors", () => {
    expect(isEmailTaken({ code: "form_identifier_exists", meta: { paramName: "phone_number" } })).toBe(false);
    expect(isEmailTaken({ code: "form_password_pwned" })).toBe(false);
    expect(isEmailTaken(undefined)).toBe(false);
  });
});

describe("clerkErrorCode", () => {
  it("reads the code of the error or of the first in a list", () => {
    expect(clerkErrorCode({ code: "factor_not_found" })).toBe("factor_not_found");
    expect(clerkErrorCode({ errors: [{ code: "form_code_incorrect" }] })).toBe("form_code_incorrect");
    expect(clerkErrorCode({ message: "no code" })).toBeUndefined();
  });
});
