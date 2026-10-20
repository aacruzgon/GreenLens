import { render, screen } from "@testing-library/react-native";

import { StatusChip } from "../composites/StatusChip";
import { ThemeProvider } from "../theme";
import type { PlantStatus } from "../tokens";

const CASES: [PlantStatus, string, string][] = [
  ["healthy", "Healthy", "circle"],
  ["watch", "Watch", "triangle"],
  ["sick", "Sick", "diamond"],
  ["dead", "Dead", "square"],
];

describe("StatusChip", () => {
  it.each(CASES)("shows %s as the word %s and the %s shape", async (status, word, shape) => {
    await render(
      <ThemeProvider scheme="light">
        <StatusChip status={status} />
      </ThemeProvider>,
    );
    expect(screen.getByText(word)).toBeTruthy();
    expect(screen.getByLabelText(`Status: ${word}`)).toBeTruthy();
    expect(screen.getByTestId(`status-shape-${shape}`, { includeHiddenElements: true })).toBeTruthy();
  });
});
