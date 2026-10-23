import { distance, pitchDeg, relativeError, toSensor, unproject } from "./ruler";

const k = { fx: 1000, fy: 1000, cx: 500, cy: 400, ref_width: 1000, ref_height: 800 };

describe("the day-1 check's ruler", () => {
  it("turns a point as shown into the sensor frame for each rear-camera orientation", () => {
    // The sensor's top-left corner is shown top-right when the phone is held upright (orientation 6).
    expect(toSensor({ x: 1, y: 0 }, 6)).toEqual({ x: 0, y: 0 });
    expect(toSensor({ x: 0, y: 1 }, 8)).toEqual({ x: 0, y: 0 });
    expect(toSensor({ x: 0.25, y: 0.75 }, 3)).toEqual({ x: 0.75, y: 0.25 });
    expect(toSensor({ x: 0.2, y: 0.3 }, 1)).toEqual({ x: 0.2, y: 0.3 });
    expect(toSensor({ x: 0.2, y: 0.3 }, 2)).toBeUndefined();
  });

  it("measures a 100 mm card lying square to the camera at 30 cm", () => {
    // 100 mm at 300 mm with fx 1000 spans 333.3 px: from the centre 166.7 px each way.
    const a = unproject({ x: (500 - 166.6667) / 1000, y: 0.5 }, 0.3, k);
    const b = unproject({ x: (500 + 166.6667) / 1000, y: 0.5 }, 0.3, k);
    expect(distance(a, b) * 1000).toBeCloseTo(100, 2);
  });

  it("counts depth along the line of sight", () => {
    expect(distance(unproject({ x: 0.5, y: 0.5 }, 0.3, k), unproject({ x: 0.5, y: 0.5 }, 0.35, k)) * 1000).toBeCloseTo(50, 6);
  });

  it("gives the error as a signed fraction, and the camera's pitch from gravity", () => {
    expect(relativeError(103, 100)).toBeCloseTo(0.03);
    expect(pitchDeg([0, 0, 1])).toBeCloseTo(90);
    expect(pitchDeg([1, 0, 0])).toBeCloseTo(0);
  });
});
