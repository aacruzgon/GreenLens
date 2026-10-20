import { fireEvent, render, screen } from "@testing-library/react-native";
import { StyleSheet } from "react-native";

import { ThemeProvider } from "@/ui";

import { TaskRow } from "../components/TaskRow";

async function renderRow(props: Partial<Parameters<typeof TaskRow>[0]> = {}, reducedMotion = false) {
  await render(
    <ThemeProvider scheme="light" reducedMotion={reducedMotion}>
      <TaskRow title="Check Tomato 2's lower leaves" meta="Tomato 2 · weekly" done={false} {...props} />
    </ThemeProvider>,
  );
}

describe("TaskRow", () => {
  it("shows the title and its meta line", async () => {
    await renderRow();
    expect(screen.getByText("Check Tomato 2's lower leaves")).toBeTruthy();
    expect(screen.getByText("Tomato 2 · weekly")).toBeTruthy();
  });

  it("checks the task when an editor taps the box", async () => {
    const onCheck = jest.fn();
    await renderRow({ onCheck });
    await fireEvent.press(screen.getByRole("checkbox", { name: "Mark done: Check Tomato 2's lower leaves" }));
    expect(onCheck).toHaveBeenCalledTimes(1);
  });

  it("hides the box for viewers", async () => {
    await renderRow();
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("draws a done task checked and struck through, and can't be checked twice", async () => {
    const onCheck = jest.fn();
    await renderRow({ onCheck, done: true });
    const box = screen.getByRole("checkbox", { name: "Check Tomato 2's lower leaves, done" });
    expect(box.props.accessibilityState).toMatchObject({ checked: true });
    await fireEvent.press(box);
    expect(onCheck).not.toHaveBeenCalled();
    const title = StyleSheet.flatten(screen.getByText("Check Tomato 2's lower leaves").props.style);
    expect(title.textDecorationLine).toBe("line-through");
  });

  it("opens what the task leads to", async () => {
    const onPress = jest.fn();
    await renderRow({ onPress, pressHint: "Starts a scan of Tomato 2" });
    await fireEvent.press(screen.getByText("Check Tomato 2's lower leaves"));
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it("renders under reduced motion", async () => {
    await renderRow({ onCheck: () => {}, done: true }, true);
    expect(screen.getByRole("checkbox")).toBeTruthy();
  });
});
