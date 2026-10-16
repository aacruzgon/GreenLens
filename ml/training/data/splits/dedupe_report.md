# Deduplication report

Threshold: cosine ≥ 0.98 on BioCLIP 2 embeddings (b9016a6865e188c0).

- Images: 13995; duplicate pairs: 8066 (20 across published splits, 105 with different labels)
- Groups dropped for conflicting labels: 56 (174 images)
- Duplicates removed from kept groups: 2786
- Kept: train 8975, val 1707, test 353

## Per dataset and split (kept)

| Dataset | train | val | test |
|---|---|---|---|
| ibean | 872 | 294 | 128 |
| plantdoc | 1900 | 351 | 225 |
| tom2024 | 2403 | 399 | 0 |
| tomato_pakistan_2026 | 3800 | 663 | 0 |

## Sample pairs to check by eye

| cosine | a | b | labels |
|---|---|---|---|
| 0.9800 | tomato_pakistan_2026:train:tj5itxzunn3k_jpg.rf.0fba696d5ac414726848b134a807e95d_aug_410.jpg | tomato_pakistan_2026:train:tj5itxzunn3k_jpg.rf.a8b6c1718774dd892c52192d2efe0574_aug_425.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9810 | tomato_pakistan_2026:train:h10_aug_156.jpg | tomato_pakistan_2026:train:h13_aug_148.jpg | tomato/healthy / tomato/healthy |
| 0.9810 | tomato_pakistan_2026:train:h69_mirror_aug_670.jpg | tomato_pakistan_2026:train:h72_hight_aug_600.jpg | tomato/healthy / tomato/healthy |
| 0.9810 | tomato_pakistan_2026:train:h50_change_90_aug_264.jpg | tomato_pakistan_2026:train:h50_mirror_vertical_aug_250.jpg | tomato/healthy / tomato/healthy |
| 0.9814 | tomato_pakistan_2026:train:HL_(103)_aug_980.jpg | tomato_pakistan_2026:train:h61_change_270_aug_290.jpg | tomato/healthy / tomato/healthy |
| 0.9814 | tomato_pakistan_2026:train:TYLCV-tomate-DB-445-171_jpg.rf.33bafc49d460bf076de1b7d0cf75ab32_aug_340.jpg | tomato_pakistan_2026:train:TYLCV-tomate-DB-445-171_jpg.rf.9d70326900dd981ab5d49063b0d644f0_aug_267.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9814 | tomato_pakistan_2026:train:h14_aug_37.jpg | tomato_pakistan_2026:train:h39_aug_517.jpg | tomato/healthy / tomato/healthy |
| 0.9819 | tomato_pakistan_2026:train:HL_(82)_aug_789.jpg | tomato_pakistan_2026:train:h80_aug_1161.jpg | tomato/healthy / tomato/healthy |
| 0.9824 | tomato_pakistan_2026:train:h71_change_270_aug_658.jpg | tomato_pakistan_2026:train:h72_aug_716.jpg | tomato/healthy / tomato/healthy |
| 0.9824 | tomato_pakistan_2026:train:early_blight_jpg.rf.1091c0a2a9a3da5b9ad688c89298614c_aug_675.jpg | tomato_pakistan_2026:train:early_blight_jpg.rf.1091c0a2a9a3da5b9ad688c89298614c_aug_678.jpg | tomato/tomato_early_blight / tomato/tomato_early_blight |
| 0.9829 | tomato_pakistan_2026:train:h71_lower_aug_640.jpg | tomato_pakistan_2026:train:h82_aug_1100.jpg | tomato/healthy / tomato/healthy |
| 0.9834 | tomato_pakistan_2026:train:1342017-162221-44540_aug_703.jpg | tomato_pakistan_2026:train:1342017-162221-44540_jpg.rf.23a1b554f636deded60ceaccf67b9c4d_aug_131.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9839 | tomato_pakistan_2026:train:TYLCV4_jpg.rf.29aadb3b94c12700425f6851c3ac155a_aug_784.jpg | tomato_pakistan_2026:train:TYLCV4_jpg.rf.29aadb3b94c12700425f6851c3ac155a_aug_790.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9844 | tomato_pakistan_2026:train:20140818-tomato-sauce-vicky-wasik-1_jpg.rf.4b72fed459a611f6953d17f290086bd4_aug_229.jpg | tomato_pakistan_2026:train:20140818-tomato-sauce-vicky-wasik-1_jpg.rf.4b72fed459a611f6953d17f290086bd4_aug_230.jpg | tomato/healthy / tomato/healthy |
| 0.9844 | tomato_pakistan_2026:train:h69_change_270_aug_662.jpg | tomato_pakistan_2026:train:h69_lower_aug_620.jpg | tomato/healthy / tomato/healthy |
| 0.9849 | tomato_pakistan_2026:train:fwa2013313115151_jpg.rf.f46c682f69e0115a10044a45345f437a_aug_613.jpg | tomato_pakistan_2026:train:fwa2013313115151_jpg.rf.f46c682f69e0115a10044a45345f437a_aug_614.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9849 | tomato_pakistan_2026:train:h62_change_180_aug_309.jpg | tomato_pakistan_2026:train:h62_change_90_aug_282.jpg | tomato/healthy / tomato/healthy |
| 0.9854 | tomato_pakistan_2026:train:TYLCV-tomate-DB-445-171_jpg.rf.33bafc49d460bf076de1b7d0cf75ab32_aug_336.jpg | tomato_pakistan_2026:train:TYLCV-tomate-DB-445-171_jpg.rf.33bafc49d460bf076de1b7d0cf75ab32_aug_337.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9854 | tomato_pakistan_2026:train:IMG-20240727-WA0167_aug_672.jpg | tomato_pakistan_2026:train:IMG-20240727-WA0167_aug_674.jpg | tomato/tomato_leaf_mold / tomato/tomato_leaf_mold |
| 0.9858 | tomato_pakistan_2026:train:h71_hight_aug_703.jpg | tomato_pakistan_2026:train:h71_mirror_aug_667.jpg | tomato/healthy / tomato/healthy |
| 0.9863 | tomato_pakistan_2026:train:8226402877_9abf151b5b_b_jpg.rf.3418a6df57d7721b34b95e09e3c95ee3_aug_227.jpg | tomato_pakistan_2026:train:8226402877_9abf151b5b_b_jpg.rf.3418a6df57d7721b34b95e09e3c95ee3_aug_230.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9868 | tomato_pakistan_2026:train:tj5itxzunn3k_jpg.rf.4aeda27b2535578e718fe6279491d0d4_aug_387.jpg | tomato_pakistan_2026:train:tj5itxzunn3k_jpg.rf.a8b6c1718774dd892c52192d2efe0574_aug_425.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9868 | tomato_pakistan_2026:train:TYLCV-tomate-DB-445-171_jpg.rf.33bafc49d460bf076de1b7d0cf75ab32_aug_337.jpg | tomato_pakistan_2026:train:TYLCV-tomate-DB-445-171_jpg.rf.33bafc49d460bf076de1b7d0cf75ab32_aug_340.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9873 | tomato_pakistan_2026:train:LB_ (372)_aug_1162.jpg | tomato_pakistan_2026:train:photo-9_aug_1128_aug_928.jpg | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9878 | plantdoc:train:gls99-anamosa.jpg | plantdoc:train:gls99-anamosa-400x334.jpg | corn/corn_gray_leaf_spot / corn/corn_northern_leaf_blight |
| 0.9888 | tomato_pakistan_2026:train:1342017-162221-44540_jpg.rf.fe408cb959432a34c574c5cb9e2f2c4f_aug_105.jpg | tomato_pakistan_2026:train:1342017-162221-44540_jpg.rf.fe408cb959432a34c574c5cb9e2f2c4f_aug_111.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9888 | tomato_pakistan_2026:train:h69_mirror_aug_669.jpg | tomato_pakistan_2026:train:h71_change_180_aug_772.jpg | tomato/healthy / tomato/healthy |
| 0.9897 | tomato_pakistan_2026:train:d-to-tylv-fo005-14496A31BE70553130A_aug_240.jpg | tomato_pakistan_2026:train:d-to-tylv-fo005-14496A31BE70553130A_jpg.rf.017e4558f1b4baeadc32a13beb178a6e_aug_576.jpg | tomato/tomato_yellow_leaf_curl_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9927 | tomato_pakistan_2026:train:IMG_20251102_155435_aug_989.jpg | tomato_pakistan_2026:train:IMG_20251102_155435_aug_1196.jpg | tomato/tomato_early_blight / tomato/tomato_yellow_leaf_curl_virus |
| 0.9966 | tomato_pakistan_2026:train:Diseased-Tomato-Plant_aug_704.jpg | tomato_pakistan_2026:train:Diseased-Tomato-Plant_aug_710.jpg | tomato/tomato_early_blight / tomato/tomato_early_blight |
