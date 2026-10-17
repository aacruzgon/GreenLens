# Leaf score on the val split (tau-leaf-val-2026-10-16)

Detector `leaf_detector/rfdetr-s-leaf-2026-10-16b-fp32.onnx` (manifest.cpu.toml, CPUExecutionProvider); `score_threshold` 0.35, `min_side_px` 24 on the stored image (long side ≤ 1600 px); `tau_leaf` in the manifest when run: 0.5.

Share of plant photos whose leaf score is below each τ_leaf (a false `no_plant_found` if it were the scan's only plant shot). Nothing kept: 2.9%. Percentiles: p1 0.000, p5 0.485, p10 0.663, p50 0.908.

## By dataset

| | n | < 0.35 | < 0.4 | < 0.45 | < 0.5 |
|---|---|---|---|---|---|
| ibean | 289 | 0.0% | 0.0% | 0.0% | 0.0% |
| plantdoc | 338 | 0.3% | 0.3% | 0.3% | 0.3% |
| tom2024 | 444 | 7.4% | 8.8% | 11.0% | 14.2% |
| tomato_pakistan_2026 | 81 | 0.0% | 0.0% | 0.0% | 0.0% |
| **All** | 1152 | 2.9% | 3.5% | 4.3% | 5.6% |

## By species

| | n | < 0.35 | < 0.4 | < 0.45 | < 0.5 |
|---|---|---|---|---|---|
| apple | 36 | 0.0% | 0.0% | 0.0% | 0.0% |
| bean | 289 | 0.0% | 0.0% | 0.0% | 0.0% |
| blueberry | 16 | 0.0% | 0.0% | 0.0% | 0.0% |
| cherry | 7 | 0.0% | 0.0% | 0.0% | 0.0% |
| corn | 180 | 2.8% | 3.3% | 3.3% | 3.9% |
| grape | 16 | 0.0% | 0.0% | 0.0% | 0.0% |
| peach | 15 | 6.7% | 6.7% | 6.7% | 6.7% |
| pepper | 16 | 0.0% | 0.0% | 0.0% | 0.0% |
| potato | 27 | 0.0% | 0.0% | 0.0% | 0.0% |
| raspberry | 17 | 0.0% | 0.0% | 0.0% | 0.0% |
| soybean | 9 | 0.0% | 0.0% | 0.0% | 0.0% |
| squash | 18 | 0.0% | 0.0% | 0.0% | 0.0% |
| strawberry | 13 | 0.0% | 0.0% | 0.0% | 0.0% |
| tomato | 493 | 5.7% | 6.7% | 8.7% | 11.4% |

## The 20 lowest

| score | dataset | species | problem | path |
|---|---|---|---|---|
| 0.000 | plantdoc | peach | healthy | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Peach leaf/79a24e562c768cd993776c518b238eed--storyboard-toe.jpg` |
| 0.000 | tom2024 | corn | corn_curvularia_leaf_spot | `tom2024/maize_diseases/Curvularia-D/1044077977_1679300784239_640x480.jpg` |
| 0.000 | tom2024 | corn | corn_curvularia_leaf_spot | `tom2024/maize_diseases/Curvularia-D/946588862_1679303952315_640x480.jpg` |
| 0.000 | tom2024 | corn | healthy | `tom2024/maize_diseases/Healthy_leaf/1343230956_1659513782856.jpg` |
| 0.000 | tom2024 | corn | healthy | `tom2024/maize_diseases/Healthy_leaf/371179508_1662121988645.jpg` |
| 0.000 | tom2024 | corn | corn_northern_leaf_blight | `tom2024/maize_diseases/Helminthosporiosis-D/1599460127_1677917368526_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/1129289751_1670938599524_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/1157132437_1670951405912_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/1171331010_1670938599524_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/1289234428_1670946191247_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/14820091_1668954629167_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/1563253531_1668954714518_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/1694866554_1670951365425_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/1779539871_1668955619403_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/195199953_1666705264117_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/214385373_1670951327160_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/23539076_1677607464643_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/321391633_1670951460056_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/361145089_1677608119036_640x480.jpg` |
| 0.000 | tom2024 | tomato | tomato_bacterial_wilt | `tom2024/tomato_diseases/Bacterial_floundering_d/388393346_1681321518751_640x480.jpg` |
