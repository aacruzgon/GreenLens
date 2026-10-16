//! Pixel buffers and the codecs: format sniffing, decoding with EXIF orientation, resizing,
//! lossy `WebP` and JPEG encoding. Borrowed views (`ImageView<'a>`) for everything that only reads.

use std::io::Cursor;

use domain::ports::RgbView;
use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use image::{
    DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, RgbImage,
    codecs::jpeg::JpegEncoder, metadata::Orientation,
};

use crate::VisionError;

/// A borrowed RGB8 image, row-major, 3 bytes per pixel. `'a` is the borrow of the pixel buffer:
/// the view is `Copy`, costs nothing to pass around, and can't outlive the buffer it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageView<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
}

impl<'a> ImageView<'a> {
    /// Checks `pixels.len() == width × height × 3`.
    pub fn new(width: u32, height: u32, pixels: &'a [u8]) -> Option<Self> {
        (pixels.len() == width as usize * height as usize * 3).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Width.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The pixels.
    pub const fn pixels(&self) -> &'a [u8] {
        self.pixels
    }

    /// The pixel at `(x, y)`.
    pub fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2]]
    }

    /// The `domain` view of the same pixels, for the `SpotSegmenter` port.
    pub fn as_domain(&self) -> RgbView<'a> {
        // reason: both types check the same length rule.
        RgbView::new(self.width, self.height, self.pixels).unwrap_or_else(|| unreachable!())
    }
}

impl<'a> From<&'a RgbImage> for ImageView<'a> {
    fn from(image: &'a RgbImage) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            pixels: image.as_raw(),
        }
    }
}

impl<'a> From<RgbView<'a>> for ImageView<'a> {
    fn from(view: RgbView<'a>) -> Self {
        Self {
            width: view.width(),
            height: view.height(),
            pixels: view.pixels(),
        }
    }
}

/// The formats an upload may have ([contracts 1], images).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceFormat {
    /// JPEG.
    Jpeg,
    /// PNG.
    Png,
    /// `WebP`.
    WebP,
    /// HEIC/HEIF (decoded only with the `heic` feature).
    Heic,
}

/// Recognises the format from the first bytes, never from the file name or the part's content
/// type. HEIC is an ISO-BMFF `ftyp` box with a HEIF brand, which `image` doesn't sniff.
pub fn sniff(bytes: &[u8]) -> Option<SourceFormat> {
    if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(
            &bytes[8..12],
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1"
        )
    {
        return Some(SourceFormat::Heic);
    }
    match image::guess_format(bytes).ok()? {
        ImageFormat::Jpeg => Some(SourceFormat::Jpeg),
        ImageFormat::Png => Some(SourceFormat::Png),
        ImageFormat::WebP => Some(SourceFormat::WebP),
        _ => None,
    }
}

/// EXIF orientation 1–8. The rear camera produces only the rotations (1, 3, 6, 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExifOrientation(u8);

/// How the stored image is turned relative to the sensor frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rotation {
    /// EXIF 1.
    None,
    /// EXIF 6: a clockwise quarter turn.
    Cw90,
    /// EXIF 3.
    Half,
    /// EXIF 8: a counter-clockwise quarter turn.
    Ccw90,
}

impl ExifOrientation {
    /// No transform.
    pub const UPRIGHT: Self = Self(1);

    /// From the EXIF value; `None` outside 1–8.
    pub const fn new(value: u8) -> Option<Self> {
        if value >= 1 && value <= 8 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// The EXIF value.
    pub const fn value(self) -> u8 {
        self.0
    }

    /// The rotation, or `None` for a mirrored orientation (2, 4, 5, 7), whose depth is dropped
    /// with `invalid_meta` ([contracts 3]).
    pub const fn rotation(self) -> Option<Rotation> {
        match self.0 {
            1 => Some(Rotation::None),
            6 => Some(Rotation::Cw90),
            3 => Some(Rotation::Half),
            8 => Some(Rotation::Ccw90),
            _ => None,
        }
    }

    fn from_image(orientation: Orientation) -> Self {
        Self(orientation.to_exif())
    }
}

impl Rotation {
    /// Whether width and height swap.
    pub const fn swaps_axes(self) -> bool {
        matches!(self, Self::Cw90 | Self::Ccw90)
    }
}

/// Bounds on what an upload may decode to, checked before pixels are allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    /// The widest side accepted, pixels.
    pub max_side: u32,
    /// The most bytes the decoder may allocate.
    pub max_alloc: u64,
}

impl Default for DecodeLimits {
    /// 12,000 pixels a side, 512 MiB: a 48-megapixel phone photo fits.
    fn default() -> Self {
        Self {
            max_side: 12_000,
            max_alloc: 512 * 1024 * 1024,
        }
    }
}

/// A decoded upload: upright pixels, and what the sensor frame was.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// Upright RGB8 pixels (the EXIF orientation is applied; metadata is gone).
    pub image: RgbImage,
    /// The orientation the file carried (HEIC: always upright, libheif applies `irot` itself).
    pub orientation: ExifOrientation,
    /// The size before orientation, in the sensor frame the depth map shares.
    pub sensor_size: (u32, u32),
    /// The format.
    pub format: SourceFormat,
}

/// Why an upload didn't decode: `415 unsupported_media_type` either way.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// Not JPEG, PNG, `WebP` or (with the feature) HEIC.
    #[error("unsupported format")]
    Unsupported,
    /// Corrupt, truncated, or over the limits.
    #[error("undecodable: {0}")]
    Invalid(#[source] domain::ports::BoxError),
}

/// Decodes an upload, reads its EXIF orientation and bakes it into the pixels. Re-encoding
/// later drops every metadata chunk (EXIF, GPS, ICC), as [contracts 1] requires.
pub fn decode_upload(bytes: &[u8], limits: DecodeLimits) -> Result<Decoded, DecodeError> {
    match sniff(bytes) {
        None => Err(DecodeError::Unsupported),
        Some(SourceFormat::Heic) => decode_heic(bytes, limits),
        Some(format) => {
            let invalid = |e: image::ImageError| DecodeError::Invalid(Box::new(e));
            let mut reader = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| DecodeError::Invalid(Box::new(e)))?;
            let mut l = image::Limits::default();
            l.max_image_width = Some(limits.max_side);
            l.max_image_height = Some(limits.max_side);
            l.max_alloc = Some(limits.max_alloc);
            reader.limits(l);
            let mut decoder = reader.into_decoder().map_err(invalid)?;
            let orientation = decoder.orientation().map_err(invalid)?;
            let sensor_size = decoder.dimensions();
            let mut img = DynamicImage::from_decoder(decoder).map_err(invalid)?;
            img.apply_orientation(orientation);
            Ok(Decoded {
                image: img.into_rgb8(),
                orientation: ExifOrientation::from_image(orientation),
                sensor_size,
                format,
            })
        }
    }
}

#[cfg(feature = "heic")]
fn decode_heic(bytes: &[u8], limits: DecodeLimits) -> Result<Decoded, DecodeError> {
    use libheif_rs::{ColorSpace, DecodingOptions, HeifContext, LibHeif, RgbChroma};
    let invalid = |e: libheif_rs::HeifError| DecodeError::Invalid(Box::new(e));
    let lib = LibHeif::new();
    let mut ctx = HeifContext::read_from_bytes(bytes).map_err(invalid)?;
    let mut security = ctx.security_limits();
    security.set_max_image_size_pixels(u64::from(limits.max_side) * u64::from(limits.max_side));
    ctx.set_security_limits(&security).map_err(invalid)?;
    let handle = ctx.primary_image_handle().map_err(invalid)?;
    let mut options = DecodingOptions::new().ok_or(DecodeError::Unsupported)?;
    options.set_convert_hdr_to_8bit(true);
    let decoded = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), Some(options))
        .map_err(invalid)?;
    let plane = decoded
        .planes()
        .interleaved
        .ok_or(DecodeError::Unsupported)?;
    let (w, h, stride) = (plane.width, plane.height, plane.stride);
    let mut image = RgbImage::new(w, h);
    let row = w as usize * 3;
    for y in 0..h as usize {
        image.as_mut()[y * row..(y + 1) * row]
            .copy_from_slice(&plane.data[y * stride..y * stride + row]);
    }
    Ok(Decoded {
        image,
        orientation: ExifOrientation::UPRIGHT,
        sensor_size: (w, h),
        format: SourceFormat::Heic,
    })
}

#[cfg(not(feature = "heic"))]
fn decode_heic(_bytes: &[u8], _limits: DecodeLimits) -> Result<Decoded, DecodeError> {
    Err(DecodeError::Unsupported)
}

/// Decodes a stored image (`WebP` from the bucket, or any `image` format) to RGB8.
pub fn decode_stored(bytes: &[u8]) -> Result<RgbImage, VisionError> {
    Ok(image::load_from_memory(bytes)?.into_rgb8())
}

/// The size with the long side at most `long_side`, keeping the aspect ratio; never upscales.
pub fn fit_long_side(size: (u32, u32), long_side: u32) -> (u32, u32) {
    let (w, h) = size;
    let long = w.max(h);
    if long <= long_side || long == 0 {
        return (w, h);
    }
    let scale = f64::from(long_side) / f64::from(long);
    let scaled = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (scaled(w), scaled(h))
}

/// Resizes to exactly `width × height` with Lanczos3 (`fast_image_resize`, SIMD).
pub fn resize(view: ImageView<'_>, width: u32, height: u32) -> Result<RgbImage, VisionError> {
    if (view.width, view.height) == (width, height) {
        return RgbImage::from_raw(width, height, view.pixels.to_vec())
            .ok_or_else(|| VisionError::Resize("buffer size".to_owned()));
    }
    let err = |e: &dyn std::fmt::Display| VisionError::Resize(e.to_string());
    let src = ImageRef::new(view.width, view.height, view.pixels, PixelType::U8x3)
        .map_err(|e| err(&e))?;
    let mut dst = Image::new(width, height, PixelType::U8x3);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|e| err(&e))?;
    RgbImage::from_raw(width, height, dst.into_vec())
        .ok_or_else(|| VisionError::Resize("buffer size".to_owned()))
}

/// Resizes so the long side is at most `long_side`.
pub fn resize_long_side(view: ImageView<'_>, long_side: u32) -> Result<RgbImage, VisionError> {
    let (w, h) = fit_long_side((view.width, view.height), long_side);
    resize(view, w, h)
}

/// Lossy `WebP` through libwebp (`image` 0.25 writes only lossless `WebP`). The output has no
/// metadata chunks.
pub fn encode_webp(view: ImageView<'_>, quality: f32) -> Vec<u8> {
    webp::Encoder::from_rgb(view.pixels, view.width, view.height)
        .encode(quality)
        .to_vec()
}

/// JPEG, for the VLM's base64 data URLs (quality 90, [vision vlm 5]).
///
/// [vision vlm 5]: ../../../docs/vision/vlm.md#5-image-preparation-and-tokens
pub fn encode_jpeg(view: ImageView<'_>, quality: u8) -> Result<Vec<u8>, VisionError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).write_image(
        view.pixels,
        view.width,
        view.height,
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(out)
}

/// A rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    /// Left.
    pub x: u32,
    /// Top.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

impl PixelRect {
    /// A normalised box `[x, y, w, h]` on an image of `size`, rounded outwards and clamped.
    pub fn from_normalised(bbox: [f64; 4], size: (u32, u32)) -> Self {
        let (iw, ih) = (f64::from(size.0), f64::from(size.1));
        let x0 = (bbox[0] * iw).floor().clamp(0.0, iw);
        let y0 = (bbox[1] * ih).floor().clamp(0.0, ih);
        let x1 = ((bbox[0] + bbox[2]) * iw).ceil().clamp(0.0, iw);
        let y1 = ((bbox[1] + bbox[3]) * ih).ceil().clamp(0.0, ih);
        Self {
            x: x0 as u32,
            y: y0 as u32,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        }
    }
}

/// Copies a rectangle out of a view.
pub fn crop(view: ImageView<'_>, rect: PixelRect) -> RgbImage {
    let w = rect.w.min(view.width.saturating_sub(rect.x));
    let h = rect.h.min(view.height.saturating_sub(rect.y));
    RgbImage::from_fn(w, h, |x, y| image::Rgb(view.rgb(rect.x + x, rect.y + y)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 128])
        })
    }

    /// A big-endian EXIF block carrying only an orientation.
    fn exif(o: u16) -> Vec<u8> {
        let mut v = b"MM\0\x2a\0\0\0\x08".to_vec();
        v.extend([0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1]);
        v.extend(o.to_be_bytes());
        v.extend([0, 0, 0, 0, 0, 0]);
        v
    }

    #[test]
    fn decode_applies_exif_orientation_and_reports_the_sensor_size() {
        let src = gradient(40, 20);
        let mut jpeg = Vec::new();
        let mut enc = JpegEncoder::new_with_quality(&mut jpeg, 90);
        enc.set_exif_metadata(exif(6)).unwrap();
        enc.write_image(src.as_raw(), 40, 20, image::ExtendedColorType::Rgb8)
            .unwrap();
        let d = decode_upload(&jpeg, DecodeLimits::default()).unwrap();
        assert_eq!(d.image.dimensions(), (20, 40));
        assert_eq!(d.sensor_size, (40, 20));
        assert_eq!(d.orientation.rotation(), Some(Rotation::Cw90));
        let webp = encode_webp(ImageView::from(&d.image), 82.0);
        assert!(!webp.windows(4).any(|w| w == b"EXIF"));
        assert_eq!(sniff(&webp), Some(SourceFormat::WebP));
    }

    #[test]
    fn heic_is_sniffed_from_its_brand() {
        let mut ftyp = vec![0, 0, 0, 24];
        ftyp.extend(b"ftypheic");
        ftyp.extend([0u8; 12]);
        assert_eq!(sniff(&ftyp), Some(SourceFormat::Heic));
        assert!(sniff(b"GIF89a....").is_none());
    }

    #[test]
    fn fit_long_side_never_upscales() {
        assert_eq!(fit_long_side((4032, 3024), 2048), (2048, 1536));
        assert_eq!(fit_long_side((800, 600), 2048), (800, 600));
        let small = resize_long_side(ImageView::from(&gradient(400, 300)), 200).unwrap();
        assert_eq!(small.dimensions(), (200, 150));
    }

    #[test]
    fn normalised_boxes_round_outwards() {
        let r = PixelRect::from_normalised([0.101, 0.25, 0.3, 0.5], (100, 50));
        assert_eq!((r.x, r.y, r.w, r.h), (10, 12, 31, 26));
    }
}
