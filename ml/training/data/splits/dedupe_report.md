# Deduplication report

Threshold: cosine ≥ 0.98 (float32) on BioCLIP 2 embeddings (b9016a6865e188c0).
Train/val by a hash of each "same plant" group: near-copies at cosine ≥ 0.97 within a dataset share a split.

- Images: 13995; duplicate pairs: 7765 (19 across published splits, 103 with different labels)
- Groups dropped for conflicting labels: 55 (172 images)
- Duplicates removed from kept groups: 2736
- Kept: train 8941, val 1792, test 354
- Same-plant groups: 9397; the largest holds 60 images

## Per dataset and split (kept)

| Dataset | train | val | test |
|---|---|---|---|
| ibean | 867 | 299 | 128 |
| plantdoc | 1939 | 315 | 226 |
| tom2024 | 2335 | 468 | 0 |
| tomato_pakistan_2026 | 3800 | 710 | 0 |

## Sample pairs to check by eye

20 pairs from the 0.98–0.99 band (the ones issue #5 asks about) and 10 above it. Paths are under `ml/training/data/raw/`.

| cosine | a | b | labels |
|---|---|---|---|
| 0.9804 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_change_270_aug_657.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h72_change_90_aug_585.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9807 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/534ddf3a932e62dd929a3ac1b29bfa29_jpg.rf.8f587ec1873456dc3184fe45d24204b5_aug_213.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/534ddf3a932e62dd929a3ac1b29bfa29_jpg.rf.8f587ec1873456dc3184fe45d24204b5_aug_214.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9809 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_change_90_aug_664.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h82_aug_1098.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9811 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/1342017-162221-44540_aug_697.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/1342017-162221-44540_jpg.rf.fe408cb959432a34c574c5cb9e2f2c4f_aug_109.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9814 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h39_change_90_aug_530.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_hight_aug_703.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9815 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls41_aug_419.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls41_lower_aug_508.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9815 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(119)_aug_882.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h48_mirror_aug_335.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9824 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(164)_aug_821.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h101_change_270_aug_96.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9832 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_late_blight/Late-blight-tomato-leaf-VikramBisht-MBag_jpg.rf.2f7e6d8bdde63e270ee9b642a3d46c7d_aug_281_aug_81.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_late_blight/Late-blight-tomato-leaf-VikramBisht-MBag_jpg.rf.c884d20210ea0278cb0eabf6ac957bcf_aug_306_aug_106.jpg` (train) | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9834 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Tomato-Early-Blight-e1513806873934_aug_1179.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Tomato-Early-Blight-e1513806873934_jpg.rf.457dd0ea6e268addd6aa70aa64223bfa_aug_1174.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9834 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/HL_(65)_aug_850.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h69_mirror_aug_669.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9836 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG20251102153724_aug_954.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG20251102153724_aug_957.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9838 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h1_aug_132.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h1_lower_aug_31.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9840 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h90_mirror_aug_1037.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h90_mirror_vertical_aug_1051.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9860 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/d-to-tylv-fo005-14496A31BE70553130A_aug_241.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/d-to-tylv-fo005-14496A31BE70553130A_aug_244.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9869 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/2013-08-20-04_jpg.rf.342b176f5c9e23ac0c862cbdf957c26e_aug_215.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/2013-08-20-04_jpg.rf.7c3608d82e3397c4620edbedfe8e263c_aug_206.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9876 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls1_mirror_aug_222.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_septora_leaf_spot/Gls1_mirror_aug_224.jpg` (train) | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9879 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h80_aug_1161.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h80_mirror_aug_1190.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9887 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_late_blight/lateblightTom_lesion_aug_1050.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_late_blight/lateblightTom_lesion_aug_379_aug_179.jpg` (train) | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9896 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_154817_aug_790.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/IMG_20251102_154817_aug_1012.jpg` (train) | tomato/tomato_early_blight / tomato/tomato_yellow_leaf_curl_virus |
| 0.9902 | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/grape leaf black rot/00gb.jpg` (train) | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/grape leaf black rot/0gb.jpg` (train) | grape/grape_black_rot / grape/grape_black_rot |
| 0.9903 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/1342017-162221-44540_jpg.rf.23a1b554f636deded60ceaccf67b9c4d_aug_134.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/1342017-162221-44540_jpg.rf.fe408cb959432a34c574c5cb9e2f2c4f_aug_105.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9908 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h25_mirror_aug_45.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h25_mirror_aug_46.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9911 | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Bell_pepper leaf spot/image_1500x1500%253E.jpeg?1472603885.jpg` (train) | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Bell_pepper leaf spot/image_300x300%2523.jpeg?1472603885.jpg` (train) | pepper/pepper_bacterial_leaf_spot / pepper/pepper_bacterial_leaf_spot |
| 0.9935 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_aug_775.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Healthy/h71_hight_aug_704.jpg` (train) | tomato/healthy / tomato/healthy |
| 0.9943 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/leaf_20curl_3_aug_528.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_leaf_yellow_curl_virus/leaf_20curl_3_aug_531.jpg` (train) | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9957 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/leaf_mold1_aug_958.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_mold_leaf/leaf_mold1_aug_959.jpg` (train) | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 0.9999 | `tom2024/maize_diseases/Healthy_leaf/155251341_1680594688411_640x480 - Copy.jpg` (train) | `tom2024/maize_diseases/Healthy_leaf/155251341_1680594688411_640x480.jpg` (train) | corn/healthy / corn/healthy |
| 0.9999 | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_155150_aug_748.jpg` (train) | `tomato_pakistan_2026/Tomato_Leaf_Dataset/Tomato_Early_blight/IMG_20251102_155150_aug_750.jpg` (train) | tomato/tomato_early_blight / tomato/tomato_early_blight |
| 1.0005 | `tom2024/maize_diseases/Healthy_leaf/1011257270_1680513414864_640x480.jpg` (train) | `tom2024/maize_diseases/Healthy_leaf/1566485938_1680513414864_640x480.jpg` (train) | corn/healthy / corn/healthy |
