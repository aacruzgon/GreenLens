# VLM benchmark: 2026-10-20-whole-plant-only

| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| fp8 | 602,112 | 1 | 65/65 | 0 | 6.5 s | 8.3 s | 100% | 19% (n=26) | 3/11 / 2/15 | 0 | 100% (n=11) | 0% | 85% | other 14, chewed_margins 28, leaf_mines 20, shot_holes 2, ragged_holes 5 | 30 | 14,080 MiB |
| fp8 | 602,112 | 2 | 65/65 | 0 | 6.6 s | 8.7 s | 98% | 19% (n=26) | 3/11 / 2/15 | 0 | 100% (n=11) | 0% | 86% | other 16, chewed_margins 28, leaf_mines 12, shot_holes 18, ragged_holes 6 | 33 | 14,080 MiB |
| fp8 | 1,003,520 | 1 | 65/65 | 0 | 6.5 s | 9.1 s | 100% | 15% (n=26) | 3/11 / 1/15 | 0 | 100% (n=11) | 0% | 86% | other 22, chewed_margins 31, leaf_mines 13, shot_holes 4, ragged_holes 7, sooty_mould 1, skeletonised_leaves 1 | 19 | 14,170 MiB |
| fp8 | 1,003,520 | 2 | 65/65 | 0 | 6.8 s | 9.5 s | 97% | 23% (n=26) | 3/11 / 3/15 | 0 | 100% (n=11) | 0% | 86% | other 15, chewed_margins 31, leaf_mines 13, shot_holes 4, ragged_holes 7, sooty_mould 1, skeletonised_leaves 1 | 20 | 14,262 MiB |
| fp8 | 1,404,928 | 1 | 65/65 | 0 | 6.5 s | 9.3 s | 98% | 15% (n=26) | 3/11 / 1/15 | 1 | 100% (n=11) | 0% | 86% | other 16, chewed_margins 27, leaf_mines 15, shot_holes 2, ragged_holes 8, sooty_mould 1, frass 1, skeletonised_leaves 1 | 22 | 14,480 MiB |
| fp8 | 1,404,928 | 2 | 65/65 | 0 | 7.0 s | 10.1 s | 97% | 15% (n=26) | 3/11 / 1/15 | 1 | 100% (n=11) | 0% | 86% | other 17, chewed_margins 28, leaf_mines 22, shot_holes 11, ragged_holes 8, sooty_mould 1, frass 1, skeletonised_leaves 1 | 25 | 14,480 MiB |
| fp8 | 2,007,040 | 1 | 65/65 | 0 | 6.5 s | 8.4 s | 98% | 15% (n=26) | 3/11 / 1/15 | 0 | 100% (n=11) | 0% | 86% | other 14, chewed_margins 31, leaf_mines 11, shot_holes 4, ragged_holes 6 | 17 | 14,968 MiB |
| fp8 | 2,007,040 | 2 | 65/65 | 0 | 6.9 s | 9.7 s | 97% | 15% (n=26) | 3/11 / 1/15 | 1 | 100% (n=11) | 0% | 87% | other 14, chewed_margins 30, leaf_mines 12, shot_holes 3, ragged_holes 7 | 18 | 14,968 MiB |
| qwen35-awq | 602,112 | 1 | 65/65 | 0 | 5.7 s | 6.9 s | 100% | 27% (n=26) | 4/11 / 3/15 | 2 | 100% (n=11) | 0% | 83% | other 68, leaf_mines 3, chewed_margins 6 | 0 | 13,994 MiB |
| qwen35-awq | 602,112 | 2 | 65/65 | 0 | 6.2 s | 7.6 s | 100% | 27% (n=26) | 4/11 / 3/15 | 2 | 100% (n=11) | 0% | 84% | other 67, leaf_mines 4, chewed_margins 6 | 0 | 14,096 MiB |
| qwen35-awq | 1,003,520 | 1 | 65/65 | 0 | 5.9 s | 7.3 s | 100% | 27% (n=26) | 4/11 / 3/15 | 1 | 100% (n=11) | 0% | 83% | other 67, leaf_mines 5, chewed_margins 7 | 0 | 14,098 MiB |
| qwen35-awq | 1,003,520 | 2 | 65/65 | 0 | 6.7 s | 8.4 s | 98% | 27% (n=26) | 4/11 / 3/15 | 1 | 100% (n=11) | 0% | 83% | other 63, leaf_mines 6, chewed_margins 7 | 0 | 14,098 MiB |
| qwen35-awq | 1,404,928 | 1 | 65/65 | 0 | 6.2 s | 7.7 s | 100% | 23% (n=26) | 4/11 / 2/15 | 2 | 100% (n=11) | 0% | 83% | other 68, leaf_mines 3, chewed_margins 6 | 0 | 14,148 MiB |
| qwen35-awq | 1,404,928 | 2 | 65/65 | 0 | 6.8 s | 8.7 s | 100% | 23% (n=26) | 4/11 / 2/15 | 1 | 100% (n=11) | 0% | 83% | other 65, leaf_mines 4, chewed_margins 6 | 0 | 14,148 MiB |
| qwen35-awq | 2,007,040 | 1 | 65/65 | 0 | 6.1 s | 7.6 s | 100% | 27% (n=26) | 4/11 / 3/15 | 2 | 100% (n=11) | 0% | 82% | other 68, leaf_mines 4, chewed_margins 6 | 0 | 14,272 MiB |
| qwen35-awq | 2,007,040 | 2 | 65/65 | 0 | 6.8 s | 8.8 s | 100% | 27% (n=26) | 4/11 / 3/15 | 2 | 100% (n=11) | 0% | 82% | other 63, leaf_mines 6, chewed_margins 6 | 0 | 14,418 MiB |
| qwen3vl-awq | 602,112 | 1 | 65/65 | 0 | 5.2 s | 7.3 s | 97% | 19% (n=26) | 3/11 / 2/15 | 1 | 100% (n=11) | 0% | 82% | other 34, chewed_margins 12, leaf_mines 3, ragged_holes 6 | 4 | 14,130 MiB |
| qwen3vl-awq | 602,112 | 2 | 65/65 | 0 | 5.3 s | 6.7 s | 97% | 19% (n=26) | 3/11 / 2/15 | 1 | 100% (n=11) | 0% | 82% | other 34, chewed_margins 12, leaf_mines 3, ragged_holes 6 | 4 | 14,130 MiB |
| qwen3vl-awq | 1,003,520 | 1 | 65/65 | 0 | 5.1 s | 6.0 s | 97% | 19% (n=26) | 3/11 / 2/15 | 1 | 100% (n=11) | 0% | 83% | other 33, chewed_margins 11, leaf_mines 4, ragged_holes 7 | 5 | 14,220 MiB |
| qwen3vl-awq | 1,003,520 | 2 | 65/65 | 0 | 5.6 s | 6.6 s | 97% | 15% (n=26) | 3/11 / 1/15 | 0 | 100% (n=11) | 0% | 83% | other 32, leaf_mines 4, chewed_margins 11, ragged_holes 7 | 5 | 14,312 MiB |
| qwen3vl-awq | 1,404,928 | 1 | 65/65 | 0 | 5.4 s | 6.3 s | 97% | 19% (n=26) | 3/11 / 2/15 | 1 | 100% (n=11) | 0% | 82% | other 33, chewed_margins 13, leaf_mines 3, ragged_holes 8 | 4 | 14,476 MiB |
| qwen3vl-awq | 1,404,928 | 2 | 65/65 | 0 | 5.5 s | 6.9 s | 97% | 19% (n=26) | 3/11 / 2/15 | 1 | 100% (n=11) | 0% | 82% | other 32, leaf_mines 3, chewed_margins 13, ragged_holes 8 | 4 | 14,532 MiB |
| qwen3vl-awq | 2,007,040 | 1 | 65/65 | 0 | 5.3 s | 6.1 s | 97% | 19% (n=26) | 3/11 / 2/15 | 1 | 100% (n=11) | 0% | 82% | other 32, chewed_margins 13, leaf_mines 3, ragged_holes 7 | 3 | 14,904 MiB |
| qwen3vl-awq | 2,007,040 | 2 | 65/65 | 0 | 5.8 s | 7.0 s | 97% | 19% (n=26) | 3/11 / 2/15 | 0 | 100% (n=11) | 0% | 83% | other 32, chewed_margins 13, leaf_mines 3, ragged_holes 7 | 3 | 14,904 MiB |

Diagnosis is the top candidate by its unweighted share of "yes" answers. The payload carries no indicator weights and no disease-head scores, so it measures the VLM's answers alone, not the app's diagnosis.

## Indicators: fp8, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_common_rust.raised_pustules` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_gray_leaf_spot.grey_tan_colour` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 3 / 0 | 0 / 7 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_northern_leaf_blight.lower_leaves_first` | 1 / 0 / 2 | 0 / 0 / 7 |
| `tomato_early_blight.concentric_rings` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_early_blight.yellow_halo` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 8 | 0 / 3 / 20 |
| `tomato_late_blight.water_soaked_patches` | 0 / 5 / 3 | 0 / 13 / 10 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 8 | 0 / 0 / 23 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 4 | 0 / 0 / 27 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 3 / 1 / 0 | 7 / 7 / 13 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 4 | 0 / 0 / 27 |

## Indicators: qwen35-awq, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_common_rust.raised_pustules` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_gray_leaf_spot.grey_tan_colour` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 3 / 0 / 0 | 2 / 5 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 7 |
| `tomato_early_blight.concentric_rings` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_early_blight.yellow_halo` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 2 / 6 | 0 / 16 / 7 |
| `tomato_late_blight.water_soaked_patches` | 0 / 7 / 1 | 0 / 15 / 8 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 8 | 0 / 0 / 23 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 4 | 0 / 0 / 27 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 4 / 0 / 0 | 6 / 6 / 15 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 4 | 0 / 0 / 27 |

## Indicators: qwen3vl-awq, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_common_rust.raised_pustules` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_gray_leaf_spot.grey_tan_colour` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 0 / 2 | 0 / 0 / 8 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 3 / 0 | 0 / 7 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 3 | 0 / 0 / 7 |
| `corn_northern_leaf_blight.lower_leaves_first` | 2 / 0 / 1 | 1 / 0 / 6 |
| `tomato_early_blight.concentric_rings` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 6 | 2 / 0 / 23 |
| `tomato_early_blight.yellow_halo` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 8 | 0 / 4 / 19 |
| `tomato_late_blight.water_soaked_patches` | 0 / 7 / 1 | 0 / 14 / 9 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 8 | 0 / 0 / 23 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 4 | 0 / 0 / 27 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 4 / 0 / 0 | 4 / 10 / 13 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 4 | 0 / 0 / 27 |
