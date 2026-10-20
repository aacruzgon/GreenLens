import { clerkErrorMessage } from "./clerkErrors";

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
