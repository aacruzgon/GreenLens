# Deduplication report

Threshold: cosine ≥ 0.98 (float32) on BioCLIP 2 embeddings (b9016a6865e188c0).
Splits by "same plant" group: near-copies at cosine ≥ 0.97 across datasets, and one field visit (`sessions`), share a split; val 0.15, and test 0.15 for datasets without a published test split.

- Images: 26862; duplicate pairs: 24437 (19 across published splits, 83 with different labels)
- Groups dropped for conflicting labels: 57 (362 images)
- Duplicates removed from kept groups: 16548 (including 16428 augmented copies of the same original photo)
- tomato_pakistan_2026: 2690 files dropped as copies of another source's photos (`originals.drop_if_name_in`)
- Dropped as near-copies (≥ 0.97) of a test image: 5
- Near-copies (≥ 0.97) still in different splits: {'maize_makerere': 154, 'maize_pretoria': 26, 'tom2024': 88, 'tomato_pakistan_2026': 39} (within datasets grouped by field visit instead)
- `--keep-splits`: 6544 of the 6544 committed images kept their split; 0 are no longer kept; new from datasets already in the splits: {'tom2024': 1}
- Kept: train 7107, val 1564, test 1276
- Same-plant groups: 5992; the largest holds 257 images

## Per dataset and split (kept)

| Dataset | train | val | test |
|---|---|---|---|
| ibean | 874 | 289 | 128 |
| maize_makerere | 828 | 184 | 182 |
| maize_pretoria | 812 | 175 | 178 |
| plantcity | 724 | 166 | 153 |
| plantdoc | 1914 | 338 | 226 |
| tom2024 | 1580 | 331 | 329 |
| tomato_pakistan_2026 | 375 | 81 | 80 |

## Sample pairs to check by eye

20 pairs from the 0.98–0.99 band (the ones issue #5 asks about) and 10 above it. Paths are under `ml/training/data/raw/`.

| cosine | a | b | labels |
|---|---|---|---|
| 0.9803 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h61_hight_aug_299.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h69_hight_aug_624.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9806 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(128)_aug_844.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_hight_aug_702.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9807 | `plantcity/test/tomato_healthy_leaf/217.JPG` (train) | `plantcity/train/tomato_healthy_leaf/tomato_1335.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9814 | `plantcity/train/tomato_bacterial_spot/tomato_946.jpg` (train) | `plantcity/train/tomato_bacterial_spot/tomato_953.jpg` (train) | tomato/tomato_bacterial_spot / tomato/tomato_bacterial_spot |
| 0.9814 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h39_mirror_vertical_aug_475.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_change_180_aug_772.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9825 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(166)_aug_853.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h10_aug_158.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9826 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_155152_1_aug_887.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_155152_aug_897.jpg` (train) | tomato/tomato_early_blight / tomato/tomato_early_blight |
| 0.9827 | `plantcity/test/tomato_late_blight/126.JPG` (train) | `plantcity/train/tomato_late_blight/tomato_509.jpg` (train) | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9834 | `plantcity/test/tomato spider mites/33.jpg` (train) | `plantcity/train/tomato spider mites/tomato_129.jpg` (train) | tomato/tomato_spider_mites / tomato/tomato_spider_mites |
| 0.9835 | `plantcity/test/tomato_late_blight/53.JPG` (train) | `plantcity/train/tomato_late_blight/tomato_230.jpg` (train) | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9844 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h25_aug_59.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h25_change_90_aug_512.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9847 | `plantcity/train/tomato_leaf_mold/tomato_220.jpg` (train) | `plantcity/train/tomato_leaf_mold/tomato_225.jpg` (train) | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 0.9849 | `plantcity/train/Corn Normal leaf/321.jpg` (train) | `plantcity/train/Corn Normal leaf/325.jpg` (train) | corn/healthy / corn/healthy |
| 0.9850 | `plantcity/test/tomato_early_blight/198.JPG` (train) | `plantcity/train/tomato_early_blight/tomato_710.jpg` (train) | tomato/tomato_early_blight / tomato/tomato_early_blight |
| 0.9850 | `plantcity/train/tomato_healthy_leaf/tomato_574.jpg` (train) | `plantcity/train/tomato_healthy_leaf/tomato_575.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9870 | `plantcity/train/tomato_leaf_mold/tomato_1163.jpg` (train) | `plantcity/train/tomato_leaf_mold/tomato_1174.jpg` (train) | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 0.9871 | `plantcity/train/tomato_bacterial_spot/tomato_167.jpg` (train) | `plantcity/train/tomato_bacterial_spot/tomato_173.jpg` (train) | tomato/tomato_bacterial_spot / tomato/tomato_bacterial_spot |
| 0.9871 | `plantcity/test/tomato_healthy_leaf/93.JPG` (train) | `plantcity/train/tomato_healthy_leaf/tomato_874.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9887 | `maize_pretoria/NCLB/IMG_3792.JPG` (train) | `maize_pretoria/NCLB/IMG_3799.JPG` (train) | corn/corn_northern_leaf_blight / corn/corn_northern_leaf_blight |
| 0.9896 | `plantcity/test/tomato_septoria_leaf/tomato_229.jpg` (train) | `plantcity/train/tomato_septoria_leaf/tomato_1206.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9910 | `plantcity/test/Corn Normal leaf/39.jpg` (train) | `plantcity/test/Corn Normal leaf/41.jpg` (train) | corn/healthy / corn/healthy |
| 0.9919 | `plantcity/test/Corn Normal leaf/2.jpg` (train) | `plantcity/train/Corn Normal leaf/5.jpg` (train) | corn/healthy / corn/healthy |
| 0.9931 | `plantcity/test/Corn Normal leaf/59.jpg` (train) | `plantcity/train/Corn Normal leaf/234.jpg` (train) | corn/healthy / corn/healthy |
| 0.9933 | `plantcity/train/tomato_healthy_leaf/tomato_1320.jpg` (train) | `plantcity/train/tomato_healthy_leaf/tomato_1323.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9939 | `maize_pretoria/NoFoliarSymptoms/IMG_3752.JPG` (train) | `maize_pretoria/NoFoliarSymptoms/IMG_3753.JPG` (train) | corn/healthy / corn/healthy |
| 0.9941 | `plantcity/test/tomato_leaf_mold/250.JPG` (train) | `plantcity/train/tomato_leaf_mold/tomato_1010.jpg` (train) | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 0.9950 | `plantcity/test/tomato_septoria_leaf/tomato_021.jpg` (train) | `plantcity/train/tomato_septoria_leaf/tomato_521.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9965 | `plantcity/test/tomato_healthy_leaf/tomato_071.jpg` (train) | `plantcity/train/tomato_healthy_leaf/tomato_139.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9977 | `plantcity/train/tomato_septoria_leaf/tomato_1121.jpg` (train) | `plantcity/train/tomato_septoria_leaf/tomato_1128.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9997 | `tom2024/tomato_diseases/healthy_leaf/314902902_1681377901570_640x480.jpg` (train) | `tom2024/tomato_diseases/healthy_leaf/706031433_1681377901570_640x480.jpg` (train) | tomato/healthy / tomato/healthy |
