//! Uploads ([contracts 3], `POST /scans/{id}/images`; [contracts 8], `POST /conversations/{id}/images`;
//! [storage 4], writes).
//!
//! 1. Stream the multipart parts with a cap per part: the photo at 12 MiB (`413
//!    upload_too_large`), the depth and confidence maps at 4 MiB each (over it, the depth is
//!    dropped with `too_large` and the photo kept), `depth_meta` at 64 KiB.
//! 2. Fingerprint the fields and file bytes, and begin the idempotency record.
//! 3. Run the `vision` CPU pipeline on `spawn_blocking`, behind a `Semaphore` so a burst of
//!    uploads can't take every blocking thread: decode (HEIC included), quality check, EXIF
//!    orientation, metadata stripped, faces blurred (fail closed: `503 processing_unavailable`),
//!    resize, WebP; depth rotated, faces zeroed, 16-bit PNG.
//! 4. Put the photo (`If-None-Match: *`), then the depth maps; a depth put that fails deletes
//!    what was written and reports `storage_error`, keeping the photo.
//! 5. Then the record. A crash between 4 and 5 leaves an object without a record, which the daily
//!    orphan sweep deletes; never a record without its object.

use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use sha2::{Digest, Sha256};

use domain::{
    authz::{ConversationOwner, Editor, Membership, Viewer},
    chat::ChatImageUploaded,
    error::ErrorKind,
    id::{ChatImageId, ImageId, QuestionId, ScanId},
    ports::{
        ContentType, IdGenExt, ImageQuality, NewChatImage, NewScanImage, ObjectKey, StoredDepth,
    },
    rules::uploads::{ACCEPTED_MEDIA_TYPES, DEPTH_MAX_BYTES, IMAGE_MAX_BYTES, accepted_media_type},
    scan::{DepthOutcome, DepthProblem, ImageKind, UploadImageResponse},
    url::{ImagePath, Sign},
    wire::BoolLit,
};

use crate::{
    error::ApiError,
    extract::idempotency::hex_digest,
    reply::Wire,
    seams::{DepthInput, PhotoInfo, ProcessedDepth, UploadInput},
    state::AppState,
};

/// `depth_meta` is a small JSON record.
pub const DEPTH_META_MAX_BYTES: u64 = 64 * 1024;
/// The upload routes' `DefaultBodyLimit`: the photo, two depth maps, the meta and multipart
/// overhead.
pub const UPLOAD_BODY_LIMIT: usize = 12 * 1024 * 1024 + 2 * 4 * 1024 * 1024 + 128 * 1024;

/// A file part.
#[derive(Debug, Clone)]
pub struct FilePart {
    /// Its bytes.
    pub bytes: Bytes,
    /// Its declared content type.
    pub content_type: Option<String>,
}

/// A depth part: absent, read, or dropped because it was over its cap.
#[derive(Debug, Clone, Default)]
pub enum DepthPart {
    /// Not sent.
    #[default]
    Absent,
    /// Read.
    Read(Bytes),
    /// Over 4 MiB: not buffered past the cap.
    TooLarge,
}

/// The parts of an upload, read with their caps.
#[derive(Debug, Clone, Default)]
pub struct UploadForm {
    /// `kind`.
    pub kind: Option<String>,
    /// `file`.
    pub file: Option<FilePart>,
    /// `question_id`.
    pub question_id: Option<String>,
    /// `depth`.
    pub depth: DepthPart,
    /// `depth_confidence`.
    pub depth_confidence: DepthPart,
    /// `depth_meta`.
    pub depth_meta: Option<Bytes>,
}

/// How a capped read ended.
enum Capped {
    Read(Bytes),
    Over,
}

/// Reads one field chunk by chunk, stopping at `max` bytes, so nothing past the cap is ever
/// buffered.
async fn read_capped(
    field: &mut axum::extract::multipart::Field<'_>,
    max: u64,
) -> Result<Capped, ApiError> {
    let mut buffer = BytesMut::new();
    while let Some(chunk) = field.chunk().await? {
        if (buffer.len() + chunk.len()) as u64 > max {
            // Drain the rest of the field so the next one can be read.
            while field.chunk().await?.is_some() {}
            return Ok(Capped::Over);
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(Capped::Read(buffer.freeze()))
}

fn text(bytes: &Bytes, field: &str) -> Result<String, ApiError> {
    String::from_utf8(bytes.to_vec())
        .map(|s| s.trim().to_owned())
        .map_err(|_| ApiError::invalid_request(field, "not UTF-8 text"))
}

/// Streams the multipart body into an [`UploadForm`]. Unknown fields are skipped.
pub async fn read_form(mut multipart: axum::extract::Multipart) -> Result<UploadForm, ApiError> {
    let mut form = UploadForm::default();
    while let Some(mut field) = multipart.next_field().await? {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "file" => {
                let content_type = field.content_type().map(str::to_owned);
                match read_capped(&mut field, IMAGE_MAX_BYTES).await? {
                    Capped::Read(bytes) => {
                        form.file = Some(FilePart {
                            bytes,
                            content_type,
                        });
                    }
                    Capped::Over => {
                        return Err(ApiError::new(ErrorKind::UploadTooLarge {
                            max_bytes: IMAGE_MAX_BYTES,
                        }));
                    }
                }
            }
            "depth" | "depth_confidence" => {
                let part = match read_capped(&mut field, DEPTH_MAX_BYTES).await? {
                    Capped::Read(bytes) => DepthPart::Read(bytes),
                    Capped::Over => DepthPart::TooLarge,
                };
                if name == "depth" {
                    form.depth = part;
                } else {
                    form.depth_confidence = part;
                }
            }
            "depth_meta" => match read_capped(&mut field, DEPTH_META_MAX_BYTES).await? {
                Capped::Read(bytes) => form.depth_meta = Some(bytes),
                Capped::Over => form.depth_meta = Some(Bytes::new()),
            },
            "kind" | "question_id" => {
                let Capped::Read(bytes) = read_capped(&mut field, 128).await? else {
                    return Err(ApiError::invalid_request(name, "too long"));
                };
                let value = text(&bytes, &name)?;
                if name == "kind" {
                    form.kind = Some(value);
                } else {
                    form.question_id = Some(value);
                }
            }
            _ => while field.chunk().await?.is_some() {},
        }
    }
    Ok(form)
}

impl UploadForm {
    /// The idempotency fingerprint ([contracts 1]): every field name with a hash of its value,
    /// in a fixed order, never the raw body, whose boundary changes on each retry. A part over its
    /// cap counts by its marker, so a retry with the same oversized depth matches.
    pub fn fingerprint(&self) -> String {
        fn part(hasher: &mut Sha256, name: &str, value: Option<&[u8]>) {
            hasher.update(name.as_bytes());
            hasher.update(b"=");
            match value {
                Some(bytes) => {
                    hasher.update(Sha256::digest(bytes));
                }
                None => hasher.update(b"-"),
            }
            hasher.update(b"\n");
        }
        fn depth(d: &DepthPart) -> Option<&[u8]> {
            match d {
                DepthPart::Absent => None,
                DepthPart::Read(b) => Some(b),
                DepthPart::TooLarge => Some(b"<too_large>"),
            }
        }
        let mut hasher = Sha256::new();
        part(&mut hasher, "kind", self.kind.as_deref().map(str::as_bytes));
        part(
            &mut hasher,
            "question_id",
            self.question_id.as_deref().map(str::as_bytes),
        );
        part(
            &mut hasher,
            "file",
            self.file.as_ref().map(|f| f.bytes.as_ref()),
        );
        part(&mut hasher, "depth", depth(&self.depth));
        part(
            &mut hasher,
            "depth_confidence",
            depth(&self.depth_confidence),
        );
        part(&mut hasher, "depth_meta", self.depth_meta.as_deref());
        hex_digest(hasher)
    }

    fn file(&self) -> Result<&FilePart, ApiError> {
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| ApiError::invalid_request("file", "required"))?;
        // The declared type is checked here; the pipeline sniffs the bytes too.
        if let Some(declared) = &file.content_type
            && declared != "application/octet-stream"
            && !accepted_media_type(declared)
        {
            return Err(ApiError::new(ErrorKind::UnsupportedMediaType {
                accepted: ACCEPTED_MEDIA_TYPES
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            }));
        }
        Ok(file)
    }
}

fn parse_kind(raw: Option<&str>) -> Result<ImageKind, ApiError> {
    let raw = raw.ok_or_else(|| ApiError::invalid_request("kind", "required"))?;
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|_| ApiError::invalid_request("kind", "not an image kind"))
}

/// Runs the CPU pipeline on a blocking thread, behind the upload semaphore. The closure owns its
/// buffers (`'static`, as `spawn_blocking` requires) and lends them to the pipeline for the call:
/// `UploadInput<'_>` borrows from the closure's own locals.
async fn process(
    state: &Arc<AppState>,
    kind: ImageKind,
    form: &UploadForm,
) -> Result<crate::seams::ProcessedUpload, ApiError> {
    let file = form.file()?.bytes.clone();
    let depth = match &form.depth {
        DepthPart::Read(d) => Some((
            d.clone(),
            match &form.depth_confidence {
                DepthPart::Read(c) => Some(c.clone()),
                _ => None,
            },
            form.depth_meta.clone().unwrap_or_default(),
        )),
        DepthPart::Absent | DepthPart::TooLarge => None,
    };
    let permit = Arc::clone(&state.upload_permits)
        .acquire_owned()
        .await
        .map_err(ApiError::internal)?;
    let images = Arc::clone(&state.images);
    let processed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let input = UploadInput {
            kind,
            file: &file,
            depth: depth.as_ref().map(|(d, c, m)| DepthInput {
                depth: d,
                confidence: c.as_deref(),
                meta: m,
            }),
        };
        images.process(input)
    })
    .await??;
    Ok(processed)
}

/// `POST /scans/{id}/images` after the idempotency record began.
pub async fn store_scan_image(
    state: &Arc<AppState>,
    editor: Membership<Editor>,
    scan: ScanId,
    form: UploadForm,
) -> Result<Wire<UploadImageResponse>, ApiError> {
    let kind = parse_kind(form.kind.as_deref())?;
    let question: Option<QuestionId> = form
        .question_id
        .as_deref()
        .map(|q| {
            q.parse()
                .map_err(|_| ApiError::invalid_request("question_id", "not an id"))
        })
        .transpose()?;
    let early_depth_problem =
        matches!(form.depth, DepthPart::TooLarge).then_some(DepthProblem::TooLarge);
    let processed = process(state, kind, &form).await?;

    let garden = editor.garden();
    let image: ImageId = state.ports.ids.id();
    let key = ObjectKey::scan_image(garden, scan, image);
    state
        .ports
        .blob
        .put(&key, processed.image.webp.clone(), ContentType::Webp)
        .await
        .map_err(|e| {
            ApiError::new(ErrorKind::ProcessingUnavailable)
                .retry_after(2)
                .cause(e)
        })?;

    let (stored_depth, depth_outcome) = match processed.depth {
        Some(Ok(d)) => store_depth(state, garden, scan, image, d).await,
        Some(Err(problem)) => (None, Some(dropped(problem))),
        None => (None, early_depth_problem.map(dropped)),
    };

    let photo_answer = match question {
        Some(q) => Some(
            state
                .pipeline
                .photo_answer_start(
                    scan,
                    q,
                    PhotoInfo {
                        image_id: image,
                        kind,
                        width: processed.image.width,
                        height: processed.image.height,
                        has_depth: stored_depth.is_some(),
                    },
                )
                .await?,
        ),
        None => None,
    };
    let added = state
        .ports
        .store
        .add_image(
            editor,
            scan,
            NewScanImage {
                id: image,
                kind,
                question,
                width: processed.image.width,
                height: processed.image.height,
                storage_key: key,
                quality: ImageQuality {
                    blur_score: processed.quality.blur_score,
                    exposure: processed.quality.exposure,
                },
                depth: stored_depth,
                photo_answer_start: photo_answer,
                uploaded_at: state.ports.clock.now(),
            },
        )
        .await?;
    // A guided job or a question photo's segmentation job was queued in `add_image`'s
    // transaction; the store woke the claims after its commit.
    if let Some(claim) = added.photo_answer {
        state.spawn_follow_up(state.pipeline.photo_answer_added(claim));
    }
    let response = UploadImageResponse {
        image: added.image,
        quality: processed.quality,
        depth: depth_outcome,
    };
    Ok(Wire::created(response.sign(scan, &state.signer())))
}

fn dropped(problem: DepthProblem) -> DepthOutcome {
    DepthOutcome::Dropped {
        stored: BoolLit,
        problem,
    }
}

/// Puts the depth map and its confidence map. Any failure deletes what was written and reports
/// `storage_error`; the photo stays ([storage 4], uploads).
async fn store_depth(
    state: &AppState,
    garden: domain::id::GardenId,
    scan: ScanId,
    image: ImageId,
    depth: ProcessedDepth,
) -> (Option<StoredDepth>, Option<DepthOutcome>) {
    let blob = &state.ports.blob;
    let depth_key = ObjectKey::depth(garden, scan, image);
    let confidence_key = depth
        .confidence_png
        .as_ref()
        .map(|_| ObjectKey::depth_confidence(garden, scan, image));
    let mut written = Vec::new();
    let mut result = blob
        .put(&depth_key, depth.depth_png.clone(), ContentType::Png)
        .await;
    if result.is_ok() {
        written.push(depth_key.clone());
        if let (Some(key), Some(png)) = (&confidence_key, &depth.confidence_png) {
            result = blob.put(key, png.clone(), ContentType::Png).await;
            if result.is_ok() {
                written.push(key.clone());
            }
        }
    }
    match result {
        Ok(()) => (
            Some(StoredDepth {
                depth_key,
                confidence_key,
                meta: depth.meta,
                width: depth.width,
                height: depth.height,
            }),
            Some(DepthOutcome::Stored { stored: BoolLit }),
        ),
        Err(error) => {
            tracing::warn!(%scan, %image, error = %error, "depth not stored");
            if !written.is_empty() {
                // Through the delete queue: detached, never cancelled, retried (storage 4). The
                // outcome isn't needed; the queue logs a delete that gave up, and the orphan
                // sweep finds the keys, since no record points to them.
                drop(state.deletes.submit(blob::DeleteJob::Keys(written)));
            }
            (None, Some(dropped(DepthProblem::StorageError)))
        }
    }
}

/// `POST /conversations/{id}/images` after the idempotency record began: the same pipeline
/// without the quality check or depth ([contracts 8]: "depth parts are ignored").
pub async fn store_chat_image(
    state: &Arc<AppState>,
    owner: ConversationOwner<Viewer>,
    form: UploadForm,
) -> Result<Wire<ChatImageUploaded>, ApiError> {
    let file = form.file()?.bytes.clone();
    let permit = Arc::clone(&state.upload_permits)
        .acquire_owned()
        .await
        .map_err(ApiError::internal)?;
    let images = Arc::clone(&state.images);
    let processed = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        images.process_chat(&file)
    })
    .await??;
    let conversation = owner.conversation();
    let image: ChatImageId = state.ports.ids.id();
    let key = ObjectKey::chat_image(owner.membership().garden(), conversation, image);
    state
        .ports
        .blob
        .put(&key, processed.webp, ContentType::Webp)
        .await
        .map_err(|e| {
            ApiError::new(ErrorKind::ProcessingUnavailable)
                .retry_after(2)
                .cause(e)
        })?;
    let id = state
        .ports
        .store
        .add_chat_image(
            owner,
            NewChatImage {
                id: image,
                width: processed.width,
                height: processed.height,
                storage_key: key,
                uploaded_at: state.ports.clock.now(),
            },
        )
        .await?;
    let url = state.signer().url(&ImagePath::ChatImage {
        conversation,
        image: id,
    });
    Ok(Wire::created(ChatImageUploaded { image_id: id, url }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_covers_bytes_not_boundaries() {
        let mut a = UploadForm {
            kind: Some("leaf_closeup".to_owned()),
            file: Some(FilePart {
                bytes: Bytes::from_static(b"jpeg"),
                content_type: Some("image/jpeg".to_owned()),
            }),
            ..UploadForm::default()
        };
        let same = a.fingerprint();
        assert_eq!(same, a.clone().fingerprint());
        a.depth = DepthPart::TooLarge;
        assert_ne!(same, a.fingerprint());
    }
}
