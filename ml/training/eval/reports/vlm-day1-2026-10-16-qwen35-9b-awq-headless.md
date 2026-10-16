# VLM day-1 check: vlm-day1-2026-10-16-qwen35-9b-awq-headless

Run on 2026-10-16T22:03:09+00:00 against `http://127.0.0.1:8000/v1`, model `Qwen3.5-9B-AWQ-4bit` served as `vlm`. Images were prepared to at most 1,003,520 pixels with 32-pixel cells and sent as JPEG at quality 90; boxes are `norm_1000`. The raw responses are in `vlm-day1-2026-10-16-qwen35-9b-awq-headless.json` and the drawn boxes in `vlm-day1-2026-10-16-qwen35-9b-awq-headless/`.

## Checks

| Check | Result | Details |
|---|---|---|
| a. findings | pass | 3187 prompt and 133 completion tokens in 3.235 s; organisms 0, damage findings 0, 8 of 8 soil aspects null |
| b. indicators | pass | answers {'q1_concentric_rings': 'no', 'q2_small_spots_dark_border': 'yes', 'q3_water_soaked_patches': 'no'}; 1.374 s; cached prompt tokens not reported |
| c. maxLength | pass | 10 characters: 'Growing a ' |
| d. anyOf-null | pass | 8 of 8 soil aspects null; 1.846 s |
| e. timing | pass | first token after 0.069 s, 116.7 tokens/s decode, 1.207 s in all |
| f. anyOf non-null | pass | 1540587537_1661430361118.jpg plain: 0 of 8 soil aspects with a value (none); 1540587537_1661430361118.jpg nudged: 8 of 8 soil aspects with a value (moisture dry, surface cracked, structure cloddy, colour pale, cover bare, deposits none, erosion none, weeds few); 1906382600_1662021022340.jpg plain: 0 of 8 soil aspects with a value (none); 1906382600_1662021022340.jpg nudged: 8 of 8 soil aspects with a value (moisture moist, surface crusted, structure cloddy, colour grey_mottled, cover bare, deposits none, erosion none, weeds few) |
| g. grammar blocks | pass | validates; returned soil.moisture.value "dry" (asked "saturated"), soil.weeds.value "few" (asked "some"), soil.colour.certainty "high" (asked "absolute"), species_guess.certainty "high" (asked "certain"), organisms[0].count "few" (asked 25), organisms[0].bbox_2d [0, 0, 1000, 1000] (asked [0, 0, 1500, 1500]); without the schema the model wrote 5 of 6: soil.weeds.value, soil.colour.certainty, species_guess.certainty, organisms[0].count, organisms[0].bbox_2d |

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
| findings | 3.235 |
| indicators | 1.374 |
| maxlength | 0.106 |
| findings_null | 1.846 |
| findings_stream | 1.207 |
| findings_soil1_plain | 1.702 |
| findings_soil1_nudged | 2.714 |
| findings_soil2_plain | 1.256 |
| findings_soil2_nudged | 2.355 |
| findings_out_of_schema | 3.25 |
| findings_out_of_schema_control | 2.878 |

Schema hashes: findings `2e5460c1f235d340`, indicators `8417c3cbfae22c9c`, maxlength `2fb73878e393a514`, findings_1img `d8997542cbd6d911`.
