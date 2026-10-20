# The plant check on the val split (plant-check-val-2026-10-19-detector-bglic)

> Detector `rfdetr-s-leaf-2026-10-19-bglic`, trained on the background pick with no COCO-Stuff plant pixel and a licence that allows commercial use and adaptation (`background_coco_train.tsv`, Oct 19 evening).

1152 plant photos (the split) and 4758 photos without a plant (`negatives_coco.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: ml/models/files/leaf_detector/rfdetr-s-leaf-2026-10-19-bglic-fp32.onnx. The manifest's `τ_leaf` (0.5) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.5:** false `no_plant_found` 7.0% (81 no-leaf photos); negatives passing 2.9%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|
| 0.35 | 3.30% | 0.00% | 0.89% | 7.66% | 1.23% | 6.2% |
| 0.50 | 7.03% | 0.00% | 1.18% | 16.89% | 2.47% | 2.9% |
| 0.70 | 16.06% | 1.73% | 5.33% | 34.68% | 9.88% | 1.0% |
| 0.90 | 48.61% | 16.61% | 48.22% | 67.57% | 60.49% | 0.1% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The manifest pair is the current detector's, chosen on the val split with it; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| manifest pair (current detector's) | A | 0.90 | 0.67 | 0.87% | 0.69% | 0.00% | 1.80% | 0.00% | 10.0% |

Photos without a plant passing, by group:

| Pair | indoor (n 1722) | outdoor (n 1152) | people_pets (n 1836) | unlabelled (n 48) |
|---|---|---|---|---|
| manifest pair (current detector's) | 10.4% | 13.5% | 7.4% | 12.5% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.55 | 0.90 | 0.95% | 0.00% | 0.00% | 2.48% | 0.00% | 5.4% |
| pooled | B | 0.60 | 0.97 | 0.95% | 0.00% | 0.00% | 2.48% | 0.00% | 5.1% |
| every dataset | A | 0.40 | 0.70 | 0.35% | 0.00% | 0.00% | 0.90% | 0.00% | 12.5% |
| every dataset | B | 0.35 | 0.93 | 0.35% | 0.00% | 0.00% | 0.90% | 0.00% | 12.4% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A | 0.85 | 0.8% | 88.9% | 4.9% | 7.2% |
| B (the rule's pick) | 0.96 | 0.7% | 90.1% | 4.8% | 7.1% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car.

## Negatives by group, prompts B at τ_plant 0.96

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 1722 | 3.9% | 5.0% |
| outdoor | 1152 | 8.2% | 12.1% |
| people_pets | 1836 | 3.4% | 5.7% |
| unlabelled | 48 | 8.3% | 14.6% |

## No-leaf photos by problem, prompts B

| Problem | no leaf | rescued |
|---|---|---|
| apple_cedar_apple_rust | 1 | 1 |
| corn_curvularia_leaf_spot | 10 | 8 |
| corn_gray_leaf_spot | 1 | 1 |
| corn_northern_leaf_blight | 3 | 3 |
| healthy | 10 | 9 |
| tomato_bacterial_wilt | 33 | 32 |
| tomato_early_blight | 16 | 13 |
| tomato_late_blight | 1 | 0 |
| tomato_leaf_mold | 1 | 1 |
| tomato_two_spotted_spider_mite | 4 | 4 |
| tomato_yellow_leaf_curl_virus | 1 | 1 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.3% | 95.1% | 14.9% | 16.8% |
| 0.60 | 0.4% | 93.8% | 12.0% | 14.0% |
| 0.70 | 0.4% | 93.8% | 9.0% | 11.0% |
| 0.80 | 0.7% | 90.1% | 6.2% | 8.4% |
| 0.90 | 1.0% | 86.4% | 3.7% | 6.0% |
| 0.95 | 1.6% | 77.8% | 2.3% | 4.8% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.3% | 96.3% | 34.4% | 35.6% |
| 0.60 | 0.3% | 96.3% | 28.2% | 29.6% |
| 0.70 | 0.3% | 95.1% | 22.2% | 23.7% |
| 0.80 | 0.3% | 95.1% | 16.2% | 18.0% |
| 0.90 | 0.5% | 92.6% | 10.0% | 12.0% |
| 0.95 | 0.7% | 90.1% | 5.7% | 7.9% |
