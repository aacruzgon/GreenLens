# The plant check on the test split (plant-check-test-holdout-2026-10-19-detector-bgveg)

> Detector `rfdetr-s-leaf-2026-10-19-bgveg`, trained on the background pick with no COCO-Stuff plant pixel (`background_coco_train.tsv` at 8aebf9a).

854 plant photos (the split) and 1000 photos without a plant (`negatives_coco_train_holdout.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: ml/models/files/leaf_detector/rfdetr-s-leaf-2026-10-19-bgveg-fp32.onnx. The manifest's `τ_leaf` (0.9) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.9:** false `no_plant_found` 42.2% (360 no-leaf photos); negatives passing 0.0%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|
| 0.35 | 5.50% | 0.00% | 0.00% | 11.19% | 0.00% | 6.5% |
| 0.50 | 11.48% | 0.00% | 0.00% | 23.10% | 1.25% | 3.5% |
| 0.70 | 21.66% | 0.78% | 1.33% | 41.43% | 8.75% | 0.9% |
| 0.90 | 42.15% | 12.50% | 17.70% | 63.81% | 45.00% | 0.0% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The manifest pair is the current detector's, chosen on the val split with it; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| manifest pair (current detector's) | A | 0.90 | 0.67 | 0.82% | 0.00% | 0.00% | 1.67% | 0.00% | 8.4% |
| A 0.48 / 0.96 | A | 0.48 | 0.96 | 1.29% | 0.00% | 0.00% | 2.62% | 0.00% | 4.9% |
| A 0.50 / 0.88 | A | 0.50 | 0.88 | 0.35% | 0.00% | 0.00% | 0.71% | 0.00% | 6.5% |
| A 0.50 / 0.96 | A | 0.50 | 0.96 | 1.41% | 0.00% | 0.00% | 2.86% | 0.00% | 4.6% |
| B 0.40 / 0.97 | B | 0.40 | 0.97 | 0.59% | 0.00% | 0.00% | 1.19% | 0.00% | 7.8% |
| B 0.52 / 0.98 | B | 0.52 | 0.98 | 0.94% | 0.00% | 0.00% | 1.90% | 0.00% | 5.2% |

Photos without a plant passing, by group:

| Pair | indoor (n 350) | outdoor (n 236) | people_pets (n 406) | unlabelled (n 8) |
|---|---|---|---|---|
| manifest pair (current detector's) | 9.1% | 14.4% | 4.4% | 0.0% |
| A 0.48 / 0.96 | 2.3% | 13.1% | 2.5% | 0.0% |
| A 0.50 / 0.88 | 4.9% | 16.1% | 2.5% | 0.0% |
| A 0.50 / 0.96 | 2.3% | 13.1% | 1.7% | 0.0% |
| B 0.40 / 0.97 | 4.3% | 17.8% | 4.9% | 12.5% |
| B 0.52 / 0.98 | 2.6% | 13.6% | 2.5% | 12.5% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.67 | 0.90 | 0.94% | 0.00% | 0.00% | 1.90% | 0.00% | 3.9% |
| pooled | B | 0.56 | 0.98 | 0.94% | 0.00% | 0.00% | 1.90% | 0.00% | 4.5% |
| every dataset | A | 0.65 | 0.83 | 0.47% | 0.00% | 0.00% | 0.95% | 0.00% | 5.2% |
| every dataset | B | 0.60 | 0.96 | 0.47% | 0.00% | 0.00% | 0.95% | 0.00% | 5.5% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A (the rule's pick) | 0.79 | 1.1% | 97.5% | 4.9% | 4.9% |
| B | 0.95 | 2.0% | 95.3% | 4.9% | 4.9% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car.

## Negatives by group, prompts A at τ_plant 0.79

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 350 | 6.0% | 6.0% |
| outdoor | 236 | 8.5% | 8.5% |
| people_pets | 406 | 2.0% | 2.0% |
| unlabelled | 8 | 0.0% | 0.0% |

## No-leaf photos by problem, prompts A

| Problem | no leaf | rescued |
|---|---|---|
| bean_angular_leaf_spot | 2 | 2 |
| bean_rust | 10 | 10 |
| corn_common_rust | 1 | 1 |
| corn_curvularia_leaf_spot | 2 | 2 |
| corn_northern_leaf_blight | 9 | 7 |
| healthy | 155 | 152 |
| pepper_bacterial_leaf_spot | 1 | 1 |
| potato_early_blight | 1 | 1 |
| potato_late_blight | 1 | 1 |
| squash_powdery_mildew | 2 | 2 |
| tomato_bacterial_wilt | 41 | 41 |
| tomato_early_blight | 36 | 33 |
| tomato_late_blight | 19 | 18 |
| tomato_leaf_mold | 16 | 16 |
| tomato_mosaic_virus | 4 | 4 |
| tomato_septoria_leaf_spot | 9 | 9 |
| tomato_two_spotted_spider_mite | 41 | 41 |
| tomato_yellow_leaf_curl_virus | 10 | 10 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.5% | 98.9% | 13.2% | 13.2% |
| 0.60 | 0.7% | 98.3% | 10.2% | 10.2% |
| 0.70 | 0.9% | 97.8% | 7.5% | 7.5% |
| 0.80 | 1.1% | 97.5% | 4.7% | 4.7% |
| 0.90 | 2.6% | 93.9% | 2.9% | 2.9% |
| 0.95 | 6.1% | 85.6% | 1.7% | 1.7% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.2% | 99.4% | 32.7% | 32.7% |
| 0.60 | 0.2% | 99.4% | 25.4% | 25.4% |
| 0.70 | 0.2% | 99.4% | 20.4% | 20.4% |
| 0.80 | 0.2% | 99.4% | 15.3% | 15.3% |
| 0.90 | 0.6% | 98.6% | 9.0% | 9.0% |
| 0.95 | 2.0% | 95.3% | 4.9% | 4.9% |
