# VLM benchmark: 2026-10-20-fp8

| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| fp8 | 602,112 | 1 | 65/65 | 0 | 6.0 s | 7.5 s | 100% | 15% (n=26) | 2/11 / 2/15 | 0 | 100% (n=11) | 0% | 32% | other 14, chewed_margins 28, leaf_mines 20, shot_holes 2, ragged_holes 5 | 30 | 14,078 MiB |
| fp8 | 602,112 | 2 | 65/65 | 0 | 6.5 s | 7.8 s | 98% | 8% (n=26) | 2/11 / 0/15 | 1 | 100% (n=11) | 0% | 32% | other 16, chewed_margins 28, leaf_mines 11, shot_holes 9, ragged_holes 6 | 34 | 14,078 MiB |
| fp8 | 1,003,520 | 1 | 65/65 | 0 | 6.2 s | 8.4 s | 100% | 12% (n=26) | 2/11 / 1/15 | 0 | 100% (n=11) | 0% | 32% | other 22, chewed_margins 31, leaf_mines 13, shot_holes 4, ragged_holes 7, sooty_mould 1, skeletonised_leaves 1 | 19 | 14,538 MiB |
| fp8 | 1,003,520 | 2 | 65/65 | 0 | 6.8 s | 9.6 s | 97% | 15% (n=26) | 3/11 / 1/15 | 1 | 100% (n=11) | 0% | 32% | chewed_margins 30, other 17, leaf_mines 13, shot_holes 14, ragged_holes 7, sooty_mould 1, skeletonised_leaves 1 | 31 | 14,538 MiB |

## Indicators: fp8, 1,003,520 pixels, 1 at once

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 3 / 0 | 0 / 11 / 2 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 3 | 0 / 3 / 10 |
| `corn_common_rust.raised_pustules` | 0 / 3 / 0 | 1 / 12 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 2 / 0 | 4 / 8 / 2 |
| `corn_gray_leaf_spot.grey_tan_colour` | 1 / 1 / 0 | 0 / 13 / 1 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 2 / 0 | 0 / 12 / 2 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 3 / 0 | 1 / 12 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 3 / 0 | 1 / 9 / 3 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 3 | 1 / 0 / 12 |
| `tomato_early_blight.concentric_rings` | 0 / 6 / 0 | 0 / 43 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 6 | 0 / 0 / 43 |
| `tomato_early_blight.yellow_halo` | 0 / 6 / 0 | 0 / 43 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 8 | 0 / 7 / 34 |
| `tomato_late_blight.water_soaked_patches` | 0 / 8 / 0 | 0 / 41 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 8 | 0 / 0 / 41 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 4 / 0 | 0 / 45 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 2 / 1 / 1 | 7 / 29 / 9 |
| `tomato_septoria_leaf_spot.small_spots` | 2 / 2 / 0 | 14 / 31 / 0 |
