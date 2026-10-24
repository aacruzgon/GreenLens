//! The `LiDAR` depth path of an upload ([contracts 3], depth; [architecture 6.7], upload): validate,
//! rotate the depth and confidence maps with the photo's EXIF orientation, transform the
//! intrinsics, distortion centre and gravity to match, scale the intrinsics to the stored image,
//! zero the depth inside every face box, and encode 16-bit and 8-bit PNGs.
//!
//! Depth problems never fail the upload: every function here returns a [`DepthProblem`], and the
//! photo is stored without its depth.
//!
//! [contracts 3]: ../../../docs/architecture/contracts.md#post-scansidimages
//! [architecture 6.7]: ../../../docs/architecture/README.md#67-depth-and-measurement-lidar

use std::io::Cursor;

use bytes::Bytes;
use domain::{
    rules::uploads::{check_depth, check_depth_coverage},
    scan::{DepthMeta, DepthProblem, ImageKind, Intrinsics},
};

use crate::{imaging::Rotation, upload::faces::FaceBox};

/// A depth map's three levels of confidence (`depth_confidence`: 0 low, 1 medium, 2 high).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DepthConfidence {
    /// 0.
    Low = 0,
    /// 1.
    Medium = 1,
    /// 2.
    High = 2,
}

/// An owned depth map in millimetres, row-major; 0 is "no depth".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthMapBuf {
    width: u32,
    height: u32,
    mm: Vec<u16>,
}

/// A borrowed depth map. `'a` is the borrow of the millimetre buffer, so the measurement stage
/// reads one decoded map from many functions without copying it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthMap<'a> {
    width: u32,
    height: u32,
    mm: &'a [u16],
}

impl DepthMapBuf {
    /// From samples; `None` when the length doesn't match.
    pub fn from_raw(width: u32, height: u32, mm: Vec<u16>) -> Option<Self> {
        (mm.len() == width as usize * height as usize).then_some(Self { width, height, mm })
    }

    /// The borrowed view.
    pub fn view(&self) -> DepthMap<'_> {
        DepthMap {
            width: self.width,
            height: self.height,
            mm: &self.mm,
        }
    }
}

impl<'a> DepthMap<'a> {
    /// From samples; `None` when the length doesn't match.
    pub fn new(width: u32, height: u32, mm: &'a [u16]) -> Option<Self> {
        (mm.len() == width as usize * height as usize).then_some(Self { width, height, mm })
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The samples.
    pub const fn samples(&self) -> &'a [u16] {
        self.mm
    }

    /// Depth at `(x, y)` in metres; `None` for "no depth".
    pub fn metres(&self, x: u32, y: u32) -> Option<f64> {
        let v = self.mm[y as usize * self.width as usize + x as usize];
        (v != 0).then(|| f64::from(v) / 1000.0)
    }

    /// The share of pixels with depth.
    pub fn coverage(&self) -> f64 {
        let n = self.mm.iter().filter(|&&v| v != 0).count();
        n as f64 / self.mm.len().max(1) as f64
    }
}

/// An owned confidence map (levels 0–2), the same size as its depth map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfidenceMapBuf {
    width: u32,
    height: u32,
    levels: Vec<u8>,
}

/// A borrowed confidence map; `'a` borrows the level buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfidenceMap<'a> {
    width: u32,
    height: u32,
    levels: &'a [u8],
}

impl ConfidenceMapBuf {
    /// The borrowed view.
    pub fn view(&self) -> ConfidenceMap<'_> {
        ConfidenceMap {
            width: self.width,
            height: self.height,
            levels: &self.levels,
        }
    }
}

impl ConfidenceMap<'_> {
    /// The level at `(x, y)`; values above 2 read as high.
    pub fn level(&self, x: u32, y: u32) -> DepthConfidence {
        match self.levels[y as usize * self.width as usize + x as usize] {
            0 => DepthConfidence::Low,
            1 => DepthConfidence::Medium,
            _ => DepthConfidence::High,
        }
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }
}

/// Decodes the `depth` part: exactly a 16-bit grayscale PNG (no silent conversion), big-endian
/// samples, at most 64 MiB of decoder memory.
pub fn decode_depth_png(bytes: &[u8]) -> Result<DepthMapBuf, DepthProblem> {
    let (w, h, raw) = decode_png(bytes, png::BitDepth::Sixteen)?;
    let mm = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes(*c))
        .collect();
    DepthMapBuf::from_raw(w, h, mm).ok_or(DepthProblem::UnsupportedFormat)
}

/// Decodes the `depth_confidence` part: an 8-bit grayscale PNG of the depth map's size.
pub fn decode_confidence_png(
    bytes: &[u8],
    size: (u32, u32),
) -> Result<ConfidenceMapBuf, DepthProblem> {
    let (w, h, levels) = decode_png(bytes, png::BitDepth::Eight)?;
    if (w, h) != size {
        return Err(DepthProblem::InvalidMeta);
    }
    Ok(ConfidenceMapBuf {
        width: w,
        height: h,
        levels,
    })
}

fn decode_png(bytes: &[u8], depth: png::BitDepth) -> Result<(u32, u32, Vec<u8>), DepthProblem> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 64 * 1024 * 1024,
        },
    );
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|_| DepthProblem::UnsupportedFormat)?;
    let info = reader.info();
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != depth {
        return Err(DepthProblem::UnsupportedFormat);
    }
    let (w, h) = (info.width, info.height);
    let size = reader
        .output_buffer_size()
        .ok_or(DepthProblem::UnsupportedFormat)?;
    let mut buf = vec![0u8; size];
    reader
        .next_frame(&mut buf)
        .map_err(|_| DepthProblem::UnsupportedFormat)?;
    Ok((w, h, buf))
}

/// Encodes a depth map as a 16-bit grayscale PNG in millimetres (big-endian samples).
pub fn encode_depth_png(depth: DepthMap<'_>) -> Result<Vec<u8>, png::EncodingError> {
    let bytes: Vec<u8> = depth.mm.iter().flat_map(|s| s.to_be_bytes()).collect();
    encode_png(depth.width, depth.height, png::BitDepth::Sixteen, &bytes)
}

/// Encodes a confidence map as an 8-bit grayscale PNG.
pub fn encode_confidence_png(conf: ConfidenceMap<'_>) -> Result<Vec<u8>, png::EncodingError> {
    encode_png(conf.width, conf.height, png::BitDepth::Eight, conf.levels)
}

pub(crate) fn encode_png(
    w: u32,
    h: u32,
    depth: png::BitDepth,
    data: &[u8],
) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(depth);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(data)?;
    writer.finish()?;
    Ok(out)
}

/// Rotates a row-major grid by exact pixel moves (no interpolation, so "no depth" stays 0).
/// Returns the new width, height and samples. Generic over the sample type: `u16` depth and
/// `u8` confidence share one implementation.
pub fn rotate_grid<T: Copy>(w: u32, h: u32, data: &[T], rotation: Rotation) -> (u32, u32, Vec<T>) {
    let (w_us, h_us) = (w as usize, h as usize);
    match rotation {
        Rotation::None => (w, h, data.to_vec()),
        Rotation::Half => (w, h, data.iter().rev().copied().collect()),
        Rotation::Cw90 => {
            // new (x', y') = (h - 1 - y, x): new width h, height w
            let mut out = Vec::with_capacity(data.len());
            for ny in 0..w_us {
                for nx in 0..h_us {
                    out.push(data[(h_us - 1 - nx) * w_us + ny]);
                }
            }
            (h, w, out)
        }
        Rotation::Ccw90 => {
            // new (x', y') = (y, w - 1 - x)
            let mut out = Vec::with_capacity(data.len());
            for ny in 0..w_us {
                for nx in 0..h_us {
                    out.push(data[nx * w_us + (w_us - 1 - ny)]);
                }
            }
            (h, w, out)
        }
    }
}

/// Turns a point of a `ref_size` frame with the pixels, in continuous pixel coordinates (pixel
/// `i` covers `[i, i + 1)`): clockwise `(u, v) → (H − v, u)`, half `(W − u, H − v)`,
/// counter-clockwise `(v, W − u)`. Used for the principal point and the distortion centre.
#[allow(
    clippy::many_single_char_names,
    reason = "u, v, w, h: the pixel-coordinate notation of the formulas"
)]
pub fn rotate_point(p: [f64; 2], ref_size: (f64, f64), rotation: Rotation) -> [f64; 2] {
    let (w, h) = ref_size;
    let [u, v] = p;
    match rotation {
        Rotation::None => [u, v],
        Rotation::Cw90 => [h - v, u],
        Rotation::Half => [w - u, h - v],
        Rotation::Ccw90 => [v, w - u],
    }
}

/// Turns gravity (camera coordinates: x right, y down, z forward) with the pixels: clockwise
/// `(x, y) → (−y, x)`, half `(−x, −y)`, counter-clockwise `(y, −x)`; z doesn't change.
pub fn rotate_gravity(g: [f64; 3], rotation: Rotation) -> [f64; 3] {
    let [x, y, z] = g;
    match rotation {
        Rotation::None => [x, y, z],
        Rotation::Cw90 => [-y, x, z],
        Rotation::Half => [-x, -y, z],
        Rotation::Ccw90 => [y, -x, z],
    }
}

/// Turns intrinsics with the pixels: a quarter turn swaps `fx` with `fy` and the reference
/// width with its height, and the principal point turns like any point.
pub fn rotate_intrinsics(k: Intrinsics, rotation: Rotation) -> Intrinsics {
    let size = (f64::from(k.ref_width), f64::from(k.ref_height));
    let [cx, cy] = rotate_point([k.cx, k.cy], size, rotation);
    if rotation.swaps_axes() {
        Intrinsics {
            fx: k.fy,
            fy: k.fx,
            cx,
            cy,
            ref_width: k.ref_height,
            ref_height: k.ref_width,
        }
    } else {
        Intrinsics { cx, cy, ..k }
    }
}

/// Scales intrinsics to another size of the same field of view: `fx` and `cx` by the width
/// ratio, `fy` and `cy` by the height ratio.
pub fn scale_intrinsics(k: Intrinsics, size: (u32, u32)) -> Intrinsics {
    let sx = f64::from(size.0) / f64::from(k.ref_width);
    let sy = f64::from(size.1) / f64::from(k.ref_height);
    Intrinsics {
        fx: k.fx * sx,
        fy: k.fy * sy,
        cx: k.cx * sx,
        cy: k.cy * sy,
        ref_width: size.0,
        ref_height: size.1,
    }
}

/// The stored `DepthMeta`: rotated with the photo, then scaled to the stored image's size. The
/// distortion centre is rotated and scaled the same way; its lookup table is in normalised
/// radius and doesn't change.
pub fn transform_meta(meta: &DepthMeta, rotation: Rotation, stored: (u32, u32)) -> DepthMeta {
    let rotated = rotate_intrinsics(meta.intrinsics, rotation);
    let scaled = scale_intrinsics(rotated, stored);
    let distortion = meta.distortion.as_ref().map(|d| {
        let ref_size = (
            f64::from(meta.intrinsics.ref_width),
            f64::from(meta.intrinsics.ref_height),
        );
        let [x, y] = rotate_point(d.center, ref_size, rotation);
        let sx = f64::from(stored.0) / f64::from(rotated.ref_width);
        let sy = f64::from(stored.1) / f64::from(rotated.ref_height);
        domain::scan::Distortion {
            center: [x * sx, y * sy],
            inverse_lookup: d.inverse_lookup.clone(),
        }
    });
    DepthMeta {
        source: meta.source,
        intrinsics: scaled,
        distortion,
        gravity: meta.gravity.map(|g| rotate_gravity(g, rotation)),
        device_model: meta.device_model.clone(),
        filtered: meta.filtered,
    }
}

/// Checks what the meta must hold before anything is rotated: positive focal lengths and
/// reference size, a reference aspect matching the photo's (within 1%), a principal point inside
/// the frame, and gravity of unit length (±5%) when sent.
pub fn validate_meta(meta: &DepthMeta, sensor_size: (u32, u32)) -> Result<(), DepthProblem> {
    let k = &meta.intrinsics;
    let finite = [k.fx, k.fy, k.cx, k.cy].iter().all(|v| v.is_finite());
    if !finite || k.fx <= 0.0 || k.fy <= 0.0 || k.ref_width == 0 || k.ref_height == 0 {
        return Err(DepthProblem::InvalidMeta);
    }
    if k.cx < 0.0 || k.cy < 0.0 || k.cx > f64::from(k.ref_width) || k.cy > f64::from(k.ref_height) {
        return Err(DepthProblem::InvalidMeta);
    }
    let ref_aspect = f64::from(k.ref_width) / f64::from(k.ref_height);
    let photo_aspect = f64::from(sensor_size.0) / f64::from(sensor_size.1.max(1));
    if (ref_aspect / photo_aspect - 1.0).abs() > 0.01 {
        return Err(DepthProblem::InvalidMeta);
    }
    if let Some(g) = meta.gravity {
        let norm = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
        if !norm.is_finite() || (norm - 1.0).abs() > 0.05 {
            return Err(DepthProblem::InvalidMeta);
        }
    }
    Ok(())
}

/// Zeroes depth, and confidence when present, inside each face box grown by `grow` (0.10) and
/// rounded outwards on the map's grid, so no edge of a face survives ([contracts 3]). The boxes
/// are the already-grown blur boxes, normalised to the upright image the map now shares.
pub fn zero_faces(
    depth: &mut DepthMapBuf,
    mut confidence: Option<&mut ConfidenceMapBuf>,
    faces: &[FaceBox],
    grow: f32,
) {
    let size = (depth.width, depth.height);
    for face in faces {
        let r = face.grown(grow).to_pixels(size);
        for y in r.y..(r.y + r.h).min(size.1) {
            for x in r.x..(r.x + r.w).min(size.0) {
                let i = y as usize * size.0 as usize + x as usize;
                depth.mm[i] = 0;
                if let Some(c) = confidence.as_deref_mut() {
                    c.levels[i] = 0;
                }
            }
        }
    }
}

/// The three depth parts of an upload, borrowed from the request body for the duration of the
/// blocking call.
#[derive(Debug, Clone, Copy)]
pub struct DepthInput<'a> {
    /// The `depth` part.
    pub depth: &'a [u8],
    /// The `depth_confidence` part, when sent.
    pub confidence: Option<&'a [u8]>,
    /// The `depth_meta` part, raw JSON.
    pub meta: &'a [u8],
}

/// What the upload knows about the photo when it processes the depth.
#[derive(Debug, Clone, Copy)]
pub struct DepthContext<'f> {
    /// The shot's kind.
    pub kind: ImageKind,
    /// The photo's EXIF orientation value (1–8).
    pub orientation: u8,
    /// The photo's size before orientation, the frame the depth map is in.
    pub sensor_size: (u32, u32),
    /// The stored image's size.
    pub stored_size: (u32, u32),
    /// The grown blur boxes. `'f` borrows the upload's face list.
    pub faces: &'f [FaceBox],
    /// How much more each blur box grows on the depth map (0.10).
    pub face_growth: f32,
}

/// Depth ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedDepth {
    /// 16-bit PNG, millimetres, upright.
    pub depth_png: Bytes,
    /// 8-bit PNG, when the capture path sent one.
    pub confidence_png: Option<Bytes>,
    /// Rotated, and scaled to the stored image (`ref_width`/`ref_height` = its size).
    pub meta: DepthMeta,
    /// The depth map's width, upright.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

impl ProcessedDepth {
    /// `ScanImage.depth.has_gravity`.
    pub fn has_gravity(&self) -> bool {
        self.meta.gravity.is_some()
    }
}

/// The whole depth path, in the contract's order. Each failure maps to its `problem` code.
pub fn process_depth(
    input: &DepthInput<'_>,
    ctx: &DepthContext<'_>,
) -> Result<ProcessedDepth, DepthProblem> {
    // The checks that need no pixels first, so a guided shot or an oversized part never decodes.
    if !ctx.kind.takes_depth() {
        return Err(DepthProblem::NotForThisKind);
    }
    if input.depth.len() as u64 > domain::rules::uploads::DEPTH_MAX_BYTES {
        return Err(DepthProblem::TooLarge);
    }
    let mut depth = decode_depth_png(input.depth)?;
    check_depth(
        ctx.kind,
        ctx.sensor_size,
        (depth.width, depth.height),
        input.depth.len() as u64,
        ctx.orientation,
    )?;
    let non_zero = depth.mm.iter().filter(|&&v| v != 0).count() as u64;
    check_depth_coverage(non_zero, depth.mm.len() as u64)?;
    let meta: DepthMeta =
        serde_json::from_slice(input.meta).map_err(|_| DepthProblem::InvalidMeta)?;
    validate_meta(&meta, ctx.sensor_size)?;
    let mut confidence = input
        .confidence
        .map(|c| decode_confidence_png(c, (depth.width, depth.height)))
        .transpose()?;
    let rotation = crate::imaging::ExifOrientation::new(ctx.orientation)
        .and_then(crate::imaging::ExifOrientation::rotation)
        .ok_or(DepthProblem::InvalidMeta)?;
    let (w, h, mm) = rotate_grid(depth.width, depth.height, &depth.mm, rotation);
    depth = DepthMapBuf {
        width: w,
        height: h,
        mm,
    };
    if let Some(c) = confidence.as_mut() {
        let (cw, ch, levels) = rotate_grid(c.width, c.height, &c.levels, rotation);
        *c = ConfidenceMapBuf {
            width: cw,
            height: ch,
            levels,
        };
    }
    zero_faces(&mut depth, confidence.as_mut(), ctx.faces, ctx.face_growth);
    let meta = transform_meta(&meta, rotation, ctx.stored_size);
    let depth_png = encode_depth_png(depth.view()).map_err(|_| DepthProblem::UnsupportedFormat)?;
    let confidence_png = confidence
        .as_ref()
        .map(|c| encode_confidence_png(c.view()))
        .transpose()
        .map_err(|_| DepthProblem::UnsupportedFormat)?;
    Ok(ProcessedDepth {
        depth_png: Bytes::from(depth_png),
        confidence_png: confidence_png.map(Bytes::from),
        meta,
        width: depth.width,
        height: depth.height,
    })
}

#[cfg(test)]
mod tests {
    use domain::scan::DepthSource;

    use super::*;

    fn k() -> Intrinsics {
        Intrinsics {
            fx: 1500.0,
            fy: 1400.0,
            cx: 1000.0,
            cy: 750.0,
            ref_width: 2000,
            ref_height: 1500,
        }
    }

    #[test]
    fn a_quarter_turn_swaps_focal_lengths_and_moves_the_principal_point() {
        let r = rotate_intrinsics(k(), Rotation::Cw90);
        assert_eq!(
            (r.fx, r.fy, r.ref_width, r.ref_height),
            (1400.0, 1500.0, 1500, 2000)
        );
        assert_eq!((r.cx, r.cy), (750.0, 1000.0));
        assert_eq!(rotate_intrinsics(r, Rotation::Ccw90), k());
        let s = scale_intrinsics(k(), (1000, 750));
        assert_eq!((s.fx, s.cx, s.cy), (750.0, 500.0, 375.0));
    }

    #[test]
    fn gravity_turns_with_the_pixels() {
        // pointing up in the sensor frame becomes pointing right after a clockwise turn
        assert_eq!(
            rotate_gravity([0.0, -1.0, 0.0], Rotation::Cw90),
            [1.0, 0.0, 0.0]
        );
        assert_eq!(
            rotate_gravity([0.0, 1.0, 0.0], Rotation::Half),
            [0.0, -1.0, 0.0]
        );
    }

    #[test]
    fn grid_rotation_matches_image_rotation() {
        // 3 wide, 2 tall: 0 1 2 / 3 4 5
        let data: Vec<u16> = (0..6).collect();
        let (w, h, cw) = rotate_grid(3, 2, &data, Rotation::Cw90);
        assert_eq!((w, h), (2, 3));
        assert_eq!(cw, vec![3, 0, 4, 1, 5, 2]); // the old top-left lands top-right
        let (_, _, ccw) = rotate_grid(3, 2, &data, Rotation::Ccw90);
        assert_eq!(ccw, vec![2, 5, 1, 4, 0, 3]);
        let (_, _, back) = rotate_grid(2, 3, &cw, Rotation::Ccw90);
        assert_eq!(back, data);
    }

    #[test]
    fn png_round_trip_and_format_checks() {
        let d = DepthMapBuf::from_raw(4, 3, (0..12).map(|v| v * 300).collect()).unwrap();
        let png = encode_depth_png(d.view()).unwrap();
        assert_eq!(decode_depth_png(&png).unwrap(), d);
        let c = ConfidenceMapBuf {
            width: 4,
            height: 3,
            levels: vec![2; 12],
        };
        let cpng = encode_confidence_png(c.view()).unwrap();
        assert_eq!(
            decode_depth_png(&cpng),
            Err(DepthProblem::UnsupportedFormat)
        );
        assert_eq!(decode_confidence_png(&cpng, (4, 3)).unwrap(), c);
    }

    #[test]
    fn faces_are_zeroed_with_the_extra_growth() {
        let mut d = DepthMapBuf::from_raw(10, 10, vec![500; 100]).unwrap();
        let face = FaceBox {
            x: 0.4,
            y: 0.4,
            w: 0.2,
            h: 0.2,
            score: 0.9,
        };
        zero_faces(&mut d, None, &[face], 0.10);
        // grown by 10%: 0.38..0.62, rounded outwards to pixels 3..=6
        assert_eq!(d.view().metres(3, 3), None);
        assert_eq!(d.view().metres(6, 6), None);
        assert_eq!(d.view().metres(2, 2), Some(0.5));
        assert_eq!(d.view().metres(7, 7), Some(0.5));
    }

    #[test]
    fn process_depth_maps_each_failure_to_its_problem() {
        let depth =
            encode_depth_png(DepthMapBuf::from_raw(4, 3, vec![400; 12]).unwrap().view()).unwrap();
        let meta = DepthMeta {
            source: DepthSource::Lidar,
            intrinsics: Intrinsics {
                fx: 30.0,
                fy: 30.0,
                cx: 20.0,
                cy: 15.0,
                ref_width: 40,
                ref_height: 30,
            },
            distortion: None,
            gravity: Some([0.0, 1.0, 0.0]),
            device_model: "iPhone16,1".to_owned(),
            filtered: false,
        };
        let meta_json = serde_json::to_vec(&meta).unwrap();
        let input = DepthInput {
            depth: &depth,
            confidence: None,
            meta: &meta_json,
        };
        let ctx = DepthContext {
            kind: ImageKind::LeafCloseup,
            orientation: 6,
            sensor_size: (40, 30),
            stored_size: (30, 40),
            faces: &[],
            face_growth: 0.10,
        };
        let done = process_depth(&input, &ctx).unwrap();
        assert_eq!((done.width, done.height), (3, 4));
        assert_eq!(done.meta.intrinsics.ref_width, 30);
        assert_eq!(done.meta.gravity, Some([-1.0, 0.0, 0.0]));
        let mirrored = DepthContext {
            orientation: 2,
            ..ctx
        };
        assert_eq!(
            process_depth(&input, &mirrored),
            Err(DepthProblem::InvalidMeta)
        );
        let guided = DepthContext {
            kind: ImageKind::JarTest,
            ..ctx
        };
        assert_eq!(
            process_depth(&input, &guided),
            Err(DepthProblem::NotForThisKind)
        );
        let bad = DepthInput {
            meta: b"{}",
            ..input
        };
        assert_eq!(process_depth(&bad, &ctx), Err(DepthProblem::InvalidMeta));
    }
}
