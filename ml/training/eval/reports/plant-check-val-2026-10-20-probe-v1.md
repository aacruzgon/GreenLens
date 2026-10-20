# The plant check on the val split (plant-check-val-2026-10-20-probe-v1)

> Trained probe plant-probe-v1-2026-10-20 beside the zero-shot prompt sets; negatives: the half of COCO val2017's the probe didn't train on.

1039 plant photos (the split) and 2386 photos without a plant (`negatives_tune.tsv`: COCO without potted plant or broccoli). Probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: manifest.cpu.toml's leaf_detector. The manifest's `τ_leaf` (0.4) is used by the leaf-score-alone line and by the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, kept for the record. The pairs to choose from are the joint searches (below). "Prompts" names the probe's prompt set (A or B, listed below).

**Leaf score alone at the manifest's `τ_leaf` 0.4:** false `no_plant_found` 1.9% (20 no-leaf photos); negatives passing 4.7%.

## Leaf score alone

| τ_leaf | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|
| 0.35 | 1.44% | 0.00% | 0.89% | 3.32% | 1.23% | 5.9% |
| 0.40 | 1.92% | 0.00% | 0.89% | 4.83% | 1.23% | 4.7% |
| 0.50 | 3.08% | 0.00% | 1.18% | 7.85% | 2.47% | 2.8% |
| 0.70 | 11.26% | 1.73% | 5.33% | 25.98% | 9.88% | 1.1% |
| 0.90 | 45.91% | 16.61% | 48.22% | 65.56% | 60.49% | 0.0% |

## Fixed pairs

False `no_plant_found` on plant photos, pooled and per dataset, and photos without a plant passing (models.md 2's target: under 1% and at most 10%). The manifest pair is the current detector's, chosen on the val split with it; on another detector or split it is only a reference.

| Pair | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass |
|---|---|---|---|---|---|---|---|---|---|
| manifest pair (current detector's) | A | 0.40 | 0.70 | 0.19% | 0.00% | 0.00% | 0.60% | 0.00% | 12.3% |

Photos without a plant passing, by group:

| Pair | indoor (n 890) | outdoor (n 559) | people_pets (n 910) | unlabelled (n 27) |
|---|---|---|---|---|
| manifest pair (current detector's) | 9.9% | 18.6% | 10.4% | 25.9% |

## Joint search: both thresholds against models.md 2's target

Pooled: false `no_plant_found` at most 1% over all plant photos. Every dataset: at most 1% on each dataset, so the field photos can't hide behind the web photos.

| Constraint | Prompts | τ_leaf | τ_plant | false no_plant | ibean | plantdoc | tom2024 | tomato_pakistan_2026 | negatives pass: check |
|---|---|---|---|---|---|---|---|---|---|
| pooled | A | 0.55 | 0.96 | 0.96% | 0.00% | 0.30% | 2.72% | 0.00% | 3.9% |
| pooled | B | 0.53 | 0.99 | 0.96% | 0.00% | 0.00% | 3.02% | 0.00% | 3.8% |
| pooled | plant-probe-v1-2026-10-20 | 0.99 | 0.93 | 0.58% | 0.00% | 1.18% | 0.60% | 0.00% | 0.0% |
| every dataset | A | 0.60 | 0.78 | 0.29% | 0.00% | 0.00% | 0.91% | 0.00% | 7.8% |
| every dataset | B | 0.42 | 0.97 | 0.29% | 0.00% | 0.00% | 0.91% | 0.00% | 7.3% |
| every dataset | plant-probe-v1-2026-10-20 | 0.99 | 0.71 | 0.10% | 0.00% | 0.00% | 0.30% | 0.00% | 0.1% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Prompts | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A | 0.85 | 0.4% | 80.0% | 4.9% | 8.6% |
| B | 0.96 | 0.3% | 85.0% | 4.8% | 8.7% |
| plant-probe-v1-2026-10-20 (the rule's pick) | 0.04 | 0.0% | 100.0% | 3.9% | 7.7% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car; plant-probe-v1-2026-10-20: trained probe plant-probe-v1-2026-10-20.

## Negatives by group, prompts plant-probe-v1-2026-10-20 at τ_plant 0.04

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 890 | 2.4% | 3.8% |
| outdoor | 559 | 10.7% | 17.2% |
| people_pets | 910 | 1.3% | 5.4% |
| unlabelled | 27 | 3.7% | 14.8% |

## No-leaf photos by problem, prompts plant-probe-v1-2026-10-20

| Problem | no leaf | rescued |
|---|---|---|
| corn_curvularia_leaf_spot | 7 | 7 |
| corn_gray_leaf_spot | 1 | 1 |
| corn_northern_leaf_blight | 3 | 3 |
| healthy | 4 | 4 |
| tomato_late_blight | 1 | 1 |
| tomato_spider_mites | 3 | 3 |
| tomato_yellow_leaf_curl_virus | 1 | 1 |

## Sweep, prompts A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.1% | 95.0% | 15.3% | 18.3% |
| 0.60 | 0.2% | 90.0% | 12.2% | 15.4% |
| 0.70 | 0.2% | 90.0% | 9.0% | 12.3% |
| 0.80 | 0.4% | 80.0% | 6.0% | 9.7% |
| 0.90 | 0.4% | 80.0% | 3.6% | 7.5% |
| 0.95 | 0.7% | 65.0% | 2.2% | 6.4% |

## Sweep, prompts B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.0% | 100.0% | 34.3% | 36.4% |
| 0.60 | 0.0% | 100.0% | 28.2% | 30.4% |
| 0.70 | 0.1% | 95.0% | 22.1% | 24.6% |
| 0.80 | 0.1% | 95.0% | 16.3% | 19.1% |
| 0.90 | 0.1% | 95.0% | 9.6% | 13.0% |
| 0.95 | 0.3% | 85.0% | 5.5% | 9.3% |

## Sweep, prompts plant-probe-v1-2026-10-20

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.0% | 100.0% | 0.2% | 4.8% |
| 0.60 | 0.0% | 100.0% | 0.2% | 4.8% |
| 0.70 | 0.0% | 100.0% | 0.1% | 4.7% |
| 0.80 | 0.0% | 100.0% | 0.1% | 4.7% |
| 0.90 | 0.1% | 95.0% | 0.1% | 4.7% |
| 0.95 | 0.2% | 90.0% | 0.0% | 4.7% |
