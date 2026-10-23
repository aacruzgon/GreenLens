# The plant check on the val split (plant-check-val-2026-10-23-probe-v2)

> Trained probe plant-probe-v2-2026-10-23 (licence-clean negatives) beside the zero-shot prompt sets; negatives: the half of COCO val2017's v1 and v2 didn't train on.

Made with `python -m ml.training.eval.plant_check --split val --negatives ml/training/runs/heads/plant-probe-v2-2026-10-23/negatives_tune.tsv --probe ml/training/runs/heads/plant-probe-v2-2026-10-23/plant-probe-v2-2026-10-23.json --pair A:0.40:0.70 --label plant-check-val-2026-10-23-probe-v2 --note 'Trained probe plant-probe-v2-2026-10-23 (licence-clean negatives) beside the zero-shot prompt sets; negatives: the half of COCO val2017'"'"'s v1 and v2 didn'"'"'t train on.'` on 2026-10-23, inputs `ml/training/data/splits/val.tsv` (SHA-256 `43ab87f1c8dc7b8d`); `ml/training/runs/heads/plant-probe-v2-2026-10-23/negatives_tune.tsv` (SHA-256 `37b2adee3537ce77`); `ml/training/runs/heads/plant-probe-v2-2026-10-23/plant-probe-v2-2026-10-23.json` (SHA-256 `459b1b6dbbfaa3c1`); `ml/training/data/splits/inat_species.tsv` (SHA-256 `c35da0db1586b800`).

1564 plant photos (the split) and 2386 photos without a plant (`negatives_tune.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: manifest.cpu.toml's leaf_detector. The manifest's `τ_leaf` (0.99) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below). Whole plants: the 503 photos of inat_species.tsv, split val, a column of their own, outside the pooled and per-dataset rates.

**Leaf score alone at the manifest's `τ_leaf` 0.99:** false `no_plant_found` 100.0% (1564 no-leaf photos); negatives passing 0.0%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | maize_makerere | maize_pretoria | plantcity | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| 0.35 | 1.02% | 0.00% | 0.54% | 0.00% | 0.00% | 0.89% | 3.32% | 1.23% | 5.9% |
| 0.50 | 2.37% | 0.00% | 0.54% | 2.29% | 0.00% | 1.18% | 7.85% | 2.47% | 2.8% |
| 0.70 | 8.82% | 1.73% | 1.63% | 5.71% | 4.82% | 5.33% | 25.98% | 9.88% | 1.1% |
| 0.90 | 36.89% | 16.61% | 10.33% | 20.00% | 27.71% | 48.22% | 65.56% | 60.49% | 0.0% |
| 0.99 | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 100.00% | 0.0% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The first row is the zero-shot pair the manifest shipped until M17, chosen on the val split with the current detector; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | maize_makerere | maize_pretoria | plantcity | plantdoc | tom2024 | tomato_pakistan_2026 | whole plants | negatives pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| zero-shot pair A 0.40 / 0.70 (shipped until M17) | A | 0.40 | 0.70 | 0.13% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.60% | 0.00% | 2.19% | 12.3% |
| A 0.40 / 0.70 | A | 0.40 | 0.70 | 0.13% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.60% | 0.00% | 2.19% | 12.3% |

Photos without a plant passing, by group:

| Pair | indoor (n 890) | outdoor (n 559) | people_pets (n 910) | unlabelled (n 27) |
|---|---|---|---|---|
| zero-shot pair A 0.40 / 0.70 (shipped until M17) | 9.9% | 18.6% | 10.4% | 25.9% |
| A 0.40 / 0.70 | 9.9% | 18.6% | 10.4% | 25.9% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos. Both also skip any pair failing more of the 503 whole-plant photos (inat_species.tsv, split val) than the zero-shot pair does: 2.19%.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | maize_makerere | maize_pretoria | plantcity | plantdoc | tom2024 | tomato_pakistan_2026 | whole plants | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.35 | 0.80 | 0.26% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 1.21% | 0.00% | 2.19% | 10.9% |
| pooled | B | 0.42 | 0.94 | 0.19% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.91% | 0.00% | 2.19% | 9.4% |
| pooled | plant-probe-v2-2026-10-23 | 0.99 | 0.74 | 0.13% | 0.00% | 0.00% | 0.00% | 0.00% | 0.30% | 0.30% | 0.00% | 2.19% | 0.1% |
| every dataset | A | 0.44 | 0.72 | 0.13% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.60% | 0.00% | 2.19% | 11.1% |
| every dataset | B | 0.42 | 0.94 | 0.19% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.91% | 0.00% | 2.19% | 9.4% |
| every dataset | plant-probe-v2-2026-10-23 | 0.99 | 0.74 | 0.13% | 0.00% | 0.00% | 0.00% | 0.00% | 0.30% | 0.30% | 0.00% | 2.19% | 0.1% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A | 0.85 | 8.9% | 91.1% | 4.9% | 4.9% |
| B | 0.96 | 6.5% | 93.5% | 4.8% | 4.8% |
| plant-probe-v2-2026-10-23 (the rule's pick) | 0.06 | 0.0% | 100.0% | 4.8% | 4.8% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car; plant-probe-v2-2026-10-23: trained probe plant-probe-v2-2026-10-23.

## Negatives by group, prompts plant-probe-v2-2026-10-23 at τ_plant 0.06

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 890 | 3.0% | 3.0% |
| outdoor | 559 | 12.9% | 12.9% |
| people_pets | 910 | 1.6% | 1.6% |
| unlabelled | 27 | 3.7% | 3.7% |

## No-leaf photos by problem, prompts plant-probe-v2-2026-10-23

| Problem | no leaf | rescued |
|---|---|---|
| apple_cedar_apple_rust | 12 | 12 |
| apple_scab | 12 | 12 |
| bean_angular_leaf_spot | 96 | 96 |
| bean_rust | 97 | 97 |
| corn_common_rust | 36 | 36 |
| corn_curvularia_leaf_spot | 38 | 38 |
| corn_gray_leaf_spot | 107 | 107 |
| corn_northern_leaf_blight | 138 | 138 |
| grape_black_rot | 8 | 8 |
| healthy | 629 | 629 |
| pepper_bacterial_leaf_spot | 8 | 8 |
| potato_early_blight | 14 | 14 |
| potato_late_blight | 13 | 13 |
| squash_powdery_mildew | 18 | 18 |
| tomato_bacterial_spot | 39 | 39 |
| tomato_early_blight | 37 | 37 |
| tomato_late_blight | 38 | 38 |
| tomato_leaf_mold | 67 | 67 |
| tomato_mosaic_virus | 6 | 6 |
| tomato_septoria_leaf_spot | 47 | 47 |
| tomato_spider_mites | 70 | 70 |
| tomato_yellow_leaf_curl_virus | 34 | 34 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | whole plants | negatives: probe | negatives: check |
|---|---|---|---|---|---|
| 0.50 | 1.0% | 99.0% | 3.58% | 15.3% | 15.3% |
| 0.60 | 1.7% | 98.3% | 4.17% | 12.2% | 12.2% |
| 0.70 | 3.3% | 96.7% | 6.76% | 9.0% | 9.0% |
| 0.80 | 5.8% | 94.2% | 9.15% | 6.0% | 6.0% |
| 0.90 | 12.0% | 88.0% | 14.51% | 3.6% | 3.6% |
| 0.95 | 21.0% | 79.0% | 23.46% | 2.2% | 2.2% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | whole plants | negatives: probe | negatives: check |
|---|---|---|---|---|---|
| 0.50 | 0.1% | 99.9% | 0.99% | 34.3% | 34.3% |
| 0.60 | 0.1% | 99.9% | 1.19% | 28.2% | 28.2% |
| 0.70 | 0.1% | 99.9% | 1.99% | 22.1% | 22.1% |
| 0.80 | 0.3% | 99.7% | 3.38% | 16.3% | 16.3% |
| 0.90 | 1.7% | 98.3% | 6.56% | 9.6% | 9.6% |
| 0.95 | 4.9% | 95.1% | 10.54% | 5.5% | 5.5% |

## Sweep, prompts plant-probe-v2-2026-10-23

| τ_plant | false no_plant | no-leaf rescued | whole plants | negatives: probe | negatives: check |
|---|---|---|---|---|---|
| 0.50 | 0.1% | 99.9% | 1.19% | 0.5% | 0.5% |
| 0.60 | 0.1% | 99.9% | 1.59% | 0.3% | 0.3% |
| 0.70 | 0.1% | 99.9% | 1.79% | 0.2% | 0.2% |
| 0.80 | 0.2% | 99.8% | 3.38% | 0.1% | 0.1% |
| 0.90 | 0.4% | 99.6% | 5.96% | 0.0% | 0.0% |
| 0.95 | 1.2% | 98.8% | 10.93% | 0.0% | 0.0% |
