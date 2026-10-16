//! Binary masks: an owned buffer (`MaskBuf`) and `domain`'s borrowed `MaskView<'a>`, with the
//! operations the measurement stage needs (resampling, erosion, connected components, holes,
//! outlines).

use std::collections::VecDeque;

use domain::ports::MaskView;

use crate::imaging::PixelRect;

/// An owned 8-bit mask, row-major: non-zero inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskBuf {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl MaskBuf {
    /// An empty mask.
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; width as usize * height as usize],
        }
    }

    /// From raw data; `None` when the length doesn't match.
    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        (data.len() == width as usize * height as usize).then_some(Self {
            width,
            height,
            data,
        })
    }

    /// The filled rectangle (a box without a mask, for the plant box or a whole-image region).
    pub fn from_rect(width: u32, height: u32, rect: PixelRect) -> Self {
        let mut m = Self::empty(width, height);
        for y in rect.y..(rect.y + rect.h).min(height) {
            for x in rect.x..(rect.x + rect.w).min(width) {
                m.set(x, y, true);
            }
        }
        m
    }

    /// The borrowed view.
    pub fn view(&self) -> MaskView<'_> {
        // reason: the constructors keep the length rule `MaskView::new` checks.
        MaskView::new(self.width, self.height, &self.data).unwrap_or_else(|| unreachable!())
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Whether `(x, y)` is inside.
    pub fn get(&self, x: u32, y: u32) -> bool {
        self.data[y as usize * self.width as usize + x as usize] != 0
    }

    /// Sets `(x, y)`.
    pub fn set(&mut self, x: u32, y: u32, inside: bool) {
        self.data[y as usize * self.width as usize + x as usize] = u8::from(inside) * 255;
    }

    /// The number of pixels inside.
    pub fn count(&self) -> usize {
        self.data.iter().filter(|&&v| v != 0).count()
    }

    /// The raw bytes.
    pub fn as_raw(&self) -> &[u8] {
        &self.data
    }
}

/// Nearest-neighbour resampling of a mask to another size of the same field of view (the
/// stored image's mask onto the depth map's grid).
pub fn resample(mask: MaskView<'_>, width: u32, height: u32) -> MaskBuf {
    let mut out = MaskBuf::empty(width, height);
    let (sx, sy) = (
        f64::from(mask.width()) / f64::from(width),
        f64::from(mask.height()) / f64::from(height),
    );
    for y in 0..height {
        let my = (((f64::from(y) + 0.5) * sy) as u32).min(mask.height() - 1);
        for x in 0..width {
            let mx = (((f64::from(x) + 0.5) * sx) as u32).min(mask.width() - 1);
            if mask.data()[my as usize * mask.width() as usize + mx as usize] != 0 {
                out.set(x, y, true);
            }
        }
    }
    out
}

/// Erodes by `radius` pixels with a square structuring element: a pixel stays inside only when
/// every pixel within `radius` (Chebyshev distance) is inside. Contracts 7 shrinks masks by two
/// depth pixels before reading depth.
pub fn erode(mask: MaskView<'_>, radius: u32) -> MaskBuf {
    let (w, h) = (mask.width(), mask.height());
    let inside = |x: u32, y: u32| mask.data()[y as usize * w as usize + x as usize] != 0;
    // Separable: rows, then columns. A pixel survives a pass when its run of inside pixels
    // reaches `radius` on both sides.
    let mut rows = MaskBuf::empty(w, h);
    for y in 0..h {
        for x in 0..w {
            let lo = x.checked_sub(radius);
            let hi = (x + radius < w).then_some(x + radius);
            if let (Some(lo), Some(hi)) = (lo, hi)
                && (lo..=hi).all(|xx| inside(xx, y))
            {
                rows.set(x, y, true);
            }
        }
    }
    let mut out = MaskBuf::empty(w, h);
    for y in 0..h {
        for x in 0..w {
            let lo = y.checked_sub(radius);
            let hi = (y + radius < h).then_some(y + radius);
            if let (Some(lo), Some(hi)) = (lo, hi)
                && (lo..=hi).all(|yy| rows.get(x, yy))
            {
                out.set(x, y, true);
            }
        }
    }
    out
}

/// One 4-connected component: its pixels, row-major order of discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    /// `(x, y)` of each pixel.
    pub pixels: Vec<(u32, u32)>,
    /// Whether any pixel lies on the image border.
    pub touches_border: bool,
}

impl Component {
    /// The pixel count.
    pub fn area(&self) -> usize {
        self.pixels.len()
    }

    /// The bounding rectangle.
    pub fn bounds(&self) -> PixelRect {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for &(x, y) in &self.pixels {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        PixelRect {
            x: x0,
            y: y0,
            w: x1 + 1 - x0,
            h: y1 + 1 - y0,
        }
    }
}

/// The 4-connected components of the pixels for which `select(x, y)` holds, breadth first.
pub fn components(width: u32, height: u32, select: impl Fn(u32, u32) -> bool) -> Vec<Component> {
    let mut seen = vec![false; width as usize * height as usize];
    let idx = |x: u32, y: u32| y as usize * width as usize + x as usize;
    let mut out = Vec::new();
    let mut queue = VecDeque::new();
    for y in 0..height {
        for x in 0..width {
            if seen[idx(x, y)] || !select(x, y) {
                continue;
            }
            seen[idx(x, y)] = true;
            queue.push_back((x, y));
            let mut c = Component {
                pixels: Vec::new(),
                touches_border: false,
            };
            while let Some((cx, cy)) = queue.pop_front() {
                c.pixels.push((cx, cy));
                c.touches_border |= cx == 0 || cy == 0 || cx + 1 == width || cy + 1 == height;
                let neighbours = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in neighbours {
                    if nx < width && ny < height && !seen[idx(nx, ny)] && select(nx, ny) {
                        seen[idx(nx, ny)] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
            out.push(c);
        }
    }
    out
}

/// Holes: gaps enclosed by the mask's outline ([contracts 7]), the components of the outside
/// pixels within the mask's bounding box that don't reach the box's edge.
pub fn holes(mask: MaskView<'_>) -> Vec<Component> {
    let (w, h) = (mask.width(), mask.height());
    let inside = |x: u32, y: u32| mask.data()[y as usize * w as usize + x as usize] != 0;
    let leaf = components(w, h, inside);
    let Some(bounds) = leaf.iter().max_by_key(|c| c.area()).map(Component::bounds) else {
        return Vec::new();
    };
    let in_box = |x: u32, y: u32| {
        x >= bounds.x && y >= bounds.y && x < bounds.x + bounds.w && y < bounds.y + bounds.h
    };
    components(w, h, |x, y| in_box(x, y) && !inside(x, y))
        .into_iter()
        .filter(|c| {
            !c.pixels.iter().any(|&(x, y)| {
                x == bounds.x
                    || y == bounds.y
                    || x + 1 == bounds.x + bounds.w
                    || y + 1 == bounds.y + bounds.h
            })
        })
        .collect()
}

/// The outline: inside pixels with at least one 4-neighbour outside (or on the border).
pub fn outline(mask: MaskView<'_>) -> Vec<(u32, u32)> {
    let (w, h) = (mask.width(), mask.height());
    let inside = |x: u32, y: u32| mask.data()[y as usize * w as usize + x as usize] != 0;
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            let edge = x == 0
                || y == 0
                || x + 1 == w
                || y + 1 == h
                || !inside(x - 1, y)
                || !inside(x + 1, y)
                || !inside(x, y - 1)
                || !inside(x, y + 1);
            if edge {
                out.push((x, y));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring() -> MaskBuf {
        // a 9×9 square leaf with a 3×3 hole in the middle
        let mut m = MaskBuf::empty(11, 11);
        for y in 1..10 {
            for x in 1..10 {
                m.set(x, y, !(4..7).contains(&x) || !(4..7).contains(&y));
            }
        }
        m
    }

    #[test]
    fn finds_the_hole_and_not_the_outside() {
        let h = holes(ring().view());
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].area(), 9);
    }

    #[test]
    fn erosion_shrinks_by_the_radius() {
        let m = MaskBuf::from_rect(
            10,
            10,
            PixelRect {
                x: 1,
                y: 1,
                w: 8,
                h: 8,
            },
        );
        let e = erode(m.view(), 2);
        assert_eq!(e.count(), 16); // 8 − 2·2 = 4 on each side
        assert!(e.get(3, 3) && !e.get(2, 3));
    }

    #[test]
    fn resample_keeps_the_field_of_view() {
        let m = MaskBuf::from_rect(
            8,
            8,
            PixelRect {
                x: 0,
                y: 0,
                w: 4,
                h: 8,
            },
        );
        let r = resample(m.view(), 4, 4);
        assert_eq!(r.count(), 8);
        assert!(r.get(1, 0) && !r.get(2, 0));
    }

    #[test]
    fn outline_is_the_rim() {
        let m = MaskBuf::from_rect(
            5,
            5,
            PixelRect {
                x: 1,
                y: 1,
                w: 3,
                h: 3,
            },
        );
        assert_eq!(outline(m.view()).len(), 8);
    }
}
