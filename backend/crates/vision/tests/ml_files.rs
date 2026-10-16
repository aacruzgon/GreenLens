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
    perception::heads::{DiseaseHead, EMBEDDING_DIM, LabelMatrix, PlantProbe},
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
