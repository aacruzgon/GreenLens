import { renderHook, waitFor } from "@testing-library/react-native";
import { isLiquidGlassAvailable } from "expo-glass-effect";
import { AccessibilityInfo } from "react-native";

import { useGlass } from "../glass";

jest.mock("expo-glass-effect", () => ({ isLiquidGlassAvailable: jest.fn(() => true), GlassView: "GlassView" }));
const available = isLiquidGlassAvailable as jest.Mock;

describe("useGlass", () => {
  beforeEach(() => {
    available.mockReturnValue(true);
    jest.spyOn(AccessibilityInfo, "isReduceTransparencyEnabled").mockResolvedValue(false);
  });

  it("draws glass where iOS has it and Reduce Transparency is off", async () => {
    const { result } = await renderHook(() => useGlass());
    await waitFor(() => expect(AccessibilityInfo.isReduceTransparencyEnabled).toHaveBeenCalled());
    expect(result.current).toBe(true);
  });

  it("keeps the solid pill with Reduce Transparency on", async () => {
    jest.spyOn(AccessibilityInfo, "isReduceTransparencyEnabled").mockResolvedValue(true);
    const { result } = await renderHook(() => useGlass());
    await waitFor(() => expect(result.current).toBe(false));
  });

  it("keeps the solid pill where glass isn't available, or its module is missing", async () => {
    available.mockReturnValue(false);
    expect((await renderHook(() => useGlass())).result.current).toBe(false);
    available.mockImplementation(() => {
      throw new Error("Cannot find native module 'ExpoGlassEffect'");
    });
    expect((await renderHook(() => useGlass())).result.current).toBe(false);
  });
});
