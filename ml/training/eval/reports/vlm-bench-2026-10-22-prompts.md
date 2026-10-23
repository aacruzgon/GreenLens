# VLM benchmark: 2026-10-22-prompts

Scored on `ml/training/runs/vlm-bench/cases-102-common-f5.jsonl` (52 cases, SHA-256 `45110e5ad5ef7496`) (`--cases`).

| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| qwen35-awq | 602,112 | 1 | 52/52 | 0 | 5.1 s | 6.0 s | 100% | 33% (n=6) | 2/4 / 0/2 | 6 | 100% (n=10) | 0% | 24% | other 49, leaf_mines 4, chewed_margins 3 | 0 | 13,994 MiB |
| qwen35-awq | 602,112 | 2 | 52/52 | 0 | 5.9 s | 7.0 s | 100% | 33% (n=6) | 2/4 / 0/2 | 5 | 100% (n=10) | 0% | 24% | other 50, chewed_margins 3, leaf_mines 3 | 0 | 14,082 MiB |
| qwen35-awq | 1,003,520 | 1 | 52/52 | 0 | 5.1 s | 6.2 s | 98% | 33% (n=6) | 2/4 / 0/2 | 6 | 100% (n=10) | 0% | 25% | other 49, chewed_margins 3, leaf_mines 3 | 0 | 14,184 MiB |
| qwen35-awq | 1,003,520 | 2 | 52/52 | 0 | 6.0 s | 7.3 s | 98% | 33% (n=6) | 2/4 / 0/2 | 7 | 100% (n=10) | 0% | 25% | other 47, chewed_margins 3, leaf_mines 3 | 0 | 14,288 MiB |
| qwen35-awq-f5 | 1,003,520 | 1 | 52/52 | 0 | 4.8 s | 5.8 s | 98% | 33% (n=6) | 2/4 / 0/2 | 6 | 100% (n=10) | 0% | 26% | other 47, leaf_mines 5, chewed_margins 3 | 1 | 14,124 MiB |
| qwen35-awq-f5 | 1,003,520 | 2 | 52/52 | 0 | 5.7 s | 7.0 s | 98% | 33% (n=6) | 2/4 / 0/2 | 5 | 100% (n=10) | 0% | 26% | other 47, leaf_mines 5, chewed_margins 3 | 1 | 14,230 MiB |
| qwen35-awq-i3 | 1,003,520 | 1 | 52/52 | 0 | 5.1 s | 6.1 s | 98% | 33% (n=6) | 2/4 / 0/2 | 8 | 90% (n=10) | 1% | 25% | other 47, leaf_mines 5, chewed_margins 3 | 1 | 14,633 MiB |
| fp8 | 602,112 | 1 | 52/52 | 0 | 6.0 s | 6.8 s | 100% | 17% (n=6) | 0/4 / 1/2 | 0 | 100% (n=10) | 0% | 31% | other 10, chewed_margins 23, leaf_mines 9, shot_holes 1 | 14 | 14,078 MiB |
| fp8 | 602,112 | 2 | 52/52 | 0 | 6.4 s | 7.2 s | 98% | 0% (n=6) | 0/4 / 0/2 | 1 | 100% (n=10) | 0% | 31% | other 12, chewed_margins 23, leaf_mines 9, shot_holes 1 | 16 | 14,078 MiB |
| fp8 | 1,003,520 | 1 | 52/52 | 0 | 6.1 s | 7.0 s | 100% | 0% (n=6) | 0/4 / 0/2 | 0 | 100% (n=10) | 0% | 32% | other 17, chewed_margins 23, leaf_mines 11, shot_holes 1 | 14 | 14,538 MiB |
| fp8 | 1,003,520 | 2 | 52/52 | 0 | 6.7 s | 7.6 s | 96% | 17% (n=6) | 1/4 / 0/2 | 1 | 100% (n=10) | 0% | 32% | chewed_margins 23, other 11, leaf_mines 11, shot_holes 10 | 25 | 14,538 MiB |
| fp8-f5 | 1,003,520 | 1 | 52/52 | 0 | 6.3 s | 7.0 s | 98% | 17% (n=6) | 0/4 / 1/2 | 0 | 100% (n=10) | 0% | 34% | other 24, chewed_margins 14, leaf_mines 8, shot_holes 2 | 29 | 14,226 MiB |
| fp8-f5 | 1,003,520 | 2 | 52/52 | 0 | 6.6 s | 7.9 s | 98% | 0% (n=6) | 0/4 / 0/2 | 0 | 100% (n=10) | 0% | 33% | other 23, chewed_margins 17, leaf_mines 9, shot_holes 2 | 27 | 14,226 MiB |
| fp8-i3 | 1,003,520 | 1 | 52/52 | 0 | 6.5 s | 7.5 s | 98% | 0% (n=6) | 0/4 / 0/2 | 0 | 100% (n=10) | 0% | 31% | other 21, chewed_margins 15, leaf_mines 10, shot_holes 2 | 28 | 14,729 MiB |

Diagnosis is the top candidate by its unweighted share of "yes" answers. The payload carries no indicator weights and no disease-head scores, so it measures the VLM's answers alone, not the app's diagnosis.

## Certainty words

Each finding by the confidence its certainty word mapped to (low 0.4, medium 0.65, high 0.85). Only the species guess has a truth here; these leaves show disease, so every damage or organism finding is a false sign (P11).

| Run | Species guess right, per confidence | Damage findings, per confidence | Organisms, per confidence |
|---|---|---|---|
| qwen35-awq (1,003,520 px, 1 at once) | 0.40: 28/28, 0.85: 23/24 | 0.40: 5, 0.65: 7, 0.85: 43 | none |
| qwen35-awq-f5 (1,003,520 px, 1 at once) | 0.40: 25/25, 0.85: 26/27 | 0.40: 3, 0.65: 10, 0.85: 42 | 0.40: 1 |
| qwen35-awq-i3 (1,003,520 px, 1 at once) | 0.40: 26/26, 0.85: 25/26 | 0.40: 2, 0.65: 12, 0.85: 41 | 0.40: 1 |
| fp8 (1,003,520 px, 1 at once) | 0.40: 4/4, 0.85: 48/48 | 0.65: 11, 0.85: 41 | 0.40: 1, 0.65: 9, 0.85: 4 |
| fp8-f5 (1,003,520 px, 1 at once) | 0.85: 51/52 | 0.65: 11, 0.85: 37 | 0.40: 2, 0.65: 18, 0.85: 9 |
| fp8-i3 (1,003,520 px, 1 at once) | 0.85: 51/52 | 0.65: 11, 0.85: 37 | 0.40: 2, 0.65: 20, 0.85: 6 |

## Indicators: qwen35-awq, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 1 | 0 / 3 / 0 |
| `corn_common_rust.raised_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 1 / 0 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 0 / 0 | 1 / 3 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 0 | 0 / 4 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 0 | 0 / 0 / 4 |
| `tomato_early_blight.concentric_rings` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 6 |
| `tomato_early_blight.yellow_halo` | 2 / 1 / 0 | 0 / 6 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 1 | 0 / 5 / 3 |
| `tomato_late_blight.water_soaked_patches` | 0 / 1 / 0 | 0 / 8 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 1 | 0 / 0 / 8 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 0 | 0 / 9 / 0 |

## Indicators: qwen35-awq-f5, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_common_rust.raised_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 1 / 0 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 0 / 0 | 1 / 3 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 0 | 0 / 4 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 0 | 0 / 1 / 3 |
| `tomato_early_blight.concentric_rings` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 6 |
| `tomato_early_blight.yellow_halo` | 2 / 1 / 0 | 1 / 5 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 1 | 0 / 1 / 7 |
| `tomato_late_blight.water_soaked_patches` | 0 / 1 / 0 | 0 / 8 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 1 | 0 / 0 / 8 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 0 | 1 / 8 / 0 |

## Indicators: qwen35-awq-i3, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 1 | 0 / 3 / 0 |
| `corn_common_rust.raised_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 1 / 0 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 0 / 0 | 1 / 3 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 0 | 0 / 4 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 0 | 0 / 0 / 4 |
| `tomato_early_blight.concentric_rings` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 6 |
| `tomato_early_blight.yellow_halo` | 2 / 1 / 0 | 1 / 5 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 1 | 0 / 5 / 3 |
| `tomato_late_blight.water_soaked_patches` | 0 / 1 / 0 | 0 / 8 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 1 | 0 / 0 / 8 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 0 | 2 / 7 / 0 |

## Indicators: fp8, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 1 | 0 / 2 / 1 |
| `corn_common_rust.raised_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 0 / 0 | 1 / 3 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 0 | 1 / 3 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 0 | 1 / 0 / 3 |
| `tomato_early_blight.concentric_rings` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 6 |
| `tomato_early_blight.yellow_halo` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 1 | 0 / 1 / 7 |
| `tomato_late_blight.water_soaked_patches` | 0 / 1 / 0 | 0 / 8 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 1 | 0 / 0 / 8 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 0 / 0 / 0 | 0 / 8 / 1 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 0 | 0 / 9 / 0 |

## Indicators: fp8-f5, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 1 | 0 / 2 / 1 |
| `corn_common_rust.raised_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 1 / 0 / 0 | 1 / 2 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 0 / 0 | 0 / 4 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 0 | 0 / 3 / 1 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 0 | 0 / 0 / 4 |
| `tomato_early_blight.concentric_rings` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 6 |
| `tomato_early_blight.yellow_halo` | 0 / 2 / 1 | 0 / 6 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 1 | 0 / 1 / 7 |
| `tomato_late_blight.water_soaked_patches` | 0 / 1 / 0 | 0 / 8 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 1 | 0 / 0 / 8 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 0 / 0 / 0 | 0 / 7 / 2 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 0 | 0 / 9 / 0 |

## Indicators: fp8-i3, 1,003,520 pixels, 1 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 1 | 0 / 2 / 1 |
| `corn_common_rust.raised_pustules` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 1 / 0 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 1 / 0 | 0 / 3 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 0 / 0 | 1 / 3 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 0 | 0 / 4 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 0 | 1 / 0 / 3 |
| `tomato_early_blight.concentric_rings` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 0 / 6 |
| `tomato_early_blight.yellow_halo` | 0 / 3 / 0 | 0 / 6 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 0 / 1 | 0 / 1 / 7 |
| `tomato_late_blight.water_soaked_patches` | 0 / 1 / 0 | 0 / 8 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 1 | 0 / 0 / 8 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 0 | 0 / 9 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 0 / 0 / 0 | 0 / 8 / 1 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 0 | 0 / 9 / 0 |
