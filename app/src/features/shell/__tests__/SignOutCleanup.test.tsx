import { fireEvent, render, screen } from "@testing-library/react-native";
import { Pressable, Text } from "react-native";

import { SessionProvider, useSession } from "@/features/auth";

function Probe() {
  const session = useSession();
  return (
    <>
      <Text>{session.isSignedIn ? "in" : "out"}</Text>
      <Pressable onPress={session.signInDemo}>
        <Text>sign in</Text>
      </Pressable>
      <Pressable onPress={() => void session.signOut()}>
        <Text>sign out</Text>
      </Pressable>
    </>
  );
}

// The review of #67: cleanup must run on every sign-out, not only Settings' button.
it("runs the sign-out cleanup when a session ends, and not before one starts", async () => {
  const onSignedOut = jest.fn();
  await render(
    <SessionProvider publishableKey="" onSignedOut={onSignedOut}>
      <Probe />
    </SessionProvider>,
  );
  expect(onSignedOut).not.toHaveBeenCalled();

  await fireEvent.press(screen.getByText("sign in"));
  expect(await screen.findByText("in")).toBeTruthy();
  expect(onSignedOut).not.toHaveBeenCalled();

  await fireEvent.press(screen.getByText("sign out"));
  expect(await screen.findByText("out")).toBeTruthy();
  expect(onSignedOut).toHaveBeenCalledTimes(1);
});
