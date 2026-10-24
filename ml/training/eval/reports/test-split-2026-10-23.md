# Test split, scored once: test-split-2026-10-23

`python -m ml.training.eval.score_test_split --plant ml/training/eval/reports/plant-check-test-cases-2026-10-23.json --vlm ml/training/runs/vlm-bench/qwen35-awq-test --faces ml/training/runs/faces/raw/test-original-0.3.jsonl --out test-split-2026-10-23`, on 2026-10-23, over `ml/training/eval/manifest.jsonl` (SHA-256 `e48d59e7852da853`), test split: 58 cases. The model manifest is `ml/models/manifest.toml` (SHA-256 `a0fc89cb94d1dacd`). Nothing was fitted or chosen on these cases ([score_test_split.py](../score_test_split.py)).

Shipped values: label matrix `edb29fe878952e7b` at `species_temperature` 1.3747; disease head `disease-head-v3c-2026-10-20` at its file's temperature 1.8205856928246815; plant check `plant-probe-v2-2026-10-23` at `τ_leaf` 0.94 and `τ_plant` 0.31.

Each rate is k of n (rate; 95% Wilson interval). A target is met or missed only when the whole interval is on one side of it, otherwise undecided at that count (evaluation 4). **Without the 7** leaves out the test cases that were dev cases when choices were made: `apple-inat-401743982`, `apple-inat-401742038`, `apple-inat-401735945`, `squash-inat-407881529`, `corn-maize-bd-2026-gray-leaf-spot-img-500`, `tomato-tomato-bd-2025-late-blight-late-blight-7`, `corn-maize-bd-2026-healthy-leaf-img-43`.

| Row | What | Target | Every case | Without the 7 | Met (every case / without the 7) |
|---|---|---|---|---|---|
| T1 | Plant check: cases with a plant that pass | 99% or more | 58 of 58 (100.0%; 94%–100%) | 51 of 51 (100.0%; 93%–100%) | undecided / undecided |
| T2 | Species, BioCLIP 2: top-1 on the five crops | 85% or more | 41 of 56 (73.2%; 60%–83%) | 36 of 50 (72.0%; 58%–83%) | missed / missed |
| T3 | Species, BioCLIP 2: top-3 on the five crops | 95% or more | 52 of 56 (92.9%; 83%–97%) | 46 of 50 (92.0%; 81%–97%) | undecided / undecided |
| T4 | BioCLIP 2 (0.8 or more) and the VLM (medium or high) agree on a wrong species | 3% or less | 0 of 41 (0.0%; 0%–9%) | 0 of 38 (0.0%; 0%–9%) | undecided / undecided |
| T5 | Disease head: true problem in the top 3, per leaf photo with a problem (guessing 60%) | 85% or more | 36 of 43 (83.7%; 70%–92%) | 32 of 38 (84.2%; 70%–93%); also without the 2 repeat leaves | undecided / undecided |
| T5, `sure` | The same on the `sure` cases' photos | 85% or more | 5 of 5 (100.0%; 57%–100%) |  | undecided |
| T6 | Disease head: macro F1 of the top-1, per crop | Report | apple 0.29 (5 photos); corn 0.46 (12 photos); grape 1.00 (6 photos); tomato 0.34 (28 photos) |  | report |
| T7 | Disease head: ECE (15 bins) at the file's temperature, 51 leaf photos | 0.05 or less | 0.210 |  | missed |
| T8 | VLM: calls valid on the first try (two per finished job) | 99% or more | 82 of 82 (100.0%; 96%–100%) | 76 of 76 (100.0%; 95%–100%) | undecided / undecided |
| T8, retry | VLM: jobs finished after one retry | 100% | 41 of 41 | 38 of 38 | met |
| T9 | VLM: species guess, top-1, on its finished jobs | Report beside BioCLIP 2's | 40 of 41 (97.6%; 87%–100%) | 37 of 38 (97.4%; 87%–100%) | report |
| T10 | VLM: job time at p95, 2 at once | Under 150 s | 8.8 s (p50 6.3 s) | 8.3 s | met |
| T11 | End to end through the API |  | not measured: needs cases through the API, and J5's end-to-end harness wasn't built |  |  |
| T12 | End to end through the API |  | not measured: needs cases through the API, and J5's end-to-end harness wasn't built |  |  |
| T13 | End to end through the API |  | not measured: needs cases through the API, and J5's end-to-end harness wasn't built |  |  |
| T14 | End to end through the API |  | not measured: needs cases through the API, and J5's end-to-end harness wasn't built |  |  |
| T15 | End to end through the API |  | not measured: needs cases through the API, and J5's end-to-end harness wasn't built |  |  |
| T16 | End to end through the API |  | not measured: needs cases through the API, and J5's end-to-end harness wasn't built |  |  |
| T17 | Upload: photos the blur and exposure check rejects | 5% or less | 3 of 73 (4.1%; 1%–11%) | 3 of 65 (4.6%; 2%–13%) | undecided / undecided |
| T18 | Upload: photos with a face box (YuNet at 0.3, as uploaded) | 2% or less | 38 of 73 (52.1%; 41%–63%) | 33 of 65 (50.8%; 39%–63%) | missed / missed |

## Details

- **T1.** Cases failing the plant check: none.
- **T2, T3.** Wrong top-1: `apple-inat-402203493` (apple) → cherry, apple, peach; `tomato-tomato-bd-2025-early-blight-early-blight-465` (tomato) → grape, bean, tomato; `tomato-tomato-bd-2025-late-blight-late-blight-41` (tomato) → potato, tomato, soybean; `tomato-tomato-bd-2025-late-blight-late-blight-5` (tomato) → potato, tomato, pepper; `tomato-tomato-bd-2025-late-blight-late-blight-7` (tomato) → pepper, potato, tomato; `tomato-tomato-bd-2025-late-blight-late-blight-80` (tomato) → potato, blueberry, soybean; `tomato-tomato-bd-2025-late-blight-late-blight-94` (tomato) → potato, tomato, pepper; `tomato-tomato-bd-2025-yellow-leaf-curl-virus-curl-1` (tomato) → soybean, tomato, bean; `tomato-tomato-bd-iphone-2025-bacterial-spot-img-3679-02d` (tomato) → pepper, tomato, bean; `tomato-tomato-bd-iphone-2025-bacterial-spot-img-4497-02d` (tomato) → pepper, potato, tomato; `tomato-tomato-bd-iphone-2025-early-blight-img-2624` (tomato) → hosta, lettuce, potato; `tomato-tomato-bd-iphone-2025-early-blight-img-4257-02d` (tomato) → potato, squash, tomato; `tomato-tomato-bd-iphone-2025-late-blight-img-5537-02d` (tomato) → soybean, lettuce, pepper; `tomato-tomato-bd-iphone-2025-leaf-mold-img-4233-02d` (tomato) → potato, tomato, pepper; `tomato-tomato-bd-iphone-2025-leaf-mold-img-4295-02d` (tomato) → potato, bean, soybean.
- **T4.** Cases where both agree on a wrong species: none.
- **T5–T7.** 51 leaf photos; top-1 51%. Top-1 answers per true class: **apple:** scab (5) → cedar apple rust 3, scab 2. **corn:** common rust (3) → common rust 2, gray leaf spot 1; gray leaf spot (3) → northern leaf blight 2, gray leaf spot 1; northern leaf blight (2) → healthy 1, gray leaf spot 1; healthy (4) → healthy 3, gray leaf spot 1. **grape:** black rot (6) → black rot 6. **tomato:** healthy (4) → healthy 3, septoria leaf spot 1; bacterial spot (3) → septoria leaf spot 1, bacterial spot 1, leaf mold 1; early blight (5) → septoria leaf spot 2, early blight 2, late blight 1; late blight (5) → late blight 3, early blight 1, septoria leaf spot 1; leaf mold (3) → septoria leaf spot 2, bacterial spot 1; septoria leaf spot (4) → septoria leaf spot 3, leaf mold 1; spider mites (3) → leaf mold 2, bacterial spot 1; yellow leaf curl virus (1) → bacterial spot 1.
- **T5, left out:** 6 leaf photos with several crops, 2 no leaf shot, 2 species without head classes.
- **T8–T10.** Run `qwen35-awq-test` on cyankiwi/Qwen3.5-9B-AWQ-4bit, 1,003,520 pixels, 2 at once: 41 of 41 jobs ran, 0 calls retried, peak GPU 12,783 MiB. Bench cases `ml/training/runs/vlm-bench/qwen35-awq-test/px1003520-c2/cases.jsonl` (SHA-256 `0f5f939fe0bfb1f8`).
- **T17, T18.** Rejected for: TooBlurry 2, TooBright 1.

Inputs: manifest `ml/training/eval/manifest.jsonl` (`e48d59e7852da853`); models_manifest `ml/models/manifest.toml` (`a0fc89cb94d1dacd`); plant `ml/training/eval/reports/plant-check-test-cases-2026-10-23.json` (`450e3683363ddf28`); disease_head `ml/models/files/disease_head/disease-head-v3c-2026-10-20.json` (`0b035a88f44c6988`); vlm_out `ml/training/runs/vlm-bench/qwen35-awq-test/px1003520-c2/out.jsonl` (`85301d064e1bab01`); faces `ml/training/runs/faces/raw/test-original-0.3.jsonl` (`b46c38688b00b160`).
