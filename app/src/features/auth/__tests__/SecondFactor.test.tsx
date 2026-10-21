import { fireEvent, render, screen } from "@testing-library/react-native";
import { Pressable, Text } from "react-native";

import { SessionProvider, useSession } from "@/features/auth";

// A signed-in Clerk user whose verified phone isn't the second factor yet; reserving it fails once, then works.
const mockReserve = jest.fn();
jest.mock("@clerk/expo", () => {
  const passthrough = ({ children }: { children: unknown }) => children;
  const user = {
    twoFactorEnabled: false,
    fullName: "Sam",
    username: null,
    phoneNumbers: [{ verification: { status: "verified" }, reservedForSecondFactor: false, setReservedForSecondFactor: (o: unknown) => mockReserve(o) }],
  };
  return {
    ClerkProvider: passthrough,
    ClerkLoaded: passthrough,
    useAuth: () => ({ isLoaded: true, isSignedIn: true, getToken: async () => "t", signOut: async () => {} }),
    useUser: () => ({ user }),
  };
});

function Probe() {
  const { secondFactorProblem } = useSession();
  return secondFactorProblem ? (
    <Pressable onPress={secondFactorProblem.retry}>
      <Text>{secondFactorProblem.message}</Text>
    </Pressable>
  ) : (
    <Text>no problem</Text>
  );
}

// #97: the effect used to swallow this error, so the account kept signing in with a password alone.
it("keeps a failed second-factor setup with its message, and a retry that clears it", async () => {
  mockReserve.mockRejectedValueOnce({ errors: [{ longMessage: "Phone numbers can't be reserved right now." }] }).mockResolvedValueOnce({});
  await render(
    <SessionProvider publishableKey="pk_test_x" onSignedOut={() => {}}>
      <Probe />
    </SessionProvider>,
  );
  expect(await screen.findByText("Phone numbers can't be reserved right now.")).toBeTruthy();
  expect(mockReserve).toHaveBeenCalledWith({ reserved: true });

  await fireEvent.press(screen.getByText("Phone numbers can't be reserved right now."));
  expect(await screen.findByText("no problem")).toBeTruthy();
  expect(mockReserve).toHaveBeenCalledTimes(2);
});
