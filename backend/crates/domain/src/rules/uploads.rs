//! Upload rules ([contracts 3], `POST /scans/{id}/images` and depth): where a photo goes, kind
//! limits, required shots, size limits and the depth checks that need no pixels.

use crate::{
    error::{DomainError, InvalidStateDetails},
    rules::questions::QuestionState,
    scan::{DepthProblem, ImageKind, ScanStatus},
};

/// 12 MB, read as MiB like the API's body limit ([contracts 2], `upload_too_large`).
pub const IMAGE_MAX_BYTES: u64 = 12 * 1024 * 1024;
/// 4 MB per depth map ([contracts 3], depth).
pub const DEPTH_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// JPEG, PNG, WebP or HEIC ([contracts 1], images); `details.accepted` of `415`.
pub const ACCEPTED_MEDIA_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/heic"];
/// Submit needs these ([contracts 2], `missing_required_shot`).
pub const REQUIRED_SHOTS: [ImageKind; 1] = [ImageKind::WholePlant];
/// A depth map's aspect ratio must match the photo's within 1%.
pub const DEPTH_ASPECT_TOLERANCE: f64 = 0.01;
/// Fewer than 20% of the pixels with depth is `too_sparse`.
pub const DEPTH_MIN_COVERAGE: f64 = 0.20;
/// A photo with depth is stored with a long side of at least 2048 pixels, or its own size when
/// smaller, because spot sizes are measured on the stored image.
pub const DEPTH_PHOTO_MIN_LONG_SIDE: u32 = 2048;

impl ImageKind {
    /// The most of this kind a scan holds before submit (contracts 3, `ImageKind`).
    pub const fn max_before_submit(self) -> u32 {
        match self {
            Self::LeafCloseup => 2,
            Self::WholePlant
            | Self::LeafUnderside
            | Self::SoilSurface
            | Self::FlowerFruit
            | Self::SqueezeTest
            | Self::JarTest => 1,
        }
    }

    /// Depth is kept only for plant and soil kinds; sent with a guided check it is dropped
    /// with `not_for_this_kind`.
    pub const fn takes_depth(self) -> bool {
        self.is_plant_or_soil()
    }
}

/// Where an accepted upload goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadRoute {
    /// A plant or soil shot of the coming run (the scan is `pending`).
    RunShot,
    /// A new guided check with its own `guided` job.
    GuidedCheck,
    /// A photo for an open `photo_request` question: it becomes `answering`.
    QuestionPhoto,
}

/// Decides where an upload goes, or refuses it.
///
/// - With `question_id`: the question must be `open`, whatever the scan's status; the photo is
///   a plant or soil kind and doesn't count toward kind limits.
/// - A `squeeze_test` or `jar_test`: any status except `failed`.
/// - Any other kind: only while `pending`.
pub fn route_upload(
    kind: ImageKind,
    scan: ScanStatus,
    question: Option<QuestionState>,
) -> Result<UploadRoute, DomainError> {
    if let Some(state) = question {
        if !kind.is_plant_or_soil() {
            return Err(DomainError::invalid(
                "kind",
                "a question photo is a plant or soil shot",
            ));
        }
        return if state == QuestionState::Open {
            Ok(UploadRoute::QuestionPhoto)
        } else {
            Err(DomainError::InvalidState(InvalidStateDetails {
                status: Some(state.as_str().to_owned()),
                allowed: Some(vec![QuestionState::Open.as_str().to_owned()]),
                ..InvalidStateDetails::default()
            }))
        };
    }
    let allowed = if kind.guided().is_some() {
        scan != ScanStatus::Failed
    } else {
        scan == ScanStatus::Pending
    };
    if !allowed {
        return Err(DomainError::InvalidState(InvalidStateDetails {
            status: Some(scan.as_str().to_owned()),
            ..InvalidStateDetails::default()
        }));
    }
    Ok(if kind.guided().is_some() {
        UploadRoute::GuidedCheck
    } else {
        UploadRoute::RunShot
    })
}

/// Checks the kind limit for an upload that counts toward it. `existing` counts the scan's
/// images of that kind that aren't question photos. After submit, guided kinds have no limit.
pub fn check_kind_limit(
    kind: ImageKind,
    scan: ScanStatus,
    existing: u32,
) -> Result<(), DomainError> {
    let limited = scan == ScanStatus::Pending || kind.is_plant_or_soil();
    let max = kind.max_before_submit();
    if limited && existing >= max {
        Err(DomainError::TooManyImages { kind, max })
    } else {
        Ok(())
    }
}

/// Checks that the run's shots include every required kind.
pub fn check_required_shots(present: &[ImageKind]) -> Result<(), DomainError> {
    let missing: Vec<ImageKind> = REQUIRED_SHOTS
        .iter()
        .copied()
        .filter(|k| !present.contains(k))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(DomainError::MissingRequiredShot { missing })
    }
}

/// Checks the media type against [`ACCEPTED_MEDIA_TYPES`].
pub fn accepted_media_type(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or_default().trim();
    ACCEPTED_MEDIA_TYPES
        .iter()
        .any(|t| t.eq_ignore_ascii_case(essence))
}

/// The depth checks that need only sizes and counts. `orientation` is the photo's EXIF
/// orientation (1–8); the mirrored ones (2, 4, 5, 7) can't come from the rear camera.
pub fn check_depth(
    kind: ImageKind,
    photo: (u32, u32),
    depth: (u32, u32),
    depth_bytes: u64,
    orientation: u8,
) -> Result<(), DepthProblem> {
    if !kind.takes_depth() {
        return Err(DepthProblem::NotForThisKind);
    }
    if depth_bytes > DEPTH_MAX_BYTES {
        return Err(DepthProblem::TooLarge);
    }
    if matches!(orientation, 2 | 4 | 5 | 7) || !(1..=8).contains(&orientation) {
        return Err(DepthProblem::InvalidMeta);
    }
    if photo.0 == 0 || photo.1 == 0 || depth.0 == 0 || depth.1 == 0 {
        return Err(DepthProblem::InvalidMeta);
    }
    let photo_aspect = f64::from(photo.0) / f64::from(photo.1);
    let depth_aspect = f64::from(depth.0) / f64::from(depth.1);
    if (photo_aspect / depth_aspect - 1.0).abs() > DEPTH_ASPECT_TOLERANCE {
        return Err(DepthProblem::AspectMismatch);
    }
    Ok(())
}

/// `too_sparse` when fewer than 20% of the depth pixels are non-zero.
pub fn check_depth_coverage(non_zero: u64, total: u64) -> Result<(), DepthProblem> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel counts are far below 2^52"
    )]
    let share = if total == 0 {
        0.0
    } else {
        non_zero as f64 / total as f64
    };
    if share < DEPTH_MIN_COVERAGE {
        Err(DepthProblem::TooSparse)
    } else {
        Ok(())
    }
}

/// The long side to store a photo at: at least 2048 pixels with depth (or its own size when
/// smaller), and `default_long_side` without.
pub fn stored_long_side(original_long_side: u32, has_depth: bool, default_long_side: u32) -> u32 {
    let target = if has_depth {
        DEPTH_PHOTO_MIN_LONG_SIDE.max(default_long_side)
    } else {
        default_long_side
    };
    original_long_side.min(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes() {
        use ImageKind as K;
        use ScanStatus as S;
        assert_eq!(
            route_upload(K::WholePlant, S::Pending, None).unwrap(),
            UploadRoute::RunShot
        );
        assert!(route_upload(K::WholePlant, S::Queued, None).is_err());
        assert_eq!(
            route_upload(K::JarTest, S::Planned, None).unwrap(),
            UploadRoute::GuidedCheck
        );
        assert!(route_upload(K::JarTest, S::Failed, None).is_err());
        assert_eq!(
            route_upload(K::LeafUnderside, S::Planned, Some(QuestionState::Open)).unwrap(),
            UploadRoute::QuestionPhoto
        );
        assert!(
            route_upload(K::LeafUnderside, S::Planned, Some(QuestionState::Answering)).is_err()
        );
        assert!(route_upload(K::JarTest, S::Planned, Some(QuestionState::Open)).is_err());
    }

    #[test]
    fn kind_limits() {
        use ImageKind as K;
        use ScanStatus as S;
        assert!(check_kind_limit(K::LeafCloseup, S::Pending, 1).is_ok());
        assert_eq!(
            check_kind_limit(K::LeafCloseup, S::Pending, 2).unwrap_err(),
            DomainError::TooManyImages {
                kind: K::LeafCloseup,
                max: 2
            }
        );
        assert!(check_kind_limit(K::JarTest, S::Pending, 1).is_err());
        assert!(check_kind_limit(K::JarTest, S::Planned, 5).is_ok());
    }

    #[test]
    fn required_shots_and_media() {
        assert!(check_required_shots(&[ImageKind::WholePlant, ImageKind::LeafCloseup]).is_ok());
        assert!(check_required_shots(&[ImageKind::LeafCloseup]).is_err());
        assert!(accepted_media_type("image/HEIC"));
        assert!(accepted_media_type("image/jpeg; charset=binary"));
        assert!(!accepted_media_type("image/gif"));
    }

    #[test]
    fn depth_checks() {
        assert!(check_depth(ImageKind::LeafCloseup, (4032, 3024), (320, 240), 100, 6).is_ok());
        assert_eq!(
            check_depth(ImageKind::LeafCloseup, (4032, 3024), (320, 180), 100, 1),
            Err(DepthProblem::AspectMismatch)
        );
        assert_eq!(
            check_depth(ImageKind::JarTest, (4032, 3024), (320, 240), 100, 1),
            Err(DepthProblem::NotForThisKind)
        );
        assert_eq!(
            check_depth(ImageKind::WholePlant, (4032, 3024), (320, 240), 100, 2),
            Err(DepthProblem::InvalidMeta)
        );
        assert_eq!(check_depth_coverage(19, 100), Err(DepthProblem::TooSparse));
        assert!(check_depth_coverage(20, 100).is_ok());
        assert_eq!(stored_long_side(4032, true, 1600), 2048);
        assert_eq!(stored_long_side(1500, true, 1600), 1500);
        assert_eq!(stored_long_side(4032, false, 1600), 1600);
    }
}
