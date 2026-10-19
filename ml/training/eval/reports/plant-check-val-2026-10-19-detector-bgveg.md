# The plant check on the val split (plant-check-val-2026-10-19-detector-bgveg)

> Detector `rfdetr-s-leaf-2026-10-19-bgveg`, trained on the background pick with no COCO-Stuff plant pixel (`background_coco_train.tsv` at 8aebf9a).

1152 plant photos (the split) and 4758 photos without a plant (`negatives_coco.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: ml/models/files/leaf_detector/rfdetr-s-leaf-2026-10-19-bgveg-fp32.onnx. The manifest's `τ_leaf` (0.9) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.9:** false `no_plant_found` 43.4% (500 no-leaf photos); negatives passing 0.0%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|
| 0.35 | 2.52% | 0.00% | 0.59% | 6.08% | 0.00% | 6.2% |
| 0.50 | 5.47% | 0.00% | 1.18% | 13.06% | 1.23% | 2.9% |
| 0.70 | 13.02% | 0.35% | 4.44% | 28.38% | 9.88% | 0.9% |
| 0.90 | 43.40% | 15.57% | 38.76% | 62.39% | 58.02% | 0.0% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The manifest pair is the current detector's, chosen on the val split with it; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| manifest pair (current detector's) | A | 0.90 | 0.67 | 0.61% | 0.35% | 0.00% | 1.35% | 0.00% | 10.0% |

Photos without a plant passing, by group:

| Pair | indoor (n 1722) | outdoor (n 1152) | people_pets (n 1836) | unlabelled (n 48) |
|---|---|---|---|---|
| manifest pair (current detector's) | 10.4% | 13.5% | 7.4% | 12.5% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.50 | 0.96 | 0.95% | 0.00% | 0.30% | 2.25% | 0.00% | 4.7% |
| pooled | B | 0.52 | 0.98 | 0.95% | 0.00% | 0.30% | 2.25% | 0.00% | 5.1% |
| every dataset | A | 0.50 | 0.88 | 0.43% | 0.00% | 0.30% | 0.90% | 0.00% | 6.6% |
| every dataset | B | 0.40 | 0.97 | 0.35% | 0.00% | 0.00% | 0.90% | 0.00% | 7.8% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A (the rule's pick) | 0.85 | 2.5% | 94.2% | 4.9% | 4.9% |
| B | 0.96 | 2.7% | 93.8% | 4.8% | 4.8% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car.

## Negatives by group, prompts A at τ_plant 0.85

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 1722 | 5.2% | 5.2% |
| outdoor | 1152 | 7.0% | 7.1% |
| people_pets | 1836 | 3.3% | 3.3% |
| unlabelled | 48 | 6.2% | 6.2% |

## No-leaf photos by problem, prompts A

| Problem | no leaf | rescued |
|---|---|---|
| apple_cedar_apple_rust | 4 | 4 |
| apple_scab | 3 | 3 |
| bean_angular_leaf_spot | 9 | 8 |
| bean_rust | 22 | 22 |
| corn_common_rust | 4 | 4 |
| corn_curvularia_leaf_spot | 11 | 9 |
| corn_gray_leaf_spot | 4 | 3 |
| corn_northern_leaf_blight | 14 | 10 |
| healthy | 234 | 219 |
| pepper_bacterial_leaf_spot | 6 | 6 |
| potato_late_blight | 6 | 6 |
| squash_powdery_mildew | 4 | 4 |
| tomato_bacterial_spot | 3 | 3 |
| tomato_bacterial_wilt | 43 | 42 |
| tomato_early_blight | 46 | 43 |
| tomato_late_blight | 11 | 10 |
| tomato_leaf_mold | 23 | 22 |
| tomato_mosaic_virus | 3 | 3 |
| tomato_septoria_leaf_spot | 12 | 12 |
| tomato_two_spotted_spider_mite | 25 | 25 |
| tomato_yellow_leaf_curl_virus | 13 | 13 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.3% | 99.2% | 14.9% | 14.9% |
| 0.60 | 0.5% | 98.8% | 12.0% | 12.0% |
| 0.70 | 1.0% | 97.8% | 9.0% | 9.0% |
| 0.80 | 1.6% | 96.2% | 6.2% | 6.2% |
| 0.90 | 3.3% | 92.4% | 3.7% | 3.7% |
| 0.95 | 6.5% | 85.0% | 2.3% | 2.3% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.3% | 99.4% | 34.4% | 34.4% |
| 0.60 | 0.3% | 99.4% | 28.2% | 28.2% |
| 0.70 | 0.3% | 99.2% | 22.2% | 22.2% |
| 0.80 | 0.4% | 99.0% | 16.2% | 16.2% |
| 0.90 | 0.9% | 98.0% | 10.0% | 10.0% |
| 0.95 | 1.9% | 95.6% | 5.7% | 5.7% |
