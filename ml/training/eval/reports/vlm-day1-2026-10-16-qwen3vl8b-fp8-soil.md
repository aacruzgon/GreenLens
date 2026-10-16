# VLM day-1 check: vlm-day1-2026-10-16-qwen3vl8b-fp8-soil

Run on 2026-10-16T21:20:30+00:00 against `http://127.0.0.1:8000/v1`, model `Qwen3-VL-8B-Instruct-FP8` served as `vlm`. Images were prepared to at most 1,003,520 pixels with 32-pixel cells and sent as JPEG at quality 90; boxes are `norm_1000`. The raw responses are in `vlm-day1-2026-10-16-qwen3vl8b-fp8-soil.json` and the drawn boxes in `vlm-day1-2026-10-16-qwen3vl8b-fp8-soil/`.

## Checks

| Check | Result | Details |
|---|---|---|
| a. findings | pass | 3170 prompt and 153 completion tokens in 1.853 s; organisms 0, damage findings 1, 8 of 8 soil aspects null |
| b. indicators | pass | answers {'q1_concentric_rings': 'no', 'q2_small_spots_dark_border': 'no', 'q3_water_soaked_patches': 'no'}; 1.425 s; cached prompt tokens not reported |
| c. maxLength | pass | 10 characters: 'Growing西红柿' |
| d. anyOf-null | pass | 8 of 8 soil aspects null; 1.911 s |
| e. timing | pass | first token after 0.033 s, 84.0 tokens/s decode, 1.854 s in all |
| f. anyOf non-null | pass | 1540587537_1661430361118.jpg plain: 0 of 8 soil aspects with a value (none); 1540587537_1661430361118.jpg nudged: 8 of 8 soil aspects with a value (moisture moist, surface cracked, structure crumbly, colour medium, cover bare, deposits none, erosion none, weeds few); 1906382600_1662021022340.jpg plain: 0 of 8 soil aspects with a value (none); 1906382600_1662021022340.jpg nudged: 8 of 8 soil aspects with a value (moisture wet, surface cracked, structure cloddy, colour dark, cover bare, deposits none, erosion none, weeds few) |
| g. grammar blocks | pass | validates; returned soil.moisture.value "wet" (asked "saturated"), soil.weeds.value "few" (asked "some"), soil.colour.certainty "high" (asked "absolute"), species_guess.certainty "high" (asked "certain"), organisms[0].count "many" (asked 25), organisms[0].bbox_2d [0, 0, 150, 150] (asked [0, 0, 1500, 1500]); without the schema the model wrote 5 of 6: soil.moisture.value, soil.weeds.value, species_guess.certainty, organisms[0].count, organisms[0].bbox_2d |

## Images

| Alias | File | Original | Prepared | JPEG bytes |
|---|---|---|---|---|
| img1 | `prunsero_leaf2.jpg` | 2000 × 3008 | 800 × 1216 | 290,145 |
| img2 | `fresh-soy-leaves-isolated-on-260nw-667193923.jpg` | 317 × 280 | 288 × 256 | 7,962 |
| img3 | `1684.jpg` | 563 × 631 | 544 × 608 | 50,989 |
| img4 | `Apple-Leaf-Wallpaper-17.jpg` | 3840 × 2160 | 1312 × 736 | 155,874 |
| soil1 | `1540587537_1661430361118.jpg` | 493 × 493 | 480 × 480 | 67,432 |
| soil2 | `1906382600_1662021022340.jpg` | 485 × 485 | 480 × 480 | 95,881 |

## Wall time per call

| Call | Seconds |
|---|---|
| findings | 1.853 |
| indicators | 1.425 |
| maxlength | 0.126 |
| findings_null | 1.911 |
| findings_stream | 1.854 |
| findings_soil1_plain | 1.949 |
| findings_soil1_nudged | 6.723 |
| findings_soil2_plain | 1.055 |
| findings_soil2_nudged | 4.232 |
| findings_out_of_schema | 4.134 |
| findings_out_of_schema_control | 2.112 |

Schema hashes: findings `2e5460c1f235d340`, indicators `8417c3cbfae22c9c`, maxlength `2fb73878e393a514`, findings_1img `d8997542cbd6d911`.
