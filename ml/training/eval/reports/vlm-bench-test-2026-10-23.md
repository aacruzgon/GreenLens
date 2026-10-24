# VLM benchmark: test-2026-10-23

Scored on `ml/training/runs/vlm-bench/qwen35-awq-test/px1003520-c2/cases.jsonl` (41 cases, SHA-256 `0f5f939fe0bfb1f8`), each run's own cases.

| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| qwen35-awq-test | 1,003,520 | 2 | 41/41 | 0 | 6.3 s | 8.8 s | 98% | 17% (n=6) | 0/2 / 1/4 | 5 | 50% (n=8) | 6% | 22% | other 46, leaf_mines 10, chewed_margins 5, sooty_mould 1 | 2 | 12,783 MiB |

Diagnosis is the top candidate by its unweighted share of "yes" answers. The payload carries no indicator weights and no disease-head scores, so it measures the VLM's answers alone, not the app's diagnosis.

## Certainty words

Each finding by the confidence its certainty word mapped to (low 0.4, medium 0.65, high 0.85). Only the species guess has a truth here; these leaves show disease, so every damage or organism finding is a false sign (P11).

| Run | Species guess right, per confidence | Damage findings, per confidence | Organisms, per confidence |
|---|---|---|---|
| qwen35-awq-test (1,003,520 px, 2 at once) | 0.40: 17/18, 0.85: 23/23 | 0.40: 4, 0.65: 1, 0.85: 57 | 0.40: 2 |

## Indicators: qwen35-awq-test, 1,003,520 pixels, 2 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 1 / 0 / 0 | 0 / 4 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 0 / 1 | 0 / 3 / 1 |
| `corn_common_rust.raised_pustules` | 1 / 0 / 0 | 0 / 4 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 2 / 0 | 1 / 2 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 1 / 1 / 0 | 1 / 2 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 2 / 0 | 0 / 3 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 0 / 0 / 0 | 1 / 4 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 0 / 0 | 0 / 5 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 0 | 0 / 1 / 4 |
| `tomato_early_blight.concentric_rings` | 0 / 2 / 0 | 0 / 2 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 2 | 0 / 1 / 1 |
| `tomato_early_blight.yellow_halo` | 1 / 1 / 0 | 0 / 2 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 1 / 0 | 0 / 2 / 1 |
| `tomato_late_blight.water_soaked_patches` | 0 / 1 / 0 | 0 / 3 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 1 | 0 / 0 / 3 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 0 / 0 | 0 / 4 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 0 / 0 / 0 | 0 / 4 / 0 |
| `tomato_septoria_leaf_spot.small_spots` | 0 / 0 / 0 | 1 / 3 / 0 |
