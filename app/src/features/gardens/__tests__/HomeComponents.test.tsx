import { fireEvent, render, screen } from "@testing-library/react-native";
import type { ReactNode } from "react";

import { ThemeProvider } from "@/ui";

import { AlertCard } from "../components/AlertCard";
import { GardenHero } from "../components/GardenHero";
import { GardenNotes } from "../components/GardenNotes";
import { HeroControls } from "../components/HeroControls";
import { PlantCarousel } from "../components/PlantCarousel";
import { WeatherCard } from "../components/WeatherCard";
import type { WeatherView } from "../model/weather";

function themed(node: ReactNode) {
  return render(<ThemeProvider scheme="light">{node}</ThemeProvider>);
}

const weather: WeatherView = {
  days: ["Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map((label, i) => ({
    date: `2026-10-${20 + i}`,
    label,
    longLabel: label,
    kind: i === 2 ? "snow" : "cloud",
    high: "13°",
    low: i === 2 ? "−1°" : "6°",
    frost: i === 2,
    accessibilityLabel: label,
  })),
  frostNote: "Frost possible Thursday",
  updated: "Updated 10:00",
};

describe("WeatherCard", () => {
  it("shows six days with the frost day marked, and when it was fetched", async () => {
    await themed(<WeatherCard state={{ status: "ready", view: weather }} />);
    expect(screen.getAllByTestId(/^weather-day/)).toHaveLength(6);
    expect(screen.getAllByTestId("weather-day-frost")).toHaveLength(1);
    expect(screen.getByText("Frost possible Thursday")).toBeTruthy();
    expect(screen.getByText("Updated 10:00")).toBeTruthy();
  });

  it("says the weather is unavailable on a 503", async () => {
    await themed(<WeatherCard state={{ status: "unavailable" }} />);
    expect(screen.getByText("Weather unavailable")).toBeTruthy();
  });

  it("shows skeletons while loading", async () => {
    await themed(<WeatherCard state={{ status: "loading" }} />);
    expect(screen.getByTestId("weather-loading")).toBeTruthy();
  });
});

describe("AlertCard", () => {
  it("offers Done to editors", async () => {
    const onDone = jest.fn();
    await themed(<AlertCard title="Frost on Thursday: cover tender plants" detail="The low is -1 °C." onDone={onDone} />);
    await fireEvent.press(screen.getByRole("button", { name: "Done: Frost on Thursday: cover tender plants" }));
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it("hides Done for viewers", async () => {
    await themed(<AlertCard title="Frost on Thursday: cover tender plants" />);
    expect(screen.queryByText("Done")).toBeNull();
  });
});

describe("PlantCarousel", () => {
  it("shows each plant's name, status and a pending scan", async () => {
    await themed(
      <PlantCarousel
        plants={[
          { id: "t1", name: "Tomato 1", status: "sick", photo: { uri: "https://example.com/t1.jpg" } },
          { id: "p", name: "Pepper", status: "watch", photo: null, note: "Scan not submitted" },
        ]}
      />,
    );
    expect(screen.getByText("Tomato 1")).toBeTruthy();
    expect(screen.getByLabelText("Status: Sick")).toBeTruthy();
    expect(screen.getByLabelText("Pepper, Watch, Scan not submitted")).toBeTruthy();
    expect(screen.getByTestId("icon-leaf", { includeHiddenElements: true })).toBeTruthy();
  });
});

describe("GardenHero", () => {
  it("sets the garden's name as the header with its place and weather", async () => {
    await themed(
      <GardenHero
        name="Back garden"
        eyebrow="Toronto · Monday"
        weatherLine="11 °C after light rain, 84% humidity. Frost possible Thursday night."
        photo={{ uri: "https://example.com/basil.jpg" }}
        photoLabel="Basil"
        ground="#EEF2E9"
        height={470}
        topInset={59}
      />,
    );
    expect(screen.getByRole("header", { name: "Back garden" })).toBeTruthy();
    expect(screen.getByText("Toronto · Monday")).toBeTruthy();
    expect(screen.getByText("11 °C after light rain, 84% humidity. Frost possible Thursday night.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Switch garden/ })).toBeNull();
  });

  it("makes the name the garden switcher when one is given", async () => {
    const onSwitch = jest.fn();
    await themed(
      <GardenHero name="Back garden" photo={null} photoLabel="" ground="#EEF2E9" height={470} topInset={59} onSwitchGarden={onSwitch} />,
    );
    await fireEvent.press(screen.getByRole("button", { name: "Back garden. Switch garden" }));
    expect(onSwitch).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("header", { name: "Back garden" })).toBeTruthy();
  });
});

const ready = { status: "ready" as const, items: [{ id: "m", text: "Everything is in containers" }] };

describe("GardenNotes", () => {
  it("shows a skeleton while loading", async () => {
    await themed(<GardenNotes state={{ status: "loading" }} />);
    expect(screen.getByTestId("notes-loading")).toBeTruthy();
  });

  it("shows an error with a retry", async () => {
    const retry = jest.fn();
    await themed(<GardenNotes state={{ status: "error", message: "Offline", retry }} />);
    expect(screen.getByText("Couldn't load the notes")).toBeTruthy();
    await fireEvent.press(screen.getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("offers Add to editors only", async () => {
    const onAddNote = jest.fn();
    const { rerender } = await themed(<GardenNotes state={ready} onAddNote={onAddNote} />);
    expect(screen.getByText("Everything is in containers")).toBeTruthy();
    await fireEvent.press(screen.getByRole("link", { name: "Add a note" }));
    expect(onAddNote).toHaveBeenCalledTimes(1);
    await rerender(
      <ThemeProvider scheme="light">
        <GardenNotes state={ready} />
      </ThemeProvider>,
    );
    expect(screen.queryByRole("link", { name: "Add a note" })).toBeNull();
  });
});

describe("HeroControls", () => {
  it("offers + to editors", async () => {
    const onAdd = jest.fn();
    await themed(<HeroControls onAddPlant={onAdd} overPhoto topInset={0} />);
    await fireEvent.press(screen.getByRole("button", { name: "Add a plant" }));
    expect(onAdd).toHaveBeenCalledTimes(1);
  });

  it("shows nothing to viewers", async () => {
    await themed(<HeroControls overPhoto={false} topInset={0} />);
    expect(screen.queryByRole("button", { name: "Add a plant" })).toBeNull();
    expect(screen.queryByTestId("hero-controls")).toBeNull();
  });
});
