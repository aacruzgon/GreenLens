# Deduplication report

Threshold: cosine ≥ 0.98 (float32) on BioCLIP 2 embeddings (b9016a6865e188c0).
Splits by "same plant" group: near-copies at cosine ≥ 0.97 across datasets, and one field visit (`sessions`), share a split; val 0.15, and test 0.15 for datasets without a published test split.

- Images: 11305; duplicate pairs: 4927 (19 across published splits, 71 with different labels)
- Groups dropped for conflicting labels: 44 (233 images)
- Duplicates removed from kept groups: 3959 (including 3888 augmented copies of the same original photo)
- tomato_pakistan_2026: 2690 files dropped as copies of another source's photos (`originals.drop_if_name_in`)
- Dropped as near-copies (≥ 0.97) of a test image: 5
- Near-copies (≥ 0.97) still in different splits: {'tom2024': 73} (within datasets grouped by field visit instead)
- Kept: train 5102, val 1152, test 854
- Same-plant groups: 4353; the largest holds 264 images

## Per dataset and split (kept)

| Dataset | train | val | test |
|---|---|---|---|
| ibean | 874 | 289 | 128 |
| plantdoc | 1914 | 338 | 226 |
| tom2024 | 1939 | 444 | 420 |
| tomato_pakistan_2026 | 375 | 81 | 80 |

## Sample pairs to check by eye

20 pairs from the 0.98–0.99 band (the ones issue #5 asks about) and 10 above it. Paths are under `ml/training/data/raw/`.

| cosine | a | b | labels |
|---|---|---|---|
| 0.9800 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h47_mirror_aug_375.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h48_mirror_aug_335.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9802 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(62)_aug_911.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h99_change_180_aug_948.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9803 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(92)_aug_793.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h72_aug_715.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9803 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_late_blight/Lb10_mirror_aug_1173.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_late_blight/Lb10_mirror_aug_545_aug_345.jpg` (train) | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9805 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h39_lower_aug_541.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_aug_776.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9807 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h69_hight_aug_624.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_hight_aug_702.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9807 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h39_change_180_aug_527.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h39_change_270_aug_481.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9809 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(119)_aug_884.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_lower_aug_641.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9812 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls71_change_180_aug_684.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls71_change_270_aug_692.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9812 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h77_aug_550.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h77_mirror_aug_561.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9815 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h5_change_270_aug_277.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h5_change_90_aug_246.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9824 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls41_aug_418.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls41_hight_aug_513.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9825 | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Raspberry leaf/171322132-56a6d3693df78cf7729070d3.jpg` (train) | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Raspberry leaf/c-204-raspberry.jpg` (train) | raspberry/healthy / raspberry/healthy |
| 0.9839 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/IMG-20240728-WA0068_aug_788.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/IMG-20240728-WA0068_aug_791.jpg` (train) | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 0.9840 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(86)_aug_799.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h77_mirror_aug_562.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9854 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG_20251102_155012_aug_1056.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG_20251102_155012_aug_1060.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9855 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(66)_aug_818.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h95_mirror_aug_1017.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9860 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG_20251102_155119_aug_1088.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG_20251102_155119_aug_1089.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9867 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_160035_aug_1062.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_160035_aug_1070.jpg` (train) | tomato/tomato_early_blight / tomato/tomato_early_blight |
| 0.9881 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h72_change_270_aug_606.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h72_change_90_aug_587.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9901 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h69_hight_aug_626.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h69_mirror_aug_669.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9902 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/IMG-20240812-WA0411_aug_910.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/IMG-20240812-WA0411_aug_911.jpg` (train) | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 0.9909 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h39_hight_aug_532.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h39_hight_aug_533.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9965 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_160055_aug_1019.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_160055_aug_1022.jpg` (train) | tomato/tomato_early_blight / tomato/tomato_early_blight |
| 0.9990 | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Raspberry leaf/579a1aa0c6b3fd0354d1a6b57a40ac42--red-raspberry-medicinal-plants.jpg` (train) | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Raspberry leaf/Raspberry-Suckers1.jpg` (train) | raspberry/healthy / raspberry/healthy |
| 0.9992 | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Potato leaf early blight/Blght.jpg` (train) | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Tomato leaf late blight/LBleaf2INIA_0.jpg` (train) | potato/potato_early_blight / tomato/tomato_late_blight |
| 0.9993 | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Corn Gray leaf spot/2015070295153021.jpg` (train) | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/test/Corn leaf blight/2015070295153021.jpg` (test) | corn/corn_gray_leaf_spot / corn/corn_northern_leaf_blight |
| 0.9995 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG_20251102_154710_aug_1017.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG_20251102_154710_aug_1022.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9998 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/IMG-20240728-WA0021_aug_633.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/IMG-20240728-WA0021_aug_634.jpg` (train) | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 1.0008 | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Potato leaf early blight/irish-blight-symptoms-on-potato-leaves-atmf8b.jpg` (train) | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/test/Potato leaf late blight/irish-blight-symptoms-on-potato-leaves-atmf8b.jpg` (test) | potato/potato_early_blight / potato/potato_late_blight |
