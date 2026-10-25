# Pre-processing parity: preprocess-parity-2026-10-24

J1 ([dev-roadmap](../../../../docs/vision/dev-roadmap.md), gap G8), 2026-10-24: `python -m ml.training.eval.preprocess_parity run` on commit `384c2120`. The `vision` crate's `preprocess_dump` example runs the upload's and the perception worker's steps on 10 dev photos and 5 leaf crops and writes every image and tensor; `ml/pdm_vision/preprocess.py` makes its own from the same pixels and from the photo files. Inputs: `ml/training/eval/manifest.jsonl` (SHA-256 `e562c89716fc452f…`), the recipes and `crop_grow` (0.1) of `ml/models/manifest.toml` (`a0fc89cb94d1dacd…`), the upload's 1600 px and `WebP` quality 82; rustc 1.99.0 (b940084d7 2026-09-28), Python 3.12.15, Pillow 12.3.0, NumPy 2.5.2.

## Result: fails the 0.02 target, for one reason, on the worker's side

- **From the same stored pixels,** the largest difference in a tensor is 0.161 for `bioclip`, 0.209 for `leaf_detector`, 0.175 for `sam_encoder` and 0.057 for the leaf crops, against 0.02. One level of 255 is 0.0150 at BioCLIP's smallest standard deviation and 0.0175 at ImageNet's, so the target allows one level. On the worst photo, 0.5% to 2.7% of the values are more than one level apart.
- **The cause is the order of the resize's two passes.** `fast_image_resize` 6.1.0 resizes an 8-bit image vertically first (`Resizer::do_convolution`: "For u8-based images, it is faster to do the vertical pass first"); Pillow resizes horizontally first. Both round and clip to 8 bits between the passes, so Lanczos3's overshoot at a strong edge is clipped at different points. The large differences sit on high-contrast edges with values near 0 or 255.
- **The worker is the side to change.** The heads, the plant probe and the thresholds were fitted on Pillow's pixels. The example's diagnostic `resize_h_first`, the same `fast_image_resize` call as two one-axis passes, horizontal first, brings BioCLIP's tensors and the crops within one level (max 0.0150) and the detector's and SAM's within two (0.0350, at most 2e-05 of a tensor's values, most likely the two libraries' fixed-point rounding). It also brings the upload's 1600 px resize from 9 levels to 2. The change belongs in `imaging::resize`.
- **Its effect is small:** from the same pixels, BioCLIP's embeddings agree to a cosine similarity of 0.9992 or more on the photos and 0.9977 or more on the crops.
- **The crop's rounding is confirmed:** every crop has the same rectangle and pixels as `pp.crop_box`'s, with the growth clamped at both corners.

## Larger, and not pre-processing: the `WebP` the API stores

The API stores each photo as lossy `WebP` at quality 82, and the worker decodes that; `ml/` trains and calibrates on the photo before it (`heads/crops.py` stores at 1600 px with no `WebP`). The round trip moves pixels by 3.3 levels on average on the worst photo (max 37), and BioCLIP's embedding to a cosine similarity of 0.986 on whole photos and 0.876 on a crop of 96 × 77 px (c2), which BioCLIP's recipe scales up to 224 px. It is almost the whole gap between what the heads were trained on and what the worker sends, and it needs its own decision (train on round-tripped photos, or store at a higher quality); J1 doesn't change it.

| BioCLIP embeddings, lowest cosine similarity | Photos | Crops |
|---|---|---|
| Same stored pixels: the worker's code against `pp`'s | 0.9992 | 0.9977 |
| `WebP`: the stored photo against the photo before it, both through `pp` | 0.9858 | 0.8756 |
| Decode and resize: the API's against Pillow's, before `WebP`, both through `pp` | 0.9995 | 0.9987 |
| End to end: the worker against `ml/`'s training chain | 0.9854 | 0.8816 |

`bioclip/bioclip-2-vision-fp32.onnx` on the CPU, hash checked against `manifest.cpu.toml`.

## Layers

| Layer | Compared | Worst photo, max | Worst photo, mean | Values more than 1 level apart |
|---|---|---|---|---|
| Decode | Pillow (`pp.load_rgb`) against `imaging::decode_upload` | 5 levels | 0.319 | 8.57% |
| Resize to 1600 px | `pp.resize` against `imaging::resize_long_side`, same decoded pixels | 9 levels | 0.229 | 0.20% |
| The same, horizontal first | `pp.resize` against the diagnostic | 2 levels | 0.006 | 0.0001% |
| `WebP` | the stored photo against the resized one | 37 levels | 3.32 | 67.1% |

| Tensor | Same pixels, max | Horizontal first, max | End to end, max | End to end, worst photo's mean | One level |
|---|---|---|---|---|---|
| `bioclip` (224 px, shortest_side_crop) | 0.1606 | 0.0150 | 0.1991 | 0.0228 | 0.0150 |
| `leaf_detector` (512 px, squash) | 0.2092 | 0.0349 | 0.3834 | 0.0368 | 0.0175 |
| `sam_encoder` (1024 px, squash) | 0.1751 | 0.0350 | 0.6100 | 0.0483 | 0.0175 |
| Leaf crops, `bioclip` | 0.0569 | 0.0150 | 0.3128 | | 0.0150 |

`pp`'s recipes equal the manifest's: `bioclip` yes, `leaf_detector` yes, `sam_encoder` yes.

## Photos

| Id | Photo | Decoded | Stored | Decode max | Resize max | `WebP` mean | `bioclip` same / horizontal first | `leaf_detector` same / horizontal first | `sam_encoder` same / horizontal first |
|---|---|---|---|---|---|---|---|---|---|
| p0 | `pepper-inat-409099323#2` | 1536×2048 | 1200×1600 | 5 | 6 | 1.78 | 0.1201 / 0.0150 | 0.1046 / 0.0175 | 0.1220 / 0.0349 |
| p1 | `grape-inat-391631249#1` | 2048×2048 | 1600×1600 | 5 | 4 | 2.03 | 0.0569 / 0.0150 | 0.0350 / 0.0175 | 0.0685 / 0.0350 |
| p2 | `tomato-tomato-bd-2025-yellow-leaf-curl-virus-curl-171#0` | 640×640 | 640×640 | 4 | 0 | 1.68 | 0.0300 / 0.0150 | 0.0871 / 0.0175 | 0.1046 / 0.0350 |
| p3 | `corn-maize-bd-2026-common-rust-img-261#0` | 1701×3024 | 900×1600 | 4 | 5 | 2.55 | 0.0292 / 0.0150 | 0.0685 / 0.0175 | 0.1046 / 0.0350 |
| p4 | `tomato-tomato-bd-iphone-2025-healthy-img-3558-02d#0` | 4032×3024 | 1600×1200 | 4 | 2 | 1.56 | 0.0150 / 0.0150 | 0.0349 / 0.0175 | 0.0350 / 0.0350 |
| p5 | `apple-inat-408956748#1` | 800×1200 | 800×1200 | 4 | 0 | 2.08 | 0.0150 / 0.0150 | 0.0523 / 0.0175 | 0.0697 / 0.0350 |
| p6 | `corn-maize-in-2025-pottasium-4-28#0` | 667×679 | 667×679 | 4 | 0 | 1.85 | 0.0292 / 0.0150 | 0.0350 / 0.0349 | 0.0350 / 0.0350 |
| p7 | `corn-commons-corn-hail#0` | 2272×1704 | 1600×1200 | 5 | 9 | 3.32 | 0.1606 / 0.0150 | 0.2092 / 0.0175 | 0.1751 / 0.0350 |
| p8 | `corn-commons-corn-magnesium#0` | 1018×555 | 1018×555 | 4 | 0 | 2.17 | 0.0284 / 0.0150 | 0.0350 / 0.0175 | 0.0350 / 0.0350 |
| p9 | `tomato-commons-tomato-blossom-end-rot-vine#0` | 1381×1541 | 1381×1541 | 4 | 0 | 2.33 | 0.1460 / 0.0150 | 0.1401 / 0.0175 | 0.1050 / 0.0350 |

## Leaf crops

Boxes on the stored photo, grown by `crop_grow` 0.1 on each side. They test the crop, not the detector.

| Id | Photo | Box (x, y, w, h) | Worker's rectangle (x, y, w, h) | Same as `pp.crop_box` | Same pixels, max | Horizontal first, max | End to end, max |
|---|---|---|---|---|---|---|---|
| c0 | p0 | past the top-left corner: 0.02, 0.03, 0.3, 0.4 | 0, 0, 420, 752 | yes | 0.0300 | 0.0150 | 0.1706 |
| c1 | p1 | past the bottom-right corner: 0.55, 0.5, 0.44, 0.49 | 809, 721, 791, 879 | yes | 0.0569 | 0.0150 | 0.1706 |
| c2 | p2 | fractional edges: 0.3137, 0.2719, 0.1234, 0.0987 | 192, 167, 96, 77 | yes | 0.0569 | 0.0150 | 0.3128 |
| c3 | p3 | under 40 px: 0.4, 0.35, 0.02, 0.03 | 358, 555, 22, 58 | yes | 0.0300 | 0.0150 | 0.2275 |
| c4 | p4 | most of the photo: 0.1, 0.1, 0.8, 0.8 | 31, 23, 1537, 1153 | yes | 0.0150 | 0.0150 | 0.1280 |

## Notes

- Every photo is a dev photo; the test split isn't used.
- The photos carry no EXIF orientation (the evaluation set strips it on import), so the decoders' rotation isn't exercised.
- The upload's face blur isn't applied. It changes the same pixels on both sides of the comparison.
- A box of zero area, which the detector never keeps, is cut as an empty image by the worker and as 1 px by `pp.crop_box`.
- The 0.02 target allows one level. Even horizontal first, a few values (2e-05 of a tensor at most) differ by two.
