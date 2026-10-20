import { render, screen } from "@testing-library/react-native";
import { StyleSheet } from "react-native";

import { Text } from "../primitives/Text";
import { Enter } from "../primitives/Enter";
import { ThemeProvider } from "../theme";

// No jest setup file yet (package.json is out of this change), so the mocks are local.
jest.mock("react-native-worklets", () => require("react-native-worklets/src/mock"));
jest.mock("react-native-reanimated", () => require("react-native-reanimated/mock"));

async function styleOf(reducedMotion: boolean) {
  await render(
    <ThemeProvider scheme="light" reducedMotion={reducedMotion}>
      <Enter testID="enter">
        <Text>Back garden</Text>
      </Enter>
    </ThemeProvider>,
  );
  expect(screen.getByText("Back garden")).toBeTruthy();
  return StyleSheet.flatten(screen.getByTestId("enter").props.style) as Record<string, unknown>;
}

describe("Enter", () => {
  it("rises and scales when motion is on", async () => {
    const style = await styleOf(false);
    expect(style.transform).toEqual([{ translateY: 24 }, { scale: 0.985 }]);
  });

  it("only fades under reduced motion", async () => {
    const style = await styleOf(true);
    expect(style.transform).toBeUndefined();
    expect(style).toHaveProperty("opacity");
  });
});
