# The plant check on the test split (plant-check-test-holdout-2026-10-19-detector-bglic)

> Detector `rfdetr-s-leaf-2026-10-19-bglic`, trained on the background pick with no COCO-Stuff plant pixel and a licence that allows commercial use and adaptation (`background_coco_train.tsv`, Oct 19 evening).

854 plant photos (the split) and 1000 photos without a plant (`negatives_coco_train_holdout.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: ml/models/files/leaf_detector/rfdetr-s-leaf-2026-10-19-bglic-fp32.onnx. The manifest's `τ_leaf` (0.5) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.5:** false `no_plant_found` 12.4% (106 no-leaf photos); negatives passing 2.6%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|
| 0.35 | 5.39% | 0.00% | 0.00% | 10.95% | 0.00% | 6.6% |
| 0.50 | 12.41% | 0.00% | 0.44% | 24.76% | 1.25% | 2.6% |
| 0.70 | 24.24% | 0.00% | 3.10% | 45.95% | 8.75% | 0.8% |
| 0.90 | 46.96% | 17.19% | 20.80% | 68.57% | 55.00% | 0.0% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The manifest pair is the current detector's, chosen on the val split with it; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| manifest pair (current detector's) | A | 0.90 | 0.67 | 0.94% | 0.00% | 0.00% | 1.90% | 0.00% | 8.4% |
| A 0.40 / 0.70 | A | 0.40 | 0.70 | 0.23% | 0.00% | 0.00% | 0.48% | 0.00% | 11.0% |
| A 0.50 / 0.88 | A | 0.50 | 0.88 | 0.47% | 0.00% | 0.00% | 0.95% | 0.00% | 5.7% |
| A 0.55 / 0.90 | A | 0.55 | 0.90 | 0.82% | 0.00% | 0.00% | 1.67% | 0.00% | 4.1% |
| B 0.35 / 0.93 | B | 0.35 | 0.93 | 0.23% | 0.00% | 0.00% | 0.48% | 0.00% | 12.1% |
| B 0.60 / 0.97 | B | 0.60 | 0.97 | 0.82% | 0.00% | 0.00% | 1.67% | 0.00% | 4.0% |

Photos without a plant passing, by group:

| Pair | indoor (n 350) | outdoor (n 236) | people_pets (n 406) | unlabelled (n 8) |
|---|---|---|---|---|
| manifest pair (current detector's) | 9.1% | 14.4% | 4.4% | 0.0% |
| A 0.40 / 0.70 | 9.4% | 18.6% | 8.1% | 0.0% |
| A 0.50 / 0.88 | 4.3% | 12.3% | 3.2% | 0.0% |
| A 0.55 / 0.90 | 3.4% | 8.9% | 2.0% | 0.0% |
| B 0.35 / 0.93 | 7.7% | 21.6% | 9.6% | 50.0% |
| B 0.60 / 0.97 | 2.6% | 9.3% | 2.0% | 12.5% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.64 | 0.90 | 0.94% | 0.00% | 0.00% | 1.90% | 0.00% | 3.6% |
| pooled | B | 0.60 | 0.97 | 0.82% | 0.00% | 0.00% | 1.67% | 0.00% | 4.0% |
| every dataset | A | 0.64 | 0.84 | 0.47% | 0.00% | 0.00% | 0.95% | 0.00% | 4.6% |
| every dataset | B | 0.60 | 0.96 | 0.47% | 0.00% | 0.00% | 0.95% | 0.00% | 4.6% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A (the rule's pick) | 0.79 | 0.4% | 97.2% | 4.9% | 7.0% |
| B | 0.95 | 0.4% | 97.2% | 4.9% | 7.0% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car.

## Negatives by group, prompts A at τ_plant 0.79

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 350 | 6.0% | 6.3% |
| outdoor | 236 | 8.5% | 13.6% |
| people_pets | 406 | 2.0% | 3.9% |
| unlabelled | 8 | 0.0% | 0.0% |

## No-leaf photos by problem, prompts A

| Problem | no leaf | rescued |
|---|---|---|
| corn_curvularia_leaf_spot | 1 | 1 |
| corn_northern_leaf_blight | 1 | 0 |
| healthy | 30 | 30 |
| tomato_bacterial_wilt | 32 | 32 |
| tomato_early_blight | 14 | 12 |
| tomato_late_blight | 3 | 3 |
| tomato_two_spotted_spider_mite | 25 | 25 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.4% | 97.2% | 13.2% | 14.9% |
| 0.60 | 0.4% | 97.2% | 10.2% | 12.0% |
| 0.70 | 0.4% | 97.2% | 7.5% | 9.3% |
| 0.80 | 0.5% | 96.2% | 4.7% | 6.9% |
| 0.90 | 0.7% | 94.3% | 2.9% | 5.2% |
| 0.95 | 1.3% | 89.6% | 1.7% | 4.1% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.2% | 98.1% | 32.7% | 33.8% |
| 0.60 | 0.2% | 98.1% | 25.4% | 26.8% |
| 0.70 | 0.2% | 98.1% | 20.4% | 22.0% |
| 0.80 | 0.2% | 98.1% | 15.3% | 17.0% |
| 0.90 | 0.2% | 98.1% | 9.0% | 10.8% |
| 0.95 | 0.4% | 97.2% | 4.9% | 7.0% |
