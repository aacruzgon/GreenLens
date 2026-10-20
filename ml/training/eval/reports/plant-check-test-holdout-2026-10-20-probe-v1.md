# The plant check on the test split (plant-check-test-holdout-2026-10-20-probe-v1)

> Final check of the pairs chosen on val: the test split and COCO train2017's hold-out.

763 plant photos (the split) and 1000 photos without a plant (`negatives_coco_train_holdout.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: manifest.cpu.toml's leaf_detector. The manifest's `τ_leaf` (0.4) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.4:** false `no_plant_found` 4.3% (33 no-leaf photos); negatives passing 4.5%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|
| 0.35 | 2.62% | 0.00% | 0.00% | 6.08% | 0.00% | 6.6% |
| 0.40 | 4.33% | 0.00% | 0.00% | 10.03% | 0.00% | 4.5% |
| 0.50 | 7.99% | 0.00% | 0.44% | 17.93% | 1.25% | 2.6% |
| 0.70 | 19.27% | 0.00% | 3.10% | 40.43% | 8.75% | 0.8% |
| 0.90 | 42.73% | 17.19% | 20.80% | 64.74% | 55.00% | 0.0% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The manifest pair is the current detector's, chosen on the val split with it; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| manifest pair (current detector's) | A | 0.40 | 0.70 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 11.0% |
| plant-probe-v1-2026-10-20 0.99 / 0.71 | plant-probe-v1-2026-10-20 | 0.99 | 0.71 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.1% |

Photos without a plant passing, by group:

| Pair | indoor (n 350) | outdoor (n 236) | people_pets (n 406) | unlabelled (n 8) |
|---|---|---|---|---|
| manifest pair (current detector's) | 9.4% | 18.6% | 8.1% | 0.0% |
| plant-probe-v1-2026-10-20 0.99 / 0.71 | 0.0% | 0.0% | 0.2% | 0.0% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.60 | 0.96 | 0.79% | 0.00% | 0.00% | 1.82% | 0.00% | 2.4% |
| pooled | B | 0.53 | 0.99 | 0.92% | 0.00% | 0.00% | 2.13% | 0.00% | 3.0% |
| pooled | plant-probe-v1-2026-10-20 | 0.99 | 0.89 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.0% |
| every dataset | A | 0.60 | 0.93 | 0.39% | 0.00% | 0.00% | 0.91% | 0.00% | 3.2% |
| every dataset | B | 0.56 | 0.98 | 0.39% | 0.00% | 0.00% | 0.91% | 0.00% | 3.6% |
| every dataset | plant-probe-v1-2026-10-20 | 0.99 | 0.89 | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.0% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A (the rule's pick) | 0.79 | 0.0% | 100.0% | 4.9% | 8.7% |
| B | 0.95 | 0.0% | 100.0% | 4.9% | 8.7% |
| plant-probe-v1-2026-10-20 | 0.04 | 0.0% | 100.0% | 4.5% | 8.0% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car; plant-probe-v1-2026-10-20: trained probe plant-probe-v1-2026-10-20.

## Negatives by group, prompts A at τ_plant 0.79

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 350 | 6.0% | 6.9% |
| outdoor | 236 | 8.5% | 16.1% |
| people_pets | 406 | 2.0% | 6.2% |
| unlabelled | 8 | 0.0% | 0.0% |

## No-leaf photos by problem, prompts A

| Problem | no leaf | rescued |
|---|---|---|
| corn_curvularia_leaf_spot | 1 | 1 |
| healthy | 16 | 16 |
| tomato_late_blight | 1 | 1 |
| tomato_spider_mites | 15 | 15 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.0% | 100.0% | 13.2% | 16.6% |
| 0.60 | 0.0% | 100.0% | 10.2% | 13.7% |
| 0.70 | 0.0% | 100.0% | 7.5% | 11.0% |
| 0.80 | 0.0% | 100.0% | 4.7% | 8.6% |
| 0.90 | 0.1% | 97.0% | 2.9% | 6.9% |
| 0.95 | 0.1% | 97.0% | 1.7% | 5.8% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.0% | 100.0% | 32.7% | 34.7% |
| 0.60 | 0.0% | 100.0% | 25.4% | 28.0% |
| 0.70 | 0.0% | 100.0% | 20.4% | 23.4% |
| 0.80 | 0.0% | 100.0% | 15.3% | 18.5% |
| 0.90 | 0.0% | 100.0% | 9.0% | 12.5% |
| 0.95 | 0.0% | 100.0% | 4.9% | 8.7% |

## Sweep, prompts plant-probe-v1-2026-10-20

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.0% | 100.0% | 0.3% | 4.6% |
| 0.60 | 0.0% | 100.0% | 0.2% | 4.6% |
| 0.70 | 0.0% | 100.0% | 0.1% | 4.6% |
| 0.80 | 0.0% | 100.0% | 0.1% | 4.6% |
| 0.90 | 0.0% | 100.0% | 0.0% | 4.5% |
| 0.95 | 0.0% | 100.0% | 0.0% | 4.5% |
