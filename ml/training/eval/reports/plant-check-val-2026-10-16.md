# The plant check on the val split (plant-check-val-2026-10-16)

1152 plant photos (the split) and 4758 photos without a plant (COCO val2017 without potted plant or broccoli). `τ_leaf` 0.35; probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0). The manifest's pair comes from the joint search (below); the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, is kept for the record.

**Leaf score alone:** false `no_plant_found` 3.0% (34 no-leaf photos); negatives passing 51.9%.

## Shipped: set A, τ_leaf 0.90, τ_plant 0.67

False `no_plant_found` on plant photos 0.8%; photos without a plant passing 11.4% (models.md 2's target: under 1% and at most 10%).

| Group | n | pass: check |
|---|---|---|
| indoor | 1722 | 11.7% |
| outdoor | 1152 | 16.3% |
| people_pets | 1836 | 8.1% |
| unlabelled | 48 | 12.5% |

## Joint search: both thresholds against models.md 2's target

| Set | τ_leaf | τ_plant | false no_plant | negatives pass: check |
|---|---|---|---|---|
| A | 0.91 | 0.67 | 1.0% | 11.0% |
| B | 0.99 | 0.87 | 0.8% | 11.9% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Set | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A (the rule's pick) | 0.85 | 0.5% | 82.4% | 4.9% | 53.7% |
| B | 0.96 | 0.5% | 82.4% | 4.8% | 53.9% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car.

## Negatives by group, set A at τ_plant 0.85

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 1722 | 5.2% | 57.0% |
| outdoor | 1152 | 7.0% | 68.9% |
| people_pets | 1836 | 3.3% | 41.1% |
| unlabelled | 48 | 6.2% | 56.2% |

## No-leaf photos by problem, set A

| Problem | no leaf | rescued |
|---|---|---|
| corn_curvularia_leaf_spot | 2 | 1 |
| corn_northern_leaf_blight | 1 | 1 |
| healthy | 5 | 5 |
| tomato_bacterial_wilt | 14 | 13 |
| tomato_early_blight | 10 | 7 |
| tomato_late_blight | 1 | 0 |
| tomato_two_spotted_spider_mite | 1 | 1 |

## Sweep, set A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.3% | 88.2% | 14.9% | 58.5% |
| 0.60 | 0.3% | 88.2% | 12.0% | 57.0% |
| 0.70 | 0.3% | 88.2% | 9.0% | 55.5% |
| 0.80 | 0.5% | 82.4% | 6.2% | 54.4% |
| 0.90 | 0.6% | 79.4% | 3.7% | 53.1% |
| 0.95 | 1.0% | 67.6% | 2.3% | 52.5% |

## Sweep, set B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.3% | 91.2% | 34.4% | 68.3% |
| 0.60 | 0.3% | 91.2% | 28.2% | 65.1% |
| 0.70 | 0.3% | 88.2% | 22.2% | 62.3% |
| 0.80 | 0.3% | 88.2% | 16.2% | 59.6% |
| 0.90 | 0.4% | 85.3% | 10.0% | 56.3% |
| 0.95 | 0.5% | 82.4% | 5.7% | 54.2% |
