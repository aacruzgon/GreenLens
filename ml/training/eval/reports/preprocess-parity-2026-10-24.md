# Pre-processing parity: preprocess-parity-2026-10-24

J1 ([dev-roadmap](../../../../docs/vision/dev-roadmap.md), gap G8), 2026-10-24: `python -m ml.training.eval.preprocess_parity run --models-dir ~/.cache/pdm-models-faces` on commit `24649526`. The `vision` crate's `preprocess_dump` example sends 10 dev photos through the API's `UploadProcessor::process` (as `whole_plant` shots with no depth map, `yunet-2026may@26cc381e` on the CPU), runs the perception worker's steps on what it stored and on 5 leaf crops, and writes every image and tensor; `ml/pdm_vision/preprocess.py` makes its own from the same pixels and from the photo files. Inputs: `ml/training/eval/manifest.jsonl` (SHA-256 `e562c89716fc452f…`), the recipes and `crop_grow` (0.1) of `ml/models/manifest.toml` (`a0fc89cb94d1dacd…`), the upload's 1600 px and `WebP` quality 82; rustc 1.99.0 (b940084d7 2026-09-28), Python 3.12.15, Pillow 12.3.0, NumPy 2.5.2.

## Result: fails the 0.02 target, on the worker's side

- **From the same stored pixels,** the largest difference in a tensor is 0.175 for `bioclip`, 0.192 for `leaf_detector`, 0.175 for `sam_encoder` and 0.058 for the leaf crops, against 0.02. One level of 255 is 0.0150 at BioCLIP's smallest standard deviation and 0.0175 at ImageNet's, so the target allows one level. On the worst photo, 0.5% to 2.7% of the values are more than one level apart.
- **The cause is the order of the resize's two passes.** `fast_image_resize` 6.1.0 resizes an 8-bit image vertically first (`Resizer::do_convolution`: "For u8-based images, it is faster to do the vertical pass first"); Pillow resizes horizontally first. Both round and clip to 8 bits between the passes, so Lanczos3's overshoot at a strong edge is clipped at different points. The large differences sit on high-contrast edges with values near 0 or 255.
- **The worker is the side to change.** The heads, the plant probe and the thresholds were fitted on Pillow's pixels. The example's diagnostic `resize_h_first`, the same `fast_image_resize` call as two one-axis passes, horizontal first, brings BioCLIP's tensors and the crops within one level (max 0.0150) and the detector's and SAM's within two (0.0350, at most 2e-05 of a tensor's values). It also brings the upload's 1600 px resize from 9 levels to 2. The change belongs in `imaging::resize`.
- **Horizontal first isn't enough on its own:** the detector's and SAM's 0.035 is still over 0.02, so the target or the fix has to move. The rest is most likely the coefficients' precision: `fast_image_resize` stores them as `i16` with at most 15 bits (`convolution/optimisations.rs`, `MAX_COEFFS_PRECISION`).
- **The pass order's effect is small:** from the same pixels, BioCLIP's embeddings agree to a cosine similarity of 0.9992 or more on the photos and 0.9977 or more on the crops.
- **The crop's rounding is confirmed:** every crop has the same rectangle and pixels as `pp.crop_box`'s, with the growth clamped at both corners.
- **Decoding differs too:** Pillow's JPEG decoder and `imaging::decode_upload` disagree by up to 5 levels, with 8.6% of the values more than one level apart on the worst photo. That moves the end-to-end numbers, not J1's same-pixels target.

## Larger, and not pre-processing: what the API stores

The API stores each photo as lossy `WebP` at quality 82, and the worker decodes that; `ml/` trains and calibrates on the photo before it (`heads/crops.py` stores at 1600 px with no `WebP`). The round trip moves pixels by 3.3 levels on average on the worst photo (max 37), and BioCLIP's embedding to a cosine similarity of 0.986 to 0.999 on whole photos and 0.876 to 0.998 on the crops. The lowest is a crop of 96 × 77 px (c2), which BioCLIP's recipe scales up to 224 px; the other crops are at 0.974 or more. It needs its own decision (train on round-tripped photos, or store at a higher quality), and J1 doesn't change it. If `ml/` round-trips its photos, Pillow's `libwebp` against the `webp` crate's encoder needs its own parity check first.

The face blur is applied too. `YuNet` found a face on 7 of the 10 photos (p0, p1, p2, p3, p5, p7, p9), all plant photos, so every box is a false one, and the blur moves BioCLIP's embedding to a cosine similarity of 0.660 on the photos and 0.858 on the crops. `ml/` never blurs; whether the API should is D13's question ([#159](https://github.com/aacruzgon/GreenLens/issues/159)).

| BioCLIP embeddings, lowest cosine similarity | Photos | Crops |
|---|---|---|
| Same stored pixels: the worker's code against `pp`'s | 0.9992 | 0.9977 |
| `WebP`: the photo through `WebP` against the photo before it, both through `pp` | 0.9858 | 0.8756 |
| Face blur: the stored photo against the same without it, both through `pp` | 0.6602 | 0.8581 |
| Decode and resize: the API's against Pillow's, before `WebP`, both through `pp` | 0.9995 | 0.9987 |
| End to end: the worker against `ml/`'s training chain | 0.6546 | 0.8446 |

`bioclip/bioclip-2-vision-fp32.onnx` on the CPU, hash checked against `manifest.cpu.toml`.

## Layers

| Layer | Compared | Worst photo, max | Worst photo, mean | Values more than 1 level apart |
|---|---|---|---|---|
| Decode | Pillow (`pp.load_rgb`) against `imaging::decode_upload` | 5 levels | 0.319 | 8.57% |
| Resize to 1600 px | `pp.resize` against `imaging::resize_long_side`, same decoded pixels | 9 levels | 0.229 | 0.20% |
| The same, horizontal first | `pp.resize` against the diagnostic | 2 levels | 0.006 | 0.0001% |
| `WebP` | the resized photo through `WebP` against it | 37 levels | 3.32 | 67.1% |
| Face blur | the stored photo against the same without the blur | 188 levels | 34.57 | 90.5% |

| Tensor | Same pixels, max | Horizontal first, max | End to end, max | End to end, worst photo's mean | One level |
|---|---|---|---|---|---|
| `bioclip` (224 px, shortest_side_crop) | 0.1752 | 0.0150 | 2.6592 | 0.5815 | 0.0150 |
| `leaf_detector` (512 px, squash) | 0.1917 | 0.0349 | 3.3115 | 0.5990 | 0.0175 |
| `sam_encoder` (1024 px, squash) | 0.1751 | 0.0350 | 3.3115 | 0.6006 | 0.0175 |
| Leaf crops, `bioclip` | 0.0584 | 0.0150 | 1.7372 | | 0.0150 |

`pp`'s recipes equal the manifest's: `bioclip` yes, `leaf_detector` yes, `sam_encoder` yes.

## Photos

| Id | Photo | Decoded | Stored | Decode max | Resize max | `WebP` mean | Blurred | `bioclip` same / horizontal first | `leaf_detector` same / horizontal first | `sam_encoder` same / horizontal first |
|---|---|---|---|---|---|---|---|---|---|---|
| p0 | `pepper-inat-409099323#2` | 1536×2048 | 1200×1600 | 5 | 6 | 1.78 | yes | 0.0150 / 0.0150 | 0.0350 / 0.0175 | 0.0350 / 0.0175 |
| p1 | `grape-inat-391631249#1` | 2048×2048 | 1600×1600 | 5 | 4 | 2.03 | yes | 0.0427 / 0.0150 | 0.0350 / 0.0175 | 0.0685 / 0.0342 |
| p2 | `tomato-tomato-bd-2025-yellow-leaf-curl-virus-curl-171#0` | 640×640 | 640×640 | 4 | 0 | 1.68 | yes | 0.0450 / 0.0150 | 0.0871 / 0.0175 | 0.1394 / 0.0350 |
| p3 | `corn-maize-bd-2026-common-rust-img-261#0` | 1701×3024 | 900×1600 | 4 | 5 | 2.55 | yes | 0.0292 / 0.0150 | 0.0349 / 0.0175 | 0.0871 / 0.0350 |
| p4 | `tomato-tomato-bd-iphone-2025-healthy-img-3558-02d#0` | 4032×3024 | 1600×1200 | 4 | 2 | 1.56 | no | 0.0150 / 0.0150 | 0.0349 / 0.0175 | 0.0350 / 0.0350 |
| p5 | `apple-inat-408956748#1` | 800×1200 | 800×1200 | 4 | 0 | 2.08 | yes | 0.0150 / 0.0150 | 0.0350 / 0.0175 | 0.0523 / 0.0350 |
| p6 | `corn-maize-in-2025-pottasium-4-28#0` | 667×679 | 667×679 | 4 | 0 | 1.85 | no | 0.0292 / 0.0150 | 0.0350 / 0.0349 | 0.0350 / 0.0350 |
| p7 | `corn-commons-corn-hail#0` | 2272×1704 | 1600×1200 | 5 | 9 | 3.32 | yes | 0.1752 / 0.0150 | 0.1917 / 0.0175 | 0.1751 / 0.0350 |
| p8 | `corn-commons-corn-magnesium#0` | 1018×555 | 1018×555 | 4 | 0 | 2.17 | no | 0.0284 / 0.0150 | 0.0350 / 0.0175 | 0.0350 / 0.0350 |
| p9 | `tomato-commons-tomato-blossom-end-rot-vine#0` | 1381×1541 | 1381×1541 | 4 | 0 | 2.33 | yes | 0.1314 / 0.0150 | 0.1401 / 0.0175 | 0.1050 / 0.0342 |

## Leaf crops

Boxes on the stored photo, grown by `crop_grow` 0.1 on each side. They test the crop, not the detector.

| Id | Photo | Box (x, y, w, h) | Worker's rectangle (x, y, w, h) | Same as `pp.crop_box` | Same pixels, max | Horizontal first, max | End to end, max |
|---|---|---|---|---|---|---|---|
| c0 | p0 | past the top-left corner: 0.02, 0.03, 0.3, 0.4 | 0, 0, 420, 752 | yes | 0.0150 | 0.0150 | 1.7372 |
| c1 | p1 | past the bottom-right corner: 0.55, 0.5, 0.44, 0.49 | 809, 721, 791, 879 | yes | 0.0569 | 0.0150 | 1.4558 |
| c2 | p2 | fractional edges: 0.3137, 0.2719, 0.1234, 0.0987 | 192, 167, 96, 77 | yes | 0.0584 | 0.0150 | 0.3413 |
| c3 | p3 | under 40 px: 0.4, 0.35, 0.02, 0.03 | 358, 555, 22, 58 | yes | 0.0300 | 0.0150 | 0.2417 |
| c4 | p4 | most of the photo: 0.1, 0.1, 0.8, 0.8 | 31, 23, 1537, 1153 | yes | 0.0150 | 0.0150 | 0.1280 |

## Notes

- Every photo is a dev photo; the test split isn't used.
- The photos carry no EXIF orientation (the evaluation set strips it on import), so the decoders' rotation isn't exercised.
- Each photo goes through `process` as a `whole_plant` shot with no depth map. An upload whose depth map is kept is stored at 2,048 px or more, and isn't covered.
- The crop's rectangle is `classify`'s arithmetic copied into the example, which `pp.crop_box` matches; `classify` doesn't expose it as a function.
- A box of zero area, which the detector never keeps, is cut as an empty image by the worker and as 1 px by `pp.crop_box`.
- The 0.02 target allows one level. Even horizontal first, a few values (2e-05 of a tensor at most) differ by two.
