import { fireEvent, render, screen } from "@testing-library/react-native";

import { Button } from "../primitives/Button";
import { ThemeProvider } from "../theme";

async function renderButton(props: Partial<React.ComponentProps<typeof Button>> = {}) {
  const onPress = jest.fn();
  await render(
    <ThemeProvider scheme="light" reducedMotion={false}>
      <Button label="See the diagnosis and plan" onPress={onPress} {...props} />
    </ThemeProvider>,
  );
  return { onPress, button: screen.getByRole("button", { name: "See the diagnosis and plan" }) };
}

describe("Button", () => {
  it("is an enabled button by default and presses", async () => {
    const { onPress, button } = await renderButton();
    expect(button).toBeEnabled();
    expect(button).not.toBeBusy();
    await fireEvent.press(button);
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it("reports disabled and ignores presses", async () => {
    const { onPress, button } = await renderButton({ disabled: true });
    expect(button).toBeDisabled();
    expect(button).not.toBeBusy();
    await fireEvent.press(button);
    expect(onPress).not.toHaveBeenCalled();
  });

  it("reports busy, which also disables it", async () => {
    const { onPress, button } = await renderButton({ busy: true });
    expect(button).toBeDisabled();
    expect(button).toBeBusy();
    await fireEvent.press(button);
    expect(onPress).not.toHaveBeenCalled();
  });

  it("puts a primary icon in the disc and keeps the 44 pt minimum", async () => {
    const icon = jest.fn(() => null);
    const { button } = await renderButton({ icon });
    expect(icon).toHaveBeenCalledWith({ color: "#2F6B47", size: 18 });
    expect(button).toHaveStyle({ minHeight: 44 });
  });
});
