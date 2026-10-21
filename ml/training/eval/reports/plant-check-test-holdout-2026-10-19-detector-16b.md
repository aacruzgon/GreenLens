# The plant check on the test split (plant-check-test-holdout-2026-10-19-detector-16b)

> The current detector, `rfdetr-s-leaf-2026-10-16b` (manifest), trained with no background photos, on the same photos for comparison.

854 plant photos (the split) and 1000 photos without a plant (`negatives_coco_train_holdout.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: manifest.cpu.toml's leaf_detector. The manifest's `τ_leaf` (0.9) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.9:** false `no_plant_found` 47.4% (405 no-leaf photos); negatives passing 1.2%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|
| 0.35 | 6.79% | 0.00% | 0.00% | 13.81% | 0.00% | 52.5% |
| 0.50 | 12.65% | 0.00% | 0.00% | 25.24% | 2.50% | 34.0% |
| 0.70 | 21.31% | 0.00% | 0.00% | 41.43% | 10.00% | 16.2% |
| 0.90 | 47.42% | 9.38% | 15.04% | 75.71% | 51.25% | 1.2% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The manifest pair is the current detector's, chosen on the val split with it; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| manifest pair (current detector's) | A | 0.90 | 0.67 | 1.05% | 0.00% | 0.00% | 2.14% | 0.00% | 9.3% |
| A 0.48 / 0.96 | A | 0.48 | 0.96 | 1.99% | 0.00% | 0.00% | 4.05% | 0.00% | 36.8% |
| A 0.50 / 0.88 | A | 0.50 | 0.88 | 0.82% | 0.00% | 0.00% | 1.67% | 0.00% | 36.3% |
| A 0.50 / 0.96 | A | 0.50 | 0.96 | 1.99% | 0.00% | 0.00% | 4.05% | 0.00% | 34.9% |
| B 0.40 / 0.97 | B | 0.40 | 0.97 | 0.82% | 0.00% | 0.00% | 1.67% | 0.00% | 48.0% |
| B 0.52 / 0.98 | B | 0.52 | 0.98 | 1.52% | 0.00% | 0.00% | 3.10% | 0.00% | 33.6% |

Photos without a plant passing, by group:

| Pair | indoor (n 350) | outdoor (n 236) | people_pets (n 406) | unlabelled (n 8) |
|---|---|---|---|---|
| manifest pair (current detector's) | 10.0% | 15.7% | 5.2% | 0.0% |
| A 0.48 / 0.96 | 38.0% | 50.0% | 28.3% | 25.0% |
| A 0.50 / 0.88 | 37.4% | 50.8% | 27.1% | 25.0% |
| A 0.50 / 0.96 | 36.0% | 48.3% | 26.4% | 25.0% |
| B 0.40 / 0.97 | 48.9% | 63.6% | 37.7% | 75.0% |
| B 0.52 / 0.98 | 34.6% | 46.6% | 25.1% | 37.5% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.92 | 0.66 | 0.94% | 0.00% | 0.00% | 1.90% | 0.00% | 9.1% |
| pooled | B | 0.99 | 0.90 | 0.94% | 0.00% | 0.00% | 1.90% | 0.00% | 9.0% |
| every dataset | A | 0.81 | 0.66 | 0.47% | 0.00% | 0.00% | 0.95% | 0.00% | 15.8% |
| every dataset | B | 0.99 | 0.85 | 0.47% | 0.00% | 0.00% | 0.95% | 0.00% | 11.8% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A (the rule's pick) | 0.79 | 1.5% | 96.8% | 4.9% | 5.9% |
| B | 0.95 | 2.3% | 95.1% | 4.9% | 6.0% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car.

## Negatives by group, prompts A at τ_plant 0.79

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 350 | 6.0% | 6.9% |
| outdoor | 236 | 8.5% | 10.2% |
| people_pets | 406 | 2.0% | 2.7% |
| unlabelled | 8 | 0.0% | 0.0% |

## No-leaf photos by problem, prompts A

| Problem | no leaf | rescued |
|---|---|---|
| apple_scab | 1 | 1 |
| bean_angular_leaf_spot | 2 | 2 |
| bean_rust | 6 | 6 |
| corn_common_rust | 1 | 1 |
| corn_curvularia_leaf_spot | 11 | 11 |
| corn_northern_leaf_blight | 7 | 5 |
| healthy | 180 | 177 |
| pepper_bacterial_leaf_spot | 1 | 1 |
| potato_early_blight | 1 | 1 |
| potato_late_blight | 2 | 2 |
| squash_powdery_mildew | 1 | 1 |
| tomato_bacterial_spot | 1 | 1 |
| tomato_bacterial_wilt | 41 | 41 |
| tomato_early_blight | 45 | 39 |
| tomato_late_blight | 19 | 17 |
| tomato_leaf_mold | 18 | 18 |
| tomato_mosaic_virus | 4 | 4 |
| tomato_septoria_leaf_spot | 10 | 10 |
| tomato_two_spotted_spider_mite | 44 | 44 |
| tomato_yellow_leaf_curl_virus | 10 | 10 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.7% | 98.5% | 13.2% | 14.1% |
| 0.60 | 0.9% | 98.0% | 10.2% | 11.1% |
| 0.70 | 1.2% | 97.5% | 7.5% | 8.4% |
| 0.80 | 1.5% | 96.8% | 4.7% | 5.8% |
| 0.90 | 3.2% | 93.3% | 2.9% | 4.1% |
| 0.95 | 6.8% | 85.7% | 1.7% | 2.9% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.5% | 99.0% | 32.7% | 33.6% |
| 0.60 | 0.5% | 99.0% | 25.4% | 26.3% |
| 0.70 | 0.5% | 99.0% | 20.4% | 21.3% |
| 0.80 | 0.5% | 99.0% | 15.3% | 16.2% |
| 0.90 | 0.8% | 98.3% | 9.0% | 10.0% |
| 0.95 | 2.3% | 95.1% | 4.9% | 6.0% |
