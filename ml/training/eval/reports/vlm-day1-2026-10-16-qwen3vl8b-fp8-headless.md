# VLM day-1 check: vlm-day1-2026-10-16-qwen3vl8b-fp8-headless

Run on 2026-10-16T18:59:46+00:00 against `http://127.0.0.1:8000/v1`, model `Qwen3-VL-8B-Instruct-FP8` served as `vlm`. Images were prepared to at most 1,003,520 pixels with 32-pixel cells and sent as JPEG at quality 90; boxes are `norm_1000`. The raw responses are in `vlm-day1-2026-10-16-qwen3vl8b-fp8-headless.json` and the drawn boxes in `vlm-day1-2026-10-16-qwen3vl8b-fp8-headless/`.

## Checks

| Check | Result | Details |
|---|---|---|
| a. findings | pass | 3170 prompt and 160 completion tokens in 3.187 s; organisms 0, damage findings 1, 8 of 8 soil aspects null |
| b. indicators | pass | answers {'q1_concentric_rings': 'no', 'q2_small_spots_dark_border': 'no', 'q3_water_soaked_patches': 'no'}; 1.444 s; cached prompt tokens not reported |
| c. maxLength | pass | 10 characters: 'Growing西红柿' |
| d. anyOf-null | pass | 8 of 8 soil aspects null; 2.007 s |
| e. timing | pass | first token after 0.06 s, 83.9 tokens/s decode, 1.967 s in all |

## Images

| Alias | File | Original | Prepared | JPEG bytes |
|---|---|---|---|---|
| img1 | `prunsero_leaf2.jpg` | 2000 × 3008 | 800 × 1216 | 290,145 |
| img2 | `fresh-soy-leaves-isolated-on-260nw-667193923.jpg` | 317 × 280 | 288 × 256 | 7,962 |
| img3 | `1684.jpg` | 563 × 631 | 544 × 608 | 50,989 |
| img4 | `Apple-Leaf-Wallpaper-17.jpg` | 3840 × 2160 | 1312 × 736 | 155,874 |

## Wall time per call

| Call | Seconds |
|---|---|
| findings | 3.187 |
| indicators | 1.444 |
| maxlength | 0.126 |
| findings_null | 2.007 |
| findings_stream | 1.967 |

Schema hashes: findings `2e5460c1f235d340`, indicators `8417c3cbfae22c9c`, maxlength `2fb73878e393a514`, findings_1img `d8997542cbd6d911`.
