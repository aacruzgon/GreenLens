//! The files `ml/` produces, read by the code that loads them on the hosts: the committed
//! manifest ([vision 6]) and the head and label-matrix formats of `ml/models/FORMATS.md`. The
//! weights aren't in git, so the head and matrix files here are written the way
//! `ml/pdm_vision/headfile.py` writes them: `json.dumps(doc, indent=1)`, matrices as standard
//! base64 of little-endian `f32`, row-major.
//!
//! [vision 6]: ../../../docs/vision/README.md#6-model-files-versions-and-pinning

use std::path::PathBuf;

use data_encoding::BASE64;
use domain::id::{ProblemOrHealthy, SpeciesId};
use serde_json::{Value, json};
use vision::{
    manifest::Manifest,
    perception::{
        BatchCaps, PerceptionConfig, TensorNames, check_precision,
        heads::{DiseaseHead, EMBEDDING_DIM, LabelMatrix, PlantProbe},
    },
    runtime::OrtEp,
};

fn ml_models() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../ml/models")
}

/// `headfile.b64_f32`.
fn b64(values: &[f32]) -> String {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    BASE64.encode(&bytes)
}

/// A one-hot row of the embedding width.
fn one_hot(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; EMBEDDING_DIM];
    v[i] = 1.0;
    v
}

/// Writes `doc` as `headfile.py` does, to a file of this test run's own.
fn write(name: &str, doc: &Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdm-vision-ml-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the temporary directory");
    let path = dir.join(name);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(doc).expect("JSON") + "\n",
    )
    .expect("the fixture is written");
    path
}

const FEATURES: &str = "bioclip-2@2957b322+b9016a6865e188c0";

/// `headfile.write_linear_head` for a disease head over tomato's three classes.
fn disease_head(labels: &Value) -> Value {
    let weight: Vec<f32> = (0..3).flat_map(one_hot).collect();
    json!({
        "format": "pdm-linear-head/1",
        "kind": "disease_head",
        "version": "disease-head-v1d-2026-10-16",
        "features": FEATURES,
        "dim": EMBEDDING_DIM,
        "classes": 3,
        "temperature": 0.8208,
        "labels": labels,
        "weight": b64(&weight),
        "bias": b64(&[0.0, 0.0, 0.0]),
    })
}

fn tomato_labels() -> Value {
    json!([
        { "index": 0, "label": "tomato/early_blight", "species_id": "tomato", "problem_id": "early_blight" },
        { "index": 1, "label": "tomato/healthy", "species_id": "tomato", "problem_id": "healthy" },
        { "index": 2, "label": "tomato/late_blight", "species_id": "tomato", "problem_id": "late_blight" },
    ])
}

fn load_err<T: std::fmt::Debug>(result: Result<T, vision::VisionError>) -> String {
    result.expect_err("the file is refused").to_string()
}

#[test]
fn the_committed_manifest_parses() {
    let manifest =
        Manifest::load(&ml_models().join("manifest.toml")).expect("ml/models/manifest.toml parses");
    for name in ["bioclip", "sam_encoder", "sam_decoder"] {
        manifest.entry(name).expect(name);
    }
    let head = manifest
        .entries_with_prefix("disease_head")
        .next()
        .expect("a disease head entry");
    assert_eq!(
        head.classifier_labels_version.as_deref(),
        Some(head.version.as_str()),
        "a head's classifier_labels_version is its version"
    );
}

/// Both committed manifests name every tensor the perception adapter runs (gap G4): `BioCLIP`'s
/// export takes `images` and returns `embeddings`.
#[test]
fn the_committed_manifests_name_every_tensor() {
    for file in ["manifest.toml", "manifest.cpu.toml"] {
        let manifest = Manifest::load(&ml_models().join(file)).expect(file);
        let names = TensorNames::from_manifest(&manifest).expect(file);
        assert_eq!(names.bioclip_input, "images", "{file}");
        assert_eq!(names.bioclip_output, "embeddings", "{file}");
        assert_eq!(names.detector_input, "input", "{file}");
        assert_eq!(names.detector_outputs, ["dets", "labels"], "{file}");
        assert_eq!(names.sam_encoder_input, "images", "{file}");
        assert_eq!(
            names.sam_encoder_outputs,
            ["high_res_feats_0", "high_res_feats_1", "image_embed"],
            "{file}"
        );
        assert_eq!(
            names.sam_decoder_outputs,
            ["mask_single", "iou_single"],
            "{file}"
        );
    }
}

#[test]
fn a_manifest_without_an_output_the_adapter_runs_is_refused() {
    let text = std::fs::read_to_string(ml_models().join("manifest.toml")).expect("the manifest");
    let without = text.replacen("embeddings = \"embeddings\"", "pooled = \"embeddings\"", 1);
    assert_ne!(text, without, "the bioclip output line is in the manifest");
    let manifest = Manifest::parse(&without).expect("still a manifest");
    let err = TensorNames::from_manifest(&manifest)
        .expect_err("no embeddings output")
        .to_string();
    assert!(err.contains("bioclip has no output embeddings"), "{err}");
}

fn committed_manifest_text() -> String {
    std::fs::read_to_string(ml_models().join("manifest.toml")).expect("the manifest")
}

/// The manifest text without its `plant_probe` entry, for the tests that need no probe or add
/// their own.
fn without_probe(text: &str) -> String {
    let mut parts = text.split("[[model]]");
    let mut out = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        let is_probe = part.lines().any(|l| {
            l.split('#').next().unwrap_or_default().replace(' ', "") == "name=\"plant_probe\""
        });
        if !is_probe {
            out.push_str("[[model]]");
            out.push_str(part);
        }
    }
    out
}

fn config_of(text: &str) -> Result<PerceptionConfig, vision::manifest::ManifestError> {
    PerceptionConfig::from_manifest(
        &Manifest::parse(text).expect("a manifest"),
        BatchCaps::default(),
    )
}

/// Gap G6: every threshold comes from the manifest, in both committed manifests.
#[test]
fn the_thresholds_come_from_the_committed_manifests() {
    for file in ["manifest.toml", "manifest.cpu.toml"] {
        let manifest = Manifest::load(&ml_models().join(file)).expect(file);
        let caps = BatchCaps {
            sam_encoder: 1,
            ..BatchCaps::default()
        };
        let c = PerceptionConfig::from_manifest(&manifest, caps).expect(file);
        assert_eq!(c.caps, caps, "the caps are the environment's");
        assert!((c.detector.score_threshold - 0.35).abs() < 1e-6, "{file}");
        assert!((c.detector.min_side_px - 24.0).abs() < 1e-6, "{file}");
        assert_eq!(c.detector.max_per_image, 40, "{file}");
        assert!((c.detector.nms_iou - 0.8).abs() < 1e-6, "{file}");
        assert!((c.tau_leaf - 0.90).abs() < 1e-6, "{file}");
        assert!((c.sam.min_iou - 0.6).abs() < 1e-6, "{file}");
        assert_eq!(c.sam.plant_min_leaves, 3, "{file}");
        assert!((c.sam.plant_grow - 0.05).abs() < 1e-12, "{file}");
        assert!((c.sam.plant_min_cover - 0.6).abs() < 1e-12, "{file}");
        assert!((c.species_temperature - 1.0).abs() < 1e-6, "{file}");
        assert!((c.crop_grow - 0.10).abs() < 1e-12, "{file}");
        assert_eq!(c.tau_plant, Some(0.67), "{file}: the day-1 plant probe's");
    }
}

/// Removes `key` from the `[model.config]` of the entry named `entry`, and only there: keys
/// like `max_batch` appear in several entries.
fn without_key(text: &str, entry: &str, key: &str) -> String {
    let mut current = "";
    let mut removed = false;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            if let Some(rest) = line.strip_prefix("name") {
                current = rest.trim_start_matches([' ', '=']).trim_matches('"');
            }
            let hit = !removed
                && current == entry
                && line.trim_start().split([' ', '=']).next() == Some(key);
            removed |= hit;
            !hit
        })
        .collect();
    assert!(removed, "{entry} has {key} in the committed manifest");
    kept.join("\n")
}

/// Every key `from_manifest` reads refuses to start when it is missing, naming its entry.
#[test]
fn every_missing_threshold_refuses_to_start() {
    let text = committed_manifest_text();
    let head = "disease_head_disease-head-v1d-2026-10-16";
    for (entry, key) in [
        ("leaf_detector", "score_threshold"),
        ("leaf_detector", "min_side_px"),
        ("leaf_detector", "max_per_image"),
        ("leaf_detector", "nms_iou"),
        ("leaf_detector", "tau_leaf"),
        ("leaf_detector", "max_batch"),
        ("sam_encoder", "max_batch"),
        ("sam_decoder", "min_iou"),
        ("sam_decoder", "plant_min_leaves"),
        ("sam_decoder", "plant_grow"),
        ("sam_decoder", "plant_min_cover"),
        ("sam_decoder", "max_boxes"),
        ("bioclip", "species_temperature"),
        ("bioclip", "max_batch"),
        (head, "crop_grow"),
    ] {
        let err = config_of(&without_key(&text, entry, key))
            .expect_err(key)
            .to_string();
        assert!(err.contains(entry) && err.contains(key), "{err}");
    }
}

/// The environment's caps may lower each model's batch, never raise it past the manifest's,
/// the batch its memory budget was measured at.
#[test]
fn a_cap_above_the_manifest_refuses_to_start() {
    let manifest = Manifest::parse(&committed_manifest_text()).expect("a manifest");
    let laptop = BatchCaps {
        sam_encoder: 1,
        detector: 4,
        bioclip: 8,
        ..BatchCaps::default()
    };
    assert!(PerceptionConfig::from_manifest(&manifest, BatchCaps::default()).is_ok());
    assert!(PerceptionConfig::from_manifest(&manifest, laptop).is_ok());
    let too_big = BatchCaps {
        sam_encoder: 4,
        ..BatchCaps::default()
    };
    let err = PerceptionConfig::from_manifest(&manifest, too_big)
        .expect_err("4 > 2")
        .to_string();
    assert_eq!(
        err,
        "SAM_ENCODER_MAX_BATCH=4 is above sam_encoder's max_batch = 2 in the manifest"
    );
}

#[test]
fn disease_heads_must_agree_on_crop_grow() {
    let text = committed_manifest_text();
    let second = |grow: &str| {
        format!(
            "{text}\n[[model]]\nname = \"disease_head_v2\"\nversion = \"v2\"\nfile = \"v2.json\"\n\
             sha256 = \"00\"\nlicence = \"MIT\"\nruns_on = \"perception\"\n\
             classifier_labels_version = \"v2\"\n\n  [model.config]\n  crop_grow = {grow}\n"
        )
    };
    assert!(config_of(&second("0.10")).is_ok());
    let err = config_of(&second("0.2"))
        .expect_err("they differ")
        .to_string();
    assert!(
        err.contains("crop_grow") && err.contains("disease_head_v2"),
        "{err}"
    );
}

#[test]
fn a_plant_probe_brings_its_own_tau_plant() {
    let probe = |config: &str| {
        format!(
            "{}\n[[model]]\nname = \"plant_probe\"\nversion = \"p1\"\nfile = \"p1.json\"\n\
             sha256 = \"00\"\nlicence = \"MIT\"\nruns_on = \"perception\"\n\n  [model.config]\n{config}\n",
            without_probe(&committed_manifest_text())
        )
    };
    let c = config_of(&probe("  tau_plant = 0.7")).expect("the probe gives tau_plant");
    assert_eq!(c.tau_plant, Some(0.7));
    let err = config_of(&probe("")).expect_err("no tau_plant").to_string();
    assert!(
        err.contains("plant_probe") && err.contains("tau_plant"),
        "{err}"
    );
}

/// Calibration works by editing the manifest: a changed value changes both the settings in use
/// and the `perception_config` that results report.
#[test]
fn a_changed_threshold_changes_the_config_and_its_hash() {
    let text = committed_manifest_text();
    let changed = text.replacen("score_threshold  = 0.35", "score_threshold  = 0.4", 1);
    assert_ne!(text, changed);
    let (a, b) = (config_of(&text).unwrap(), config_of(&changed).unwrap());
    assert!((b.detector.score_threshold - 0.4).abs() < 1e-6);
    assert_ne!(a, b);
    assert_ne!(
        Manifest::parse(&text).unwrap().perception_config(),
        Manifest::parse(&changed).unwrap().perception_config()
    );
}

/// Gap G11: the fp16 manifest on the CPU provider stops with an error naming the entry and
/// `--cpu`, instead of segfaulting in ONNX Runtime; the CPU manifest passes.
#[test]
fn fp16_models_are_refused_on_the_cpu() {
    let gpu = Manifest::load(&ml_models().join("manifest.toml")).expect("manifest.toml");
    let cpu = Manifest::load(&ml_models().join("manifest.cpu.toml")).expect("manifest.cpu.toml");
    let err = load_err(check_precision(&gpu, OrtEp::Cpu));
    assert!(err.contains("fp16") && err.contains("--cpu"), "{err}");
    assert!(check_precision(&gpu, OrtEp::Cuda).is_ok());
    assert!(check_precision(&cpu, OrtEp::Cpu).is_ok());
}

#[test]
fn one_fp16_entry_is_enough_to_refuse_the_cpu() {
    let cpu = std::fs::read_to_string(ml_models().join("manifest.cpu.toml")).expect("the manifest");
    let one = cpu.replacen(
        "file      = \"sam2/sam2.1-hiera-tiny-box-decoder-fp32.onnx\"",
        "file      = \"sam2/sam2.1-hiera-tiny-box-decoder-fp16.onnx\"",
        1,
    );
    let mut lines: Vec<&str> = one.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains("box-decoder-fp16"))
        .expect("the decoder entry");
    let precision = lines[at..]
        .iter()
        .position(|l| l.starts_with("precision"))
        .expect("its precision")
        + at;
    lines[precision] = "precision = \"fp16\"";
    let manifest = Manifest::parse(&lines.join("\n")).expect("a manifest");
    let err = load_err(check_precision(&manifest, OrtEp::Cpu));
    assert!(err.starts_with("sam_decoder is an fp16 model"), "{err}");
    assert!(check_precision(&manifest, OrtEp::Cuda).is_ok());
}

/// Without a plant probe only the leaf score can find a plant: no plant probability passes.
#[test]
fn without_a_probe_no_plant_probability_passes() {
    use domain::scan::ImageKind;
    use vision::perception::heads::plant_present;
    let mut c =
        config_of(&without_probe(&committed_manifest_text())).expect("the committed manifest");
    assert!(c.plant_threshold().is_infinite());
    let shot = |leaf, plant| [(ImageKind::WholePlant, leaf, plant)];
    assert!(!plant_present(
        &shot(0.0, 0.0),
        c.tau_leaf,
        c.plant_threshold()
    ));
    assert!(!plant_present(
        &shot(0.0, 1.0),
        c.tau_leaf,
        c.plant_threshold()
    ));
    assert!(plant_present(
        &shot(0.9, 0.0),
        c.tau_leaf,
        c.plant_threshold()
    ));
    c.tau_plant = Some(0.7);
    assert!(plant_present(
        &shot(0.0, 0.8),
        c.tau_leaf,
        c.plant_threshold()
    ));
}

/// An fp16 entry whose `precision` key is missing is still caught by its file name.
#[test]
fn an_fp16_file_without_a_precision_key_is_refused_on_the_cpu() {
    let gpu = std::fs::read_to_string(ml_models().join("manifest.toml")).expect("the manifest");
    let no_key: String = gpu
        .lines()
        .filter(|l| !l.starts_with("precision"))
        .collect::<Vec<_>>()
        .join("\n");
    let manifest = Manifest::parse(&no_key).expect("a manifest");
    assert!(manifest.models().iter().all(|m| m.precision.is_none()));
    let err = load_err(check_precision(&manifest, OrtEp::Cpu));
    assert!(err.contains("fp16"), "{err}");
}

#[test]
fn a_label_matrix_scores_its_species_rows_and_keeps_its_organisms() {
    let rows: Vec<f32> = [one_hot(0), one_hot(1), one_hot(2)].concat();
    let path = write(
        "labels.json",
        &json!({
            "format": "pdm-label-matrix/1",
            "label_set_version": "129f0395e8d89bd5",
            "model": "bioclip-2@2957b322090f9cb17ae72c71981c7218a28d81e0",
            "templates": "openai-imagenet-80",
            "dim": EMBEDDING_DIM,
            "count": 3,
            "labels": [
                { "id": "strawberry", "table": "species", "label": "Fragaria × ananassa with common name strawberry" },
                { "id": "ladybug_larva", "table": "organism", "label": "Coccinellidae larva with common name ladybug larva" },
                { "id": "tomato", "table": "species", "label": "Solanum lycopersicum with common name tomato" },
            ],
            "rows": b64(&rows),
        }),
    );
    let m = LabelMatrix::load(&path).expect("the matrix loads");
    assert_eq!(m.version, "129f0395e8d89bd5");
    let species: Vec<&str> = m.species.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(species, ["strawberry", "tomato"]);
    assert_eq!(m.organisms.len(), 1);
    // The image matches tomato's row: the file's third row, the second species row.
    let p = m.probabilities(&one_hot(2), 1.0);
    assert_eq!(p.len(), 2, "species rows only");
    assert!(p[1] > 0.99, "tomato wins: {p:?}");
}

#[test]
fn a_disease_head_reads_every_problem_id() {
    let path = write("head.json", &disease_head(&tomato_labels()));
    let head = DiseaseHead::load(&path).expect("the head loads");
    assert_eq!(head.version, "disease-head-v1d-2026-10-16");
    assert_eq!(head.features, FEATURES);
    let tomato: SpeciesId = "tomato".parse().unwrap();
    assert_eq!(
        head.classes(&tomato),
        [
            "early_blight".parse::<ProblemOrHealthy>().unwrap(),
            ProblemOrHealthy::Healthy,
            "late_blight".parse().unwrap(),
        ],
        "no class is read as healthy by default"
    );
    let top = head.top(&one_hot(2), &tomato, 1);
    assert_eq!(top[0].problem_id, "late_blight".parse().unwrap());
}

#[test]
fn a_disease_head_label_without_problem_id_is_refused() {
    let mut labels = tomato_labels();
    labels[0].as_object_mut().unwrap().remove("problem_id");
    let path = write("head-no-problem.json", &disease_head(&labels));
    assert!(
        load_err(DiseaseHead::load(&path)).contains("labels[0] needs species_id and problem_id")
    );
}

#[test]
fn a_head_of_another_format_kind_order_or_size_is_refused() {
    let mut doc = disease_head(&tomato_labels());
    doc["format"] = json!("pdm-linear-head/2");
    assert!(load_err(DiseaseHead::load(&write("head-format.json", &doc))).contains("format"));

    let mut doc = disease_head(&tomato_labels());
    doc["kind"] = json!("species_head");
    assert!(load_err(DiseaseHead::load(&write("head-kind.json", &doc))).contains("kind"));

    let mut labels = tomato_labels();
    labels[1]["index"] = json!(5);
    let doc = disease_head(&labels);
    assert!(load_err(DiseaseHead::load(&write("head-index.json", &doc))).contains("index 5"));

    let mut doc = disease_head(&tomato_labels());
    doc["classes"] = json!(4);
    assert!(load_err(DiseaseHead::load(&write("head-classes.json", &doc))).contains("classes"));
}

#[test]
fn the_plant_probe_is_a_one_class_linear_head() {
    let path = write(
        "probe.json",
        &json!({
            "format": "pdm-linear-head/1",
            "kind": "plant_probe",
            "version": "plant-probe-2026-10-16",
            "features": FEATURES,
            "dim": EMBEDDING_DIM,
            "classes": 1,
            "temperature": 1.0,
            "labels": [{ "index": 0, "label": "plant" }],
            "weight": b64(&one_hot(0)),
            "bias": b64(&[-1.25]),
        }),
    );
    let probe = PlantProbe::load(&path).expect("the probe loads");
    assert_eq!(probe.features, FEATURES);
    let expected = 1.0 / (1.0 + (-(1.0f32 - 1.25)).exp());
    assert!((probe.probability(&one_hot(0)) - expected).abs() < 1e-6);
}

#[test]
fn a_matrix_whose_counts_disagree_is_refused() {
    let path = write(
        "labels-short.json",
        &json!({
            "format": "pdm-label-matrix/1",
            "label_set_version": "0000000000000000",
            "dim": EMBEDDING_DIM,
            "count": 2,
            "labels": [{ "id": "tomato", "table": "species", "label": "tomato" }],
            "rows": b64(&one_hot(0)),
        }),
    );
    assert!(load_err(LabelMatrix::load(&path)).contains("count 2"));
}
