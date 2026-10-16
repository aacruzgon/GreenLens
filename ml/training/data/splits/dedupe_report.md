# Deduplication report

Threshold: cosine ≥ 0.98 on BioCLIP 2 embeddings (b9016a6865e188c0).

- Images: 2578; duplicate pairs: 77 (19 across published splits, 36 with different labels)
- Groups dropped for conflicting labels: 31 (65 images)
- Duplicates removed from kept groups: 37
- Kept: train 1907, val 344, test 225

## Per dataset and split (kept)

| Dataset | train | val | test |
|---|---|---|---|
| plantdoc | 1907 | 344 | 225 |

## Sample pairs to check by eye

| cosine | a | b | labels |
|---|---|---|---|
| 0.9800 | plantdoc:train:g1902-1.jpg | plantdoc:test:1321189.jpg | corn/corn_gray_leaf_spot / corn/corn_northern_leaf_blight |
| 0.9800 | plantdoc:train:d8731a01428301a3c46a643d12c6093d.jpg | plantdoc:train:village-herbals-bulk-herbs-raspberry-leaves.jpg | raspberry/healthy / raspberry/healthy |
| 0.9814 | plantdoc:train:Blight-leaf-.jpg | plantdoc:train:126689-004-D622CD2F.jpg | potato/potato_early_blight / tomato/tomato_early_blight |
| 0.9814 | plantdoc:train:green-apple-leaf-slice-isolated-white-ripe-background-clipping-path-42352971.jpg | plantdoc:train:green-apple-leaf-slice-isolated-white-ripe-background-clipping-path-44598638.jpg | apple/healthy / apple/healthy |
| 0.9824 | plantdoc:train:171322132-56a6d3693df78cf7729070d3.jpg | plantdoc:train:c-204-raspberry.jpg | raspberry/healthy / raspberry/healthy |
| 0.9829 | plantdoc:train:stock-photo-peach-with-leaf-699537772.jpg | plantdoc:train:stock-photo-peach-with-leaf-isolated-on-white-with-clipping-path-494602609.jpg | peach/healthy / peach/healthy |
| 0.9829 | plantdoc:train:Sept9-300x225.jpg | plantdoc:train:FS547-4-big.jpg | tomato/tomato_septoria_leaf_spot / tomato/tomato_bacterial_spot |
| 0.9839 | plantdoc:train:gray_leaf_spot.jpg | plantdoc:test:07c.jpg | corn/corn_gray_leaf_spot / corn/corn_northern_leaf_blight |
| 0.9858 | plantdoc:train:ab40cce958c60d8b6e0ec2e4fdb8f526.jpg | plantdoc:train:Tomato2.jpg | tomato/tomato_mosaic_virus / tomato/tomato_yellow_leaf_curl_virus |
| 0.9868 | plantdoc:train:EBTomato.jpg | plantdoc:train:tomatoblight.jpg | tomato/tomato_early_blight / tomato/tomato_early_blight |
| 0.9878 | plantdoc:train:Blight-leaf-.jpg | plantdoc:train:Symptom_potato_late_blight.jpg | potato/potato_early_blight / potato/potato_late_blight |
| 0.9897 | plantdoc:train:Septoria-Tomato.jpg | plantdoc:train:tomatofactsheet.png.png | tomato/tomato_septoria_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9902 | plantdoc:train:Cherry-Fruit-and-Leaves-tp1263.jpg | plantdoc:train:i-have-a-cherry-tree-5252.jpg | cherry/healthy / cherry/healthy |
| 0.9902 | plantdoc:train:00gb.jpg | plantdoc:train:0gb.jpg | grape/grape_black_rot / grape/grape_black_rot |
| 0.9912 | plantdoc:train:36-early-blight-concentric-rings.jpg | plantdoc:train:tomatoblight.jpg | potato/potato_early_blight / tomato/tomato_early_blight |
| 0.9917 | plantdoc:train:tomato-late-blight-foliar-lesions_Scot-Nelson_CC-BY-SA-2.0.jpg | plantdoc:test:5816740026_d42ef24413_Phytophthora-Infestans.jpg | tomato/tomato_late_blight / potato/potato_late_blight |
| 0.9917 | plantdoc:train:14321627273_58d893c3e2_potato-plant.jpg | plantdoc:train:14321627273_58d893c3e2_b.jpg | potato/potato_early_blight / potato/potato_late_blight |
| 0.9937 | plantdoc:train:Septoria-leaf-spot.jpg | plantdoc:train:tomato-blight-treatment-blight.jpg | pepper/pepper_bacterial_leaf_spot / tomato/tomato_septoria_leaf_spot |
| 0.9956 | plantdoc:train:20130802_111648.jpg | plantdoc:train:thumb-20130802_111648.jpg | apple/apple_cedar_apple_rust / apple/apple_cedar_apple_rust |
| 0.9971 | plantdoc:train:924977LateBlightLeaf.jpg | plantdoc:train:LateBlightLeaf.jpg | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9985 | plantdoc:train:Jessore-potato-late-blight-8-2h2fdkl.jpg | plantdoc:train:symptoms_of_late_blight_0.jpg | potato/potato_late_blight / tomato/tomato_late_blight |
| 0.9990 | plantdoc:train:image_1500x1500%253E.jpeg?1467983528.jpg | plantdoc:train:image_original.jpeg?1472603886.jpg | pepper/pepper_bacterial_leaf_spot / pepper/pepper_bacterial_leaf_spot |
| 0.9995 | plantdoc:train:image_1500x1500%253E.jpeg?1472603885.jpg | plantdoc:train:image_original.jpeg?1472603885.jpg | pepper/pepper_bacterial_leaf_spot / pepper/pepper_bacterial_leaf_spot |
| 0.9995 | plantdoc:train:20150619d.jpg | plantdoc:train:IMG_5360.jpg | tomato/tomato_late_blight / tomato/tomato_late_blight |
| 0.9995 | plantdoc:train:Figure-2-.jpg | plantdoc:train:Northern-corn-leaf-blight-NM-F2.jpg | corn/corn_northern_leaf_blight / corn/corn_northern_leaf_blight |
| 1.0000 | plantdoc:train:earlyblightpotato.jpg | plantdoc:test:earlyblightpotato.jpg | tomato/tomato_early_blight / potato/potato_early_blight |
| 1.0000 | plantdoc:train:backus-056-potato-blight.jpg | plantdoc:test:backus-056-potato-blight.jpg | potato/potato_late_blight / potato/potato_early_blight |
| 1.0000 | plantdoc:train:1421_0.jpeg?itok=FMtmgePj.jpg | plantdoc:test:1421_0.jpeg?itok=FMtmgePj.jpg | potato/potato_early_blight / potato/potato_late_blight |
| 1.0000 | plantdoc:train:corn-gray-leaf-spot-f4.jpg | plantdoc:test:corn-gray-leaf-spot-f4.jpg | corn/corn_gray_leaf_spot / corn/corn_northern_leaf_blight |
| 1.0010 | plantdoc:train:irish-blight-symptoms-on-potato-leaves-atmf8b.jpg | plantdoc:test:irish-blight-symptoms-on-potato-leaves-atmf8b.jpg | potato/potato_early_blight / potato/potato_late_blight |
