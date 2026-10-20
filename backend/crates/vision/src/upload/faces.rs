//! Face detection and blurring on upload ([architecture 11]; model in [vision models 9]):
//! `YuNet` 2026may on the CPU, two passes (the whole image at 1280 pixels, then overlapping
//! 1280-pixel tiles of the full-resolution image), boxes union-merged across passes and grown by
//! 25% before a strong Gaussian blur. Any failure fails the upload: no photo is stored unblurred.
//!
//! [architecture 11]: ../../../docs/architecture/README.md#11-security-privacy-and-safety
//! [vision models 9]: ../../../docs/vision/models.md#9-face-detector

use image::{RgbImage, imageops};
use ndarray::Array4;
use ort::{inputs, session::Session, value::TensorRef};

use crate::{
    VisionError,
    imaging::{self, ImageView, PixelRect},
    manifest::Manifest,
    runtime::{OrtRuntime, SessionPool, SessionSettings},
};

/// The detector's settings ([vision models 9], passes for a 12-megapixel photo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceConfig {
    /// Pass A: the whole image scaled to this long side.
    pub pass_a_long_side: u32,
    /// Pass B: square tiles of the full-resolution image, this many pixels a side.
    pub tile: u32,
    /// Pass B: the step between tiles (960 = 25% overlap).
    pub tile_stride: u32,
    /// Recall first: a false blur costs a little evidence, a missed face is a privacy failure.
    pub score_threshold: f32,
    /// `NMS` within a pass.
    pub nms_iou: f32,
    /// Boxes from different passes are united when they overlap by more than this or one
    /// contains the other.
    pub merge_iou: f32,
    /// Each side grows by this share of the box before blurring.
    pub grow: f32,
    /// Sessions in the pool: how many uploads detect faces at once.
    pub sessions: usize,
}

impl Default for FaceConfig {
    fn default() -> Self {
        Self {
            pass_a_long_side: 1280,
            tile: 1280,
            tile_stride: 960,
            score_threshold: 0.3,
            nms_iou: 0.3,
            merge_iou: 0.2,
            grow: 0.25,
            sessions: 2,
        }
    }
}

/// A face box, normalised to the upright image (`[0, 1]` on each axis), so the same box applies
/// to the photo at any size and to its depth map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBox {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// `YuNet`'s score.
    pub score: f32,
}

impl FaceBox {
    fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    fn intersection(&self, other: &Self) -> f32 {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        (x1 - x0).max(0.0) * (y1 - y0).max(0.0)
    }

    /// Intersection over union.
    pub fn iou(&self, other: &Self) -> f32 {
        let inter = self.intersection(other);
        let union = self.area() + other.area() - inter;
        if union <= 0.0 { 0.0 } else { inter / union }
    }

    /// Whether one box lies inside the other.
    pub fn contains_or_inside(&self, other: &Self) -> bool {
        let inter = self.intersection(other);
        inter > 0.0 && (inter >= other.area() * 0.999 || inter >= self.area() * 0.999)
    }

    /// The smallest box holding both.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.w).max(other.x + other.w);
        let y1 = (self.y + self.h).max(other.y + other.h);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
            score: self.score.max(other.score),
        }
    }

    /// Grown by `share` of its size on each side, clamped to the image.
    #[must_use]
    pub fn grown(&self, share: f32) -> Self {
        let (dx, dy) = (self.w * share, self.h * share);
        let x0 = (self.x - dx).max(0.0);
        let y0 = (self.y - dy).max(0.0);
        let x1 = (self.x + self.w + dx).min(1.0);
        let y1 = (self.y + self.h + dy).min(1.0);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
            score: self.score,
        }
    }

    /// The box in pixels of an image of `size`, rounded outwards.
    pub fn to_pixels(&self, size: (u32, u32)) -> PixelRect {
        PixelRect::from_normalised(
            [
                f64::from(self.x),
                f64::from(self.y),
                f64::from(self.w),
                f64::from(self.h),
            ],
            size,
        )
    }
}

/// Greedy `NMS` within one pass: highest score first, drop boxes overlapping a kept one.
pub fn nms(mut boxes: Vec<FaceBox>, iou: f32) -> Vec<FaceBox> {
    boxes.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<FaceBox> = Vec::new();
    for b in boxes {
        if kept.iter().all(|k| k.iou(&b) <= iou) {
            kept.push(b);
        }
    }
    kept
}

/// Unites boxes from different passes: any two that overlap by more than `iou`, or where one
/// contains the other, become their union, repeated until nothing changes. Union rather than
/// suppression, so a face half-caught by a tile is covered by the whole-image box and vice versa.
pub fn union_merge(mut boxes: Vec<FaceBox>, iou: f32) -> Vec<FaceBox> {
    loop {
        let mut merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if boxes[i].iou(&boxes[j]) > iou || boxes[i].contains_or_inside(&boxes[j]) {
                    let u = boxes[i].union(&boxes[j]);
                    boxes.swap_remove(j);
                    boxes[i] = u;
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return boxes;
        }
    }
}

/// `YuNet`'s twelve outputs for one stride: `cls`, `obj` (`[1, A, 1]`) and `bbox` (`[1, A, 4]`).
/// `kps` isn't needed for blurring.
#[derive(Debug, Clone, Copy)]
pub struct StrideOutputs<'a> {
    /// 8, 16 or 32.
    pub stride: u32,
    /// Class scores.
    pub cls: &'a [f32],
    /// Objectness.
    pub obj: &'a [f32],
    /// Box offsets, 4 per anchor.
    pub bbox: &'a [f32],
}

/// Decodes one stride (`OpenCV`'s `face_detect.cpp`): anchor `i` sits at row `i / cols`, column
/// `i % cols`; score = √(clamp(cls) · clamp(obj)); centre = ((col + b0)·s, (row + b1)·s); size =
/// (e^b2·s, e^b3·s). Boxes come out in pixels of the padded input.
pub fn decode_stride(out: StrideOutputs<'_>, padded_width: u32, threshold: f32) -> Vec<[f32; 5]> {
    let cols = (padded_width / out.stride) as usize;
    let s = out.stride as f32;
    let mut boxes = Vec::new();
    for (i, (&cls, &obj)) in out.cls.iter().zip(out.obj).enumerate() {
        let score = (cls.clamp(0.0, 1.0) * obj.clamp(0.0, 1.0)).sqrt();
        if score < threshold || cols == 0 {
            continue;
        }
        let (row, col) = ((i / cols) as f32, (i % cols) as f32);
        let b = &out.bbox[i * 4..i * 4 + 4];
        let (cx, cy) = ((col + b[0]) * s, (row + b[1]) * s);
        let (w, h) = (b[2].exp() * s, b[3].exp() * s);
        boxes.push([cx - w / 2.0, cy - h / 2.0, w, h, score]);
    }
    boxes
}

/// The face detector: a pool of `YuNet` sessions and the pass settings.
pub struct FaceDetector {
    sessions: SessionPool,
    config: FaceConfig,
    version: String,
}

impl std::fmt::Debug for FaceDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceDetector")
            .field("config", &self.config)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl FaceDetector {
    /// The manifest entry's name.
    pub const MODEL: &'static str = "yunet";

    /// Loads `config.sessions` sessions of the manifest's `yunet` file (hash checked).
    pub fn load(
        runtime: &OrtRuntime,
        manifest: &Manifest,
        config: FaceConfig,
    ) -> Result<Self, VisionError> {
        manifest.verify(&[Self::MODEL])?;
        let entry = manifest.entry(Self::MODEL)?;
        let path = manifest.path(entry);
        let sessions = (0..config.sessions.max(1))
            .map(|_| runtime.session(&path, SessionSettings::default()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            sessions: SessionPool::new(Self::MODEL, sessions),
            config,
            version: entry.version.clone(),
        })
    }

    /// The model version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Every face in the upright image, as normalised, merged and **not yet grown** boxes.
    /// Blocking: runs both passes on one pooled session.
    #[allow(
        clippy::many_single_char_names,
        reason = "w, h, tx, ty: image and tile geometry"
    )]
    pub fn detect(&self, image: ImageView<'_>) -> Result<Vec<FaceBox>, VisionError> {
        let mut session = self.sessions.checkout();
        let (w, h) = (image.width(), image.height());
        let mut found = Vec::new();
        // Pass A: the whole image at 1280 pixels.
        let small = imaging::resize_long_side(image, self.config.pass_a_long_side)?;
        let scale = (f64::from(w) / f64::from(small.width())) as f32;
        for b in self.run_pass(&mut session, ImageView::from(&small))? {
            found.push(normalise(b, (0, 0), scale, (w, h)));
        }
        // Pass B: full-resolution tiles, only when the image is larger than one tile.
        if w.max(h) > self.config.tile {
            for (tx, ty) in tile_origins((w, h), self.config.tile, self.config.tile_stride) {
                let rect = PixelRect {
                    x: tx,
                    y: ty,
                    w: self.config.tile.min(w - tx),
                    h: self.config.tile.min(h - ty),
                };
                let tile = imaging::crop(image, rect);
                for b in self.run_pass(&mut session, ImageView::from(&tile))? {
                    found.push(normalise(b, (tx, ty), 1.0, (w, h)));
                }
            }
        }
        Ok(union_merge(found, self.config.merge_iou))
    }

    /// One `YuNet` run: BGR, 0–255, padded with zeros to multiples of 32 at the bottom and right.
    /// Returns `[x, y, w, h, score]` in the pass image's pixels, after `NMS`.
    #[allow(
        clippy::many_single_char_names,
        reason = "w, h, x, y, r, g, b: image notation"
    )]
    fn run_pass(
        &self,
        session: &mut Session,
        image: ImageView<'_>,
    ) -> Result<Vec<[f32; 5]>, VisionError> {
        let (w, h) = (image.width(), image.height());
        let (pw, ph) = (w.div_ceil(32) * 32, h.div_ceil(32) * 32);
        let mut input = Array4::<f32>::zeros((1, 3, ph as usize, pw as usize));
        for y in 0..h {
            for x in 0..w {
                let [r, g, b] = image.rgb(x, y);
                let (yy, xx) = (y as usize, x as usize);
                input[[0, 0, yy, xx]] = f32::from(b);
                input[[0, 1, yy, xx]] = f32::from(g);
                input[[0, 2, yy, xx]] = f32::from(r);
            }
        }
        let outputs = session.run(inputs!["input" => TensorRef::from_array_view(&input)?])?;
        let mut boxes = Vec::new();
        for stride in [8u32, 16, 32] {
            let tensor = |prefix: &str| -> Result<&[f32], VisionError> {
                let name = format!("{prefix}_{stride}");
                let value = outputs
                    .get(name.as_str())
                    .ok_or_else(|| VisionError::Shape {
                        model: Self::MODEL,
                        detail: format!("missing output {name}"),
                    })?;
                Ok(value.try_extract_tensor::<f32>()?.1)
            };
            let decoded = decode_stride(
                StrideOutputs {
                    stride,
                    cls: tensor("cls")?,
                    obj: tensor("obj")?,
                    bbox: tensor("bbox")?,
                },
                pw,
                self.config.score_threshold,
            );
            boxes.extend(decoded);
        }
        let as_faces: Vec<FaceBox> = boxes
            .iter()
            .map(|b| FaceBox {
                x: b[0],
                y: b[1],
                w: b[2],
                h: b[3],
                score: b[4],
            })
            .collect();
        Ok(nms(as_faces, self.config.nms_iou)
            .into_iter()
            .map(|f| [f.x, f.y, f.w, f.h, f.score])
            .collect())
    }

    /// The share each side of a face box grows by before blurring.
    pub const fn grow(&self) -> f32 {
        self.config.grow
    }
}

fn normalise(b: [f32; 5], origin: (u32, u32), scale: f32, size: (u32, u32)) -> FaceBox {
    let (iw, ih) = (size.0 as f32, size.1 as f32);
    let x = (b[0] * scale + origin.0 as f32).clamp(0.0, iw);
    let y = (b[1] * scale + origin.1 as f32).clamp(0.0, ih);
    let x1 = ((b[0] + b[2]) * scale + origin.0 as f32).clamp(0.0, iw);
    let y1 = ((b[1] + b[3]) * scale + origin.1 as f32).clamp(0.0, ih);
    FaceBox {
        x: x / iw,
        y: y / ih,
        w: (x1 - x) / iw,
        h: (y1 - y) / ih,
        score: b[4],
    }
}

/// Tile origins covering `size` with square tiles of `tile` pixels every `stride` pixels; the
/// last row and column are pulled back so they end at the image edge.
pub fn tile_origins(size: (u32, u32), tile: u32, stride: u32) -> Vec<(u32, u32)> {
    let axis = |len: u32| -> Vec<u32> {
        if len <= tile {
            return vec![0];
        }
        let mut v: Vec<u32> = (0..len - tile).step_by(stride.max(1) as usize).collect();
        v.push(len - tile);
        v.dedup();
        v
    };
    let (xs, ys) = (axis(size.0), axis(size.1));
    ys.iter()
        .flat_map(|&y| xs.iter().map(move |&x| (x, y)))
        .collect()
}

/// Blurs each (already grown) box in place with a Gaussian of sigma = a quarter of the box's
/// longer side, strong enough not to be reversible. Pixels outside the boxes are untouched.
///
/// `fast_blur` (three box passes) costs the same at any sigma. `imageops::blur` convolves
/// directly, at a cost proportional to sigma: one large box on a 12 MP photo took 48 s.
pub fn blur_faces(image: &mut RgbImage, faces: &[FaceBox]) {
    let size = image.dimensions();
    for face in faces {
        let r = face.to_pixels(size);
        if r.w == 0 || r.h == 0 {
            continue;
        }
        let sigma = (r.w.max(r.h) as f32 / 4.0).max(2.0);
        let region = imageops::crop_imm(image, r.x, r.y, r.w, r.h).to_image();
        let blurred = imageops::fast_blur(&region, sigma);
        imageops::replace(image, &blurred, i64::from(r.x), i64::from(r.y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fb(x: f32, y: f32, w: f32, h: f32) -> FaceBox {
        FaceBox {
            x,
            y,
            w,
            h,
            score: 0.9,
        }
    }

    #[test]
    fn union_merge_unites_overlaps_and_containment() {
        let merged = union_merge(
            vec![
                fb(0.1, 0.1, 0.2, 0.2),
                fb(0.15, 0.15, 0.2, 0.2),   // overlaps the first
                fb(0.12, 0.12, 0.05, 0.05), // inside the first
                fb(0.7, 0.7, 0.1, 0.1),     // apart
            ],
            0.2,
        );
        assert_eq!(merged.len(), 2);
        let big = merged.iter().find(|b| b.x < 0.5).unwrap();
        assert!((big.w - 0.25).abs() < 1e-6);
    }

    #[test]
    fn growth_is_clamped_to_the_image() {
        let g = fb(0.0, 0.4, 0.2, 0.2).grown(0.25);
        assert!((g.x - 0.0).abs() < 1e-6 && (g.w - 0.25).abs() < 1e-6);
        assert!((g.y - 0.35).abs() < 1e-6 && (g.h - 0.3).abs() < 1e-6);
    }

    #[test]
    fn tiles_cover_a_12_megapixel_photo() {
        let t = tile_origins((4032, 3024), 1280, 960);
        assert_eq!(t.len(), 12);
        assert!(t.contains(&(4032 - 1280, 3024 - 1280)));
    }

    #[test]
    fn decode_places_an_anchor_at_its_grid_cell() {
        // a 64×32 padded input at stride 32 has 2×1 anchors; anchor 1 is column 1
        let out = StrideOutputs {
            stride: 32,
            cls: &[0.0, 0.81],
            obj: &[0.0, 1.0],
            bbox: &[0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0],
        };
        let b = decode_stride(out, 64, 0.3);
        assert_eq!(b.len(), 1);
        assert!((b[0][4] - 0.9).abs() < 1e-6);
        assert!((b[0][0] - (48.0 - 16.0)).abs() < 1e-4); // centre 1.5·32 = 48, width 32
    }

    #[test]
    fn blur_changes_only_the_box() {
        let mut img = RgbImage::from_fn(64, 64, |x, y| {
            image::Rgb(if (x / 2 + y / 2) % 2 == 0 {
                [255; 3]
            } else {
                [0; 3]
            })
        });
        let before = img.clone();
        blur_faces(&mut img, &[fb(0.25, 0.25, 0.25, 0.25)]);
        assert_eq!(img.get_pixel(2, 2), before.get_pixel(2, 2));
        assert_ne!(img.get_pixel(24, 24), before.get_pixel(24, 24));
    }

    #[test]
    fn a_large_box_is_flattened() {
        // A face filling most of a photo: the checkerboard inside the box must be gone.
        let mut img = RgbImage::from_fn(1200, 900, |x, y| {
            image::Rgb(if (x / 4 + y / 4) % 2 == 0 {
                [255; 3]
            } else {
                [0; 3]
            })
        });
        blur_faces(&mut img, &[fb(0.1, 0.1, 0.8, 0.8)]);
        let inside: Vec<u8> = (300..600)
            .flat_map(|y| (300..900).map(move |x| (x, y)))
            .map(|(x, y)| img.get_pixel(x, y)[0])
            .collect();
        let (lo, hi) = (inside.iter().min().unwrap(), inside.iter().max().unwrap());
        assert!(hi - lo < 16, "the box still has contrast {lo}..{hi}");
    }
}
