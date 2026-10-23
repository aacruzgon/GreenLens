// Garden map geometry (contracts 9, garden map; architecture 6.8). Pure.

/** A polygon's area in square metres (the shoelace formula), whichever way round it is drawn. */
export function polygonArea(points: readonly (readonly number[])[]): number {
  let twice = 0;
  for (let i = 0; i < points.length; i++) {
    const [x1, y1] = points[i];
    const [x2, y2] = points[(i + 1) % points.length];
    twice += x1 * y2 - x2 * y1;
  }
  return Math.abs(twice) / 2;
}

/** Whether a point lies inside a polygon (ray casting): which bed a pin stands in. */
export function inPolygon(x: number, y: number, points: readonly (readonly number[])[]): boolean {
  let inside = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
    const [xi, yi] = points[i];
    const [xj, yj] = points[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}
