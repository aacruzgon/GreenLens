import { fireEvent, render, screen } from "@testing-library/react-native";

import { ResetPasswordScreen, SignUpScreen } from "@/features/auth";
import { ThemeProvider } from "@/ui";

// A Clerk sign-in and sign-up whose calls the tests script; `status` moves the way Clerk's does.
const mockSignIn = {
  status: "needs_identifier" as string,
  create: jest.fn(async (_: unknown) => {
    mockSignIn.status = "needs_first_factor";
    return { error: null as unknown };
  }),
  resetPasswordEmailCode: {
    sendCode: jest.fn(async () => ({ error: null as unknown })),
    verifyCode: jest.fn(async (_: unknown) => ({ error: null as unknown })),
    submitPassword: jest.fn(async (_: unknown) => ({ error: null as unknown })),
  },
  mfa: {
    sendPhoneCode: jest.fn(async () => ({ error: null as unknown })),
    sendEmailCode: jest.fn(async () => ({ error: null as unknown })),
  },
  finalize: jest.fn(async () => ({ error: null as unknown })),
};
const mockSignUp = {
  password: jest.fn(async (_: unknown) => ({ error: null as unknown })),
  verifications: { sendEmailCode: jest.fn(async () => ({ error: null as unknown })) },
};
jest.mock("@clerk/expo", () => ({
  useSignIn: () => ({ signIn: mockSignIn, fetchStatus: "idle" }),
  useSignUp: () => ({ signUp: mockSignUp, fetchStatus: "idle" }),
}));

const mockRouter = { push: jest.fn(), replace: jest.fn() };
let mockParams: Record<string, string> = {};
jest.mock("expo-router", () => ({
  useRouter: () => mockRouter,
  useLocalSearchParams: () => mockParams,
  Link: ({ children }: { children: unknown }) => children,
}));

beforeEach(() => {
  jest.clearAllMocks();
  mockSignIn.status = "needs_identifier";
  mockParams = {};
});

async function sendCodeTo(email: string) {
  mockParams = { email };
  await render(
    <ThemeProvider>
      <ResetPasswordScreen />
    </ThemeProvider>,
  );
  await fireEvent.press(screen.getByRole("button", { name: "Send code" }));
  expect(mockSignIn.create).toHaveBeenCalledWith({ identifier: email });
  expect(await screen.findByText(`Enter the 6-digit code we sent to ${email}, and a new password.`)).toBeTruthy();
}

it("resets the password, then asks for the code by text", async () => {
  await sendCodeTo("sam@example.com");
  mockSignIn.resetPasswordEmailCode.verifyCode.mockImplementationOnce(async () => {
    mockSignIn.status = "needs_new_password";
    return { error: null };
  });
  mockSignIn.resetPasswordEmailCode.submitPassword.mockImplementationOnce(async () => {
    mockSignIn.status = "needs_second_factor";
    return { error: null };
  });

  await fireEvent.changeText(screen.getByLabelText("Code"), "123456");
  await fireEvent.changeText(screen.getByLabelText("New password"), "a new long password");
  await fireEvent.press(screen.getByRole("button", { name: "Reset password" }));

  expect(mockSignIn.resetPasswordEmailCode.verifyCode).toHaveBeenCalledWith({ code: "123456" });
  expect(mockSignIn.resetPasswordEmailCode.submitPassword).toHaveBeenCalledWith({
    password: "a new long password",
    signOutOfOtherSessions: true,
  });
  expect(mockSignIn.mfa.sendPhoneCode).toHaveBeenCalled();
  expect(mockRouter.replace).toHaveBeenCalledWith({ pathname: "/verify", params: { mode: "sms" } });
});

it("keeps the checked code when the new password is refused", async () => {
  await sendCodeTo("sam@example.com");
  mockSignIn.resetPasswordEmailCode.verifyCode.mockImplementationOnce(async () => {
    mockSignIn.status = "needs_new_password";
    return { error: null };
  });
  mockSignIn.resetPasswordEmailCode.submitPassword.mockResolvedValueOnce({
    error: { longMessage: "Password has been found in an online data breach." },
  });

  await fireEvent.changeText(screen.getByLabelText("Code"), "123456");
  await fireEvent.changeText(screen.getByLabelText("New password"), "password1");
  await fireEvent.press(screen.getByRole("button", { name: "Reset password" }));
  expect(await screen.findByText("Password has been found in an online data breach.")).toBeTruthy();

  mockSignIn.resetPasswordEmailCode.submitPassword.mockImplementationOnce(async () => {
    mockSignIn.status = "complete";
    return { error: null };
  });
  await fireEvent.changeText(screen.getByLabelText("New password"), "a new long password");
  await fireEvent.press(screen.getByRole("button", { name: "Reset password" }));

  expect(mockSignIn.resetPasswordEmailCode.verifyCode).toHaveBeenCalledTimes(1);
  expect(mockSignIn.finalize).toHaveBeenCalled();
});

// Review of #113: the password changed, then the text code failed to send; the retry must not check the code again.
it("after the reset, a retry only sends the text code again", async () => {
  await sendCodeTo("sam@example.com");
  mockSignIn.resetPasswordEmailCode.verifyCode.mockImplementationOnce(async () => {
    mockSignIn.status = "needs_new_password";
    return { error: null };
  });
  mockSignIn.resetPasswordEmailCode.submitPassword.mockImplementationOnce(async () => {
    mockSignIn.status = "needs_second_factor";
    return { error: null };
  });
  mockSignIn.mfa.sendPhoneCode.mockResolvedValueOnce({ error: { longMessage: "Too many requests." } });

  await fireEvent.changeText(screen.getByLabelText("Code"), "123456");
  await fireEvent.changeText(screen.getByLabelText("New password"), "a new long password");
  await fireEvent.press(screen.getByRole("button", { name: "Reset password" }));
  expect(await screen.findByText("Too many requests.")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Send a new code" })).toBeNull();

  await fireEvent.press(screen.getByRole("button", { name: "Continue" }));
  expect(mockSignIn.resetPasswordEmailCode.verifyCode).toHaveBeenCalledTimes(1);
  expect(mockSignIn.resetPasswordEmailCode.submitPassword).toHaveBeenCalledTimes(1);
  expect(mockSignIn.mfa.sendPhoneCode).toHaveBeenCalledTimes(2);
  expect(mockRouter.replace).toHaveBeenCalledWith({ pathname: "/verify", params: { mode: "sms" } });
});

it("says so when the account has no password to reset", async () => {
  mockSignIn.resetPasswordEmailCode.sendCode.mockResolvedValueOnce({
    error: { code: "factor_not_found", message: "Reset password email code factor not found" },
  });
  mockParams = { email: "sam@example.com" };
  await render(
    <ThemeProvider>
      <ResetPasswordScreen />
    </ThemeProvider>,
  );
  await fireEvent.press(screen.getByRole("button", { name: "Send code" }));
  expect(
    await screen.findByText("This account signs in without a password, so there's no password to reset."),
  ).toBeTruthy();
});

it("offers a reset when sign-up finds the email taken", async () => {
  mockSignUp.password.mockResolvedValueOnce({
    error: { errors: [{ code: "form_identifier_exists", meta: { paramName: "email_address" } }] },
  });
  await render(
    <ThemeProvider>
      <SignUpScreen />
    </ThemeProvider>,
  );
  await fireEvent.changeText(screen.getByLabelText("Email"), " sam@example.com ");
  await fireEvent.changeText(screen.getByLabelText("Phone number"), "555 555 0100");
  await fireEvent.changeText(screen.getByLabelText("Password"), "a long password");
  await fireEvent.press(screen.getByRole("button", { name: "Continue" }));

  expect(await screen.findByText("You already have an account")).toBeTruthy();
  expect(mockSignUp.verifications.sendEmailCode).not.toHaveBeenCalled();
  await fireEvent.press(screen.getByRole("button", { name: "Reset password" }));
  expect(mockRouter.push).toHaveBeenCalledWith({ pathname: "/reset-password", params: { email: "sam@example.com" } });
});
