# VLM benchmark: 2026-10-20-qwen3vl-awq

| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| qwen3vl-awq | 602,112 | 1 | 65/65 | 0 | 4.3 s | 4.8 s | 98% | 12% (n=26) | 2/11 / 1/15 | 0 | 91% (n=11) | 1% | 32% | other 34, chewed_margins 12, leaf_mines 3, ragged_holes 6 | 4 | 14,092 MiB |
| qwen3vl-awq | 602,112 | 2 | 65/65 | 0 | 4.9 s | 5.5 s | 98% | 12% (n=26) | 2/11 / 1/15 | 0 | 91% (n=11) | 1% | 32% | other 33, chewed_margins 12, leaf_mines 3, ragged_holes 6 | 4 | 14,258 MiB |
| qwen3vl-awq | 1,003,520 | 1 | 65/65 | 0 | 4.4 s | 5.1 s | 97% | 12% (n=26) | 2/11 / 1/15 | 0 | 100% (n=11) | 0% | 33% | other 32, chewed_margins 12, leaf_mines 4, ragged_holes 7 | 4 | 14,260 MiB |
| qwen3vl-awq | 1,003,520 | 2 | 65/65 | 0 | 5.1 s | 5.9 s | 97% | 12% (n=26) | 2/11 / 1/15 | 0 | 100% (n=11) | 0% | 32% | other 32, chewed_margins 11, leaf_mines 4, ragged_holes 7 | 5 | 14,260 MiB |

## Indicators: qwen3vl-awq, 1,003,520 pixels, 1 at once

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 3 / 0 | 0 / 8 / 5 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 3 | 0 / 1 / 12 |
| `corn_common_rust.raised_pustules` | 0 / 3 / 0 | 0 / 13 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 2 / 0 | 1 / 8 / 5 |
| `corn_gray_leaf_spot.grey_tan_colour` | 1 / 1 / 0 | 0 / 11 / 3 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 2 / 0 | 0 / 13 / 1 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 3 / 0 | 0 / 13 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 3 / 0 | 0 / 8 / 5 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 13 |
| `tomato_early_blight.concentric_rings` | 0 / 6 / 0 | 0 / 43 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 6 | 0 / 0 / 43 |
| `tomato_early_blight.yellow_halo` | 0 / 6 / 0 | 0 / 43 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 8 | 0 / 7 / 34 |
| `tomato_late_blight.water_soaked_patches` | 0 / 8 / 0 | 0 / 41 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 8 | 0 / 0 / 41 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 4 / 0 | 4 / 41 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 2 / 2 / 0 | 4 / 39 / 2 |
| `tomato_septoria_leaf_spot.small_spots` | 2 / 2 / 0 | 14 / 30 / 1 |
