# The plant check on the test split (plant-check-test-holdout-2026-10-23-probe-v2)

> Final check of the pairs chosen on val: the test split and COCO train2017's hold-out (v1 was checked on it once on Oct 20).

Made with `python -m ml.training.eval.plant_check --split test --negatives ml/training/data/splits/negatives_coco_train_holdout.tsv --probe ml/training/runs/heads/plant-probe-v2-2026-10-23/plant-probe-v2-2026-10-23.json --pair A:0.40:0.70 --pair plant-probe-v2-2026-10-23:0.99:0.74 --label plant-check-test-holdout-2026-10-23-probe-v2 --note 'Final check of the pairs chosen on val: the test split and COCO train2017'"'"'s hold-out (v1 was checked on it once on Oct 20).'` on 2026-10-23, inputs `ml/training/data/splits/test.tsv` (SHA-256 `8c8748a995c4d2a8`); `ml/training/data/splits/negatives_coco_train_holdout.tsv` (SHA-256 `e4a2f38e476a594d`); `ml/training/runs/heads/plant-probe-v2-2026-10-23/plant-probe-v2-2026-10-23.json` (SHA-256 `459b1b6dbbfaa3c1`).

1276 plant photos (the split) and 1000 photos without a plant (`negatives_coco_train_holdout.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: manifest.cpu.toml's leaf_detector. The manifest's `τ_leaf` (0.99) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.99:** false `no_plant_found` 100.0% (1276 no-leaf photos); negatives passing 0.0%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | maize_makerere | maize_pretoria | plantcity | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| 0.35 | 1.57% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 6.08% | 0.00% | 6.6% |
| 0.50 | 4.86% | 0.00% | 0.00% | 0.56% | 0.00% | 0.44% | 17.93% | 1.25% | 2.6% |
| 0.70 | 12.54% | 0.00% | 0.00% | 3.37% | 4.58% | 3.10% | 40.43% | 8.75% | 0.8% |
| 0.90 | 32.21% | 17.19% | 9.89% | 12.36% | 29.41% | 20.80% | 64.74% | 55.00% | 0.0% |
| 0.99 | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 0.0% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The first row is the zero-shot pair the manifest shipped until M17, chosen on the val split with the current detector; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | maize_makerere | maize_pretoria | plantcity | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| zero-shot pair A 0.40 / 0.70 (shipped until M17) | A | 0.40 | 0.70 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 11.0% |
| A 0.40 / 0.70 | A | 0.40 | 0.70 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 11.0% |
| plant-probe-v2-2026-10-23 0.99 / 0.74 | plant-probe-v2-2026-10-23 | 0.99 | 0.74 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.0% |

Photos without a plant passing, by group:

| Pair | indoor (n 350) | outdoor (n 236) | people_pets (n 406) | unlabelled (n 8) |
|---|---|---|---|---|
| zero-shot pair A 0.40 / 0.70 (shipped until M17) | 9.4% | 18.6% | 8.1% | 0.0% |
| A 0.40 / 0.70 | 9.4% | 18.6% | 8.1% | 0.0% |
| plant-probe-v2-2026-10-23 0.99 / 0.74 | 0.0% | 0.0% | 0.0% | 0.0% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | maize_makerere | maize_pretoria | plantcity | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.55 | 0.98 | 0.86% | 0.00% | 0.00% | 0.00% | 0.65% | 0.00% | 2.74% | 1.25% | 2.2% |
| pooled | B | 0.56 | 0.99 | 0.94% | 0.00% | 0.00% | 0.56% | 0.65% | 0.00% | 3.04% | 0.00% | 2.4% |
| pooled | plant-probe-v2-2026-10-23 | 0.99 | 0.63 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.0% |
| every dataset | A | 0.59 | 0.93 | 0.39% | 0.00% | 0.00% | 0.56% | 0.65% | 0.00% | 0.91% | 0.00% | 3.2% |
| every dataset | B | 0.56 | 0.98 | 0.31% | 0.00% | 0.00% | 0.00% | 0.65% | 0.00% | 0.91% | 0.00% | 3.6% |
| every dataset | plant-probe-v2-2026-10-23 | 0.99 | 0.63 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.0% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A | 0.79 | 5.5% | 94.5% | 4.9% | 4.9% |
| B | 0.95 | 4.7% | 95.3% | 4.9% | 4.9% |
| plant-probe-v2-2026-10-23 (the rule's pick) | 0.07 | 0.0% | 100.0% | 5.0% | 5.0% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car; plant-probe-v2-2026-10-23: trained probe plant-probe-v2-2026-10-23.

## Negatives by group, prompts plant-probe-v2-2026-10-23 at τ_plant 0.07

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 350 | 2.6% | 2.6% |
| outdoor | 236 | 13.6% | 13.6% |
| people_pets | 406 | 2.0% | 2.0% |
| unlabelled | 8 | 12.5% | 12.5% |

## No-leaf photos by problem, prompts plant-probe-v2-2026-10-23

| Problem | no leaf | rescued |
|---|---|---|
| apple_cedar_apple_rust | 10 | 10 |
| apple_scab | 10 | 10 |
| bean_angular_leaf_spot | 43 | 43 |
| bean_rust | 43 | 43 |
| corn_common_rust | 30 | 30 |
| corn_curvularia_leaf_spot | 34 | 34 |
| corn_gray_leaf_spot | 92 | 92 |
| corn_northern_leaf_blight | 137 | 137 |
| grape_black_rot | 8 | 8 |
| healthy | 517 | 517 |
| pepper_bacterial_leaf_spot | 9 | 9 |
| potato_early_blight | 6 | 6 |
| potato_late_blight | 5 | 5 |
| squash_powdery_mildew | 6 | 6 |
| tomato_bacterial_spot | 32 | 32 |
| tomato_early_blight | 35 | 35 |
| tomato_late_blight | 48 | 48 |
| tomato_leaf_mold | 60 | 60 |
| tomato_mosaic_virus | 10 | 10 |
| tomato_septoria_leaf_spot | 38 | 38 |
| tomato_spider_mites | 73 | 73 |
| tomato_yellow_leaf_curl_virus | 30 | 30 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.9% | 99.1% | 13.2% | 13.2% |
| 0.60 | 1.7% | 98.3% | 10.2% | 10.2% |
| 0.70 | 3.4% | 96.6% | 7.5% | 7.5% |
| 0.80 | 5.7% | 94.3% | 4.7% | 4.7% |
| 0.90 | 11.8% | 88.2% | 2.9% | 2.9% |
| 0.95 | 23.0% | 77.0% | 1.7% | 1.7% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.0% | 100.0% | 32.7% | 32.7% |
| 0.60 | 0.0% | 100.0% | 25.4% | 25.4% |
| 0.70 | 0.0% | 100.0% | 20.4% | 20.4% |
| 0.80 | 0.1% | 99.9% | 15.3% | 15.3% |
| 0.90 | 0.8% | 99.2% | 9.0% | 9.0% |
| 0.95 | 4.7% | 95.3% | 4.9% | 4.9% |

## Sweep, prompts plant-probe-v2-2026-10-23

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.0% | 100.0% | 0.2% | 0.2% |
| 0.60 | 0.0% | 100.0% | 0.2% | 0.2% |
| 0.70 | 0.0% | 100.0% | 0.0% | 0.0% |
| 0.80 | 0.0% | 100.0% | 0.0% | 0.0% |
| 0.90 | 0.2% | 99.8% | 0.0% | 0.0% |
| 0.95 | 1.0% | 99.0% | 0.0% | 0.0% |
