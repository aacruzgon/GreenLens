//! The adapter over `vision::measure` (compiled once that module exists).

use super::{ImageMeasurer, MeasureFailed, MeasureRequest, MeasuredQuantity};

/// `vision::measure::Measurer` as the pipeline's [`ImageMeasurer`]: maps the owned request
/// onto vision's borrowed `MeasureInput<'_>` inside the blocking closure.
pub struct VisionMeasurer {
    inner: std::sync::Arc<vision::measure::Measurer>,
}

impl VisionMeasurer {
    /// Wraps a shared measurer.
    pub fn new(inner: std::sync::Arc<vision::measure::Measurer>) -> Self {
        Self { inner }
    }
}

impl ImageMeasurer for VisionMeasurer {
    fn version(&self) -> String {
        self.inner.version()
    }

    fn measure(&self, request: &MeasureRequest) -> Result<Vec<MeasuredQuantity>, MeasureFailed> {
        let regions: Vec<vision::measure::MeasureRegion> = request
            .regions
            .iter()
            .map(|r| vision::measure::MeasureRegion {
                region_id: r.region_id,
                class: r.class,
                bbox: r.bbox,
                mask: r.mask.clone(),
            })
            .collect();
        let input = vision::measure::MeasureInput {
            image_id: request.image_id,
            kind: request.kind,
            photo: &request.photo,
            depth_png: &request.depth_png,
            confidence_png: request.confidence_png.as_deref(),
            meta: &request.meta,
            regions: &regions,
        };
        self.inner
            .measure(&input)
            .map(|qs| {
                qs.into_iter()
                    .map(|q| MeasuredQuantity {
                        measurement: q.measurement,
                        confidence: q.confidence,
                        bbox: q.bbox,
                    })
                    .collect()
            })
            .map_err(|e| MeasureFailed(e.to_string()))
    }

    fn organism_length_mm(
        &self,
        depth_png: &[u8],
        meta: &domain::scan::DepthMeta,
        bbox: domain::wire::BBox,
    ) -> Option<f64> {
        let depth = vision::depth::decode_depth_png(depth_png).ok()?;
        self.inner.organism_length_mm(&depth.view(), meta, bbox)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::{
        scan::{DepthMeta, DepthSource, Intrinsics},
        wire::BBox,
    };

    use super::*;

    #[test]
    fn an_organism_box_on_an_encoded_depth_map_has_its_length() {
        let meta = DepthMeta {
            source: DepthSource::Lidar,
            intrinsics: Intrinsics {
                fx: 1000.0,
                fy: 1000.0,
                cx: 500.0,
                cy: 500.0,
                ref_width: 1000,
                ref_height: 1000,
            },
            distortion: None,
            gravity: None,
            device_model: "test".to_owned(),
            filtered: false,
        };
        // 300 mm everywhere, as the upload stores it: a 16-bit PNG.
        let depth = vision::depth::DepthMapBuf::from_raw(10, 10, vec![300; 100]).unwrap();
        let png = vision::depth::encode_depth_png(depth.view()).unwrap();
        let measurer = VisionMeasurer::new(Arc::new(vision::measure::Measurer::new(
            vision::measure::MeasurementConfig::default(),
            Arc::new(vision::measure::spots::ColourSpotSegmenter::default()),
        )));
        // 0.1 of 1000 px at 0.3 m with fx 1000: 30 mm.
        let len = measurer
            .organism_length_mm(&png, &meta, BBox::new(0.4, 0.4, 0.1, 0.05).unwrap())
            .unwrap();
        assert!((len - 30.0).abs() < 1e-9);
        assert!(
            measurer
                .organism_length_mm(b"not a png", &meta, BBox::FULL)
                .is_none()
        );
    }
}
