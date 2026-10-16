# Head and label-matrix files

> **Status:** Proposal v0.1, 2026-10-16. To be confirmed with the Rust perception worker's author ([backend vision 15](../../docs/backend/vision.md#15-open-questions), question 5). Written and read by [`pdm_vision/headfile.py`](../pdm_vision/headfile.py).

The backend fixes only that these are JSON files with their matrices as base64 little-endian `f32` ([backend vision 5](../../docs/backend/vision.md#5-the-model-manifest)). This is the exact shape `ml/training` writes. Matrices are row-major. Each file has a `format` string, and a reader refuses a format it doesn't know.

## Linear head: `pdm-linear-head/1`

Used by `disease_head*`, `species_head` and `plant_probe`. Output `i` is `(weight[i] · e + bias[i]) / temperature`, where `e` is the L2-normalised BioCLIP 2 embedding.

| Field | Type | Meaning |
|---|---|---|
| `format` | `"pdm-linear-head/1"` | |
| `kind` | `"disease_head"` \| `"species_head"` \| `"plant_probe"` | |
| `version` | string | The manifest entry's `version`. For the disease head it equals `classifier_labels.yaml`'s `model_version` |
| `features` | string | The backbone the head was trained on, e.g. `bioclip-2@2957b322` |
| `dim` | integer | 768 for BioCLIP 2 |
| `classes` | integer | 1 for the plant probe |
| `temperature` | number | `T_disease`, `T_species`; 1.0 for the plant probe |
| `labels` | array, length `classes` | Disease head: `{index, label, species_id, problem_id}` with `problem_id` a slug or `"healthy"`, the same rows as `classifier_labels.yaml`. Species head: `{index, species_id}`. Plant probe: `[{index: 0, label: "plant"}]` |
| `weight` | base64 | `classes × dim` `f32` |
| `bias` | base64 | `classes` `f32` |

The plant probe's probability is `σ(weight · e + bias)`. The heads apply softmax over the working species' rows only ([contracts 5](../../docs/architecture/contracts.md#results)).

## Label matrix: `pdm-label-matrix/1`

Used by `bioclip_labels_<label_set_version>`.

| Field | Type | Meaning |
|---|---|---|
| `format` | `"pdm-label-matrix/1"` | |
| `label_set_version` | string | First 16 hex digits of the SHA-256 over the BioCLIP revision, the template list and the sorted labels ([training 8](../../docs/vision/training.md#8-label-lists)) |
| `model` | string | `bioclip-2@<revision>` |
| `templates` | string | `openai-imagenet-80` |
| `dim`, `count` | integer | 768, N |
| `labels` | array, length N | `{id, table, label}`: `table` is `species` or `organism`, `label` the `bioclip_label` |
| `rows` | base64 | `N × dim` `f32`, each row L2-normalised |

### Computing `label_set_version`

The knowledge loader and `ml/training/labels/bioclip_text.py` must produce the same value, so the serialisation is fixed:

```python
doc = {
    "model": "bioclip-2@<full revision>",
    "templates": [t("{label}") for t in OPENAI_IMAGENET_TEMPLATES],   # open_clip's 80, in order
    "labels": sorted(f"{table}:{id}\t{bioclip_label}" for each label),
}
blob = json.dumps(doc, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
label_set_version = sha256(blob).hexdigest()[:16]
```
