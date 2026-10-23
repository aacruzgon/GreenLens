import { foregroundGrace } from "./foregroundGrace";

describe("the stream's background grace (README 10)", () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  function setup(running: boolean) {
    const calls: boolean[] = [];
    let clock = 0;
    const grace = foregroundGrace({ setForeground: (fg) => calls.push(fg) }, { graceMs: 25_000, liveActivityRunning: () => running, now: () => clock });
    return { calls, grace, advance: (ms: number) => (clock += ms) };
  }

  it("closes at once without a Live Activity", () => {
    const { calls, grace } = setup(false);
    grace.change("background");
    expect(calls).toEqual([false]);
  });

  it("keeps the stream for the grace with a Live Activity, then closes it", () => {
    const { calls, grace, advance } = setup(true);
    grace.change("background");
    expect(calls).toEqual([]);
    advance(25_000);
    jest.advanceTimersByTime(25_000);
    expect(calls).toEqual([false]);
    grace.change("active");
    expect(calls).toEqual([false, true]);
  });

  it("returning within the grace leaves the open stream alone", () => {
    const { calls, grace, advance } = setup(true);
    grace.change("background");
    advance(5_000);
    jest.advanceTimersByTime(5_000);
    grace.change("active");
    expect(calls).toEqual([true]);
    jest.advanceTimersByTime(30_000);
    expect(calls).toEqual([true]);
  });

  it("reconnects on return when JavaScript was suspended before the timer fired", () => {
    const { calls, grace, advance } = setup(true);
    grace.change("background");
    // Suspended: the clock moves, the timer doesn't.
    advance(60_000);
    grace.change("active");
    expect(calls).toEqual([false, true]);
  });

  it("inactive leaves the stream open", () => {
    const { calls, grace } = setup(true);
    grace.change("inactive");
    grace.change("active");
    expect(calls).toEqual([true]);
  });
});
