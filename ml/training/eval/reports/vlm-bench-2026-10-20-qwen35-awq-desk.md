# VLM benchmark: 2026-10-20-qwen35-awq-desk

| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| qwen35-awq-desk | 1,003,520 | 1 | 65/65 | 0 | 5.3 s | 6.2 s | 98% | 31% (n=26) | 4/11 / 4/15 | 5 | 91% (n=11) | 1% | 26% | other 66, chewed_margins 7, leaf_mines 6 | 0 | 12,500 MiB |
| qwen35-awq-desk | 1,003,520 | 2 | 65/65 | 0 | 6.2 s | 7.6 s | 98% | 31% (n=26) | 4/11 / 4/15 | 4 | 91% (n=11) | 1% | 25% | other 67, leaf_mines 3, chewed_margins 7 | 0 | 12,500 MiB |

Diagnosis is the top candidate by its unweighted share of "yes" answers. The payload carries no indicator weights and no disease-head scores, so it measures the VLM's answers alone, not the app's diagnosis.

## Indicators: qwen35-awq-desk, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 3 / 0 | 0 / 7 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 2 / 1 | 0 / 6 / 1 |
| `corn_common_rust.raised_pustules` | 0 / 3 / 0 | 0 / 7 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 2 / 0 | 1 / 7 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 1 / 1 / 0 | 0 / 8 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 2 / 0 | 0 / 8 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 2 / 1 / 0 | 1 / 6 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 1 / 2 / 0 | 0 / 7 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 7 |
| `tomato_early_blight.concentric_rings` | 0 / 6 / 0 | 0 / 25 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_early_blight.yellow_halo` | 5 / 1 / 0 | 10 / 14 / 1 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 1 / 7 | 0 / 11 / 12 |
| `tomato_late_blight.water_soaked_patches` | 0 / 8 / 0 | 0 / 23 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 8 | 0 / 0 / 23 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 4 / 0 | 0 / 27 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 1 / 3 / 0 | 7 / 20 / 0 |
| `tomato_septoria_leaf_spot.small_spots` | 1 / 3 / 0 | 11 / 16 / 0 |
