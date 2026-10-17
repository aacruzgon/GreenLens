# The plant check on the val split (plant-check-val-2026-10-16-detector-bg)

1152 plant photos (the split) and 4758 photos without a plant (COCO val2017 without potted plant or broccoli). `τ_leaf` 0.9; probe = zero-shot BioCLIP 2 at logit scale 100 on the fp16 embedding (b9016a6865e188c0); leaf detector: ml/models/files/leaf_detector/rfdetr-s-leaf-2026-10-16c-bg-fp32.onnx. The manifest's pair comes from the joint search (below); the rule fixed before measuring, tau_plant = lowest value with the probe alone passing <= 5% of negatives, is kept for the record.

**Leaf score alone:** false `no_plant_found` 41.8% (481 no-leaf photos); negatives passing 0.1%.

## Shipped: set A, τ_leaf 0.90, τ_plant 0.67

False `no_plant_found` on plant photos 0.6%; photos without a plant passing 10.0% (models.md 2's target: under 1% and at most 10%).

| Group | n | pass: check |
|---|---|---|
| indoor | 1722 | 10.4% |
| outdoor | 1152 | 13.6% |
| people_pets | 1836 | 7.4% |
| unlabelled | 48 | 12.5% |

## Joint search: both thresholds against models.md 2's target

| Set | τ_leaf | τ_plant | false no_plant | negatives pass: check |
|---|---|---|---|---|
| A | 0.48 | 0.96 | 1.0% | 4.3% |
| B | 0.64 | 0.97 | 1.0% | 4.8% |

## The rule fixed before measuring: τ_leaf kept, τ_plant at 5% of negatives

| Set | τ_plant | false no_plant (check) | no-leaf rescued | negatives pass: probe | negatives pass: check |
|---|---|---|---|---|---|
| A (the rule's pick) | 0.85 | 2.5% | 94.0% | 4.9% | 5.0% |
| B | 0.96 | 3.0% | 92.9% | 4.8% | 4.9% |

Prompt sets: A: plant = plant, leaves, garden plant; not = wall, person, pet, room, hand, car; B: plant = plant, leaves, garden plant, wilted plant, dying plant, leaf, tree; not = wall, person, pet, room, hand, car.

## Negatives by group, set A at τ_plant 0.85

| Group | n | pass: probe | pass: check |
|---|---|---|---|
| indoor | 1722 | 5.2% | 5.2% |
| outdoor | 1152 | 7.0% | 7.2% |
| people_pets | 1836 | 3.3% | 3.4% |
| unlabelled | 48 | 6.2% | 6.2% |

## No-leaf photos by problem, set A

| Problem | no leaf | rescued |
|---|---|---|
| apple_cedar_apple_rust | 4 | 4 |
| apple_scab | 4 | 4 |
| bean_angular_leaf_spot | 5 | 5 |
| bean_rust | 21 | 21 |
| corn_common_rust | 4 | 4 |
| corn_curvularia_leaf_spot | 6 | 4 |
| corn_gray_leaf_spot | 2 | 2 |
| corn_northern_leaf_blight | 13 | 10 |
| healthy | 227 | 211 |
| pepper_bacterial_leaf_spot | 6 | 6 |
| potato_early_blight | 1 | 1 |
| potato_late_blight | 9 | 8 |
| squash_powdery_mildew | 6 | 6 |
| tomato_bacterial_spot | 4 | 4 |
| tomato_bacterial_wilt | 42 | 41 |
| tomato_early_blight | 42 | 38 |
| tomato_late_blight | 10 | 9 |
| tomato_leaf_mold | 22 | 21 |
| tomato_mosaic_virus | 3 | 3 |
| tomato_septoria_leaf_spot | 12 | 12 |
| tomato_two_spotted_spider_mite | 25 | 25 |
| tomato_yellow_leaf_curl_virus | 13 | 13 |

## Sweep, set A

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.4% | 99.0% | 14.9% | 15.0% |
| 0.60 | 0.5% | 98.8% | 12.0% | 12.1% |
| 0.70 | 1.0% | 97.5% | 9.0% | 9.0% |
| 0.80 | 1.6% | 96.3% | 6.2% | 6.3% |
| 0.90 | 3.5% | 91.7% | 3.7% | 3.7% |
| 0.95 | 6.6% | 84.2% | 2.3% | 2.4% |

## Sweep, set B

| τ_plant | false no_plant | no-leaf rescued | negatives: probe | negatives: check |
|---|---|---|---|---|
| 0.50 | 0.3% | 99.4% | 34.4% | 34.5% |
| 0.60 | 0.3% | 99.4% | 28.2% | 28.2% |
| 0.70 | 0.3% | 99.2% | 22.2% | 22.2% |
| 0.80 | 0.5% | 98.8% | 16.2% | 16.3% |
| 0.90 | 1.0% | 97.5% | 10.0% | 10.0% |
| 0.95 | 2.2% | 94.8% | 5.7% | 5.7% |
