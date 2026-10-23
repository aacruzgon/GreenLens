# VLM benchmark: 2026-10-22-qwen35-awq-desktop

## Conditions

Added by hand: `vlm_bench`'s report records none of them. The per-case answers, the memory samples and `nvidia-smi`'s process lists are in [`vlm-bench-2026-10-22-qwen35-awq-desktop-run.json`](vlm-bench-2026-10-22-qwen35-awq-desktop-run.json).

- **Cases:** the bench's Oct 20 file of 65 one-photo dev cases (`cases.jsonl`, SHA-256 `d454b133…`), built from the evaluation manifest at `6d33aed` (103 cases, SHA-256 `378494e8…`). That is the dev split as labelled before the second review ([#135](https://github.com/aacruzgon/GreenLens/pull/135)) and the replacement cases ([#140](https://github.com/aacruzgon/GreenLens/pull/140)). 26 diagnosed `sure` cases are scored, and 11 are healthy. On `main` after #140, 7 of those 26 are still `sure`, 12 are `probable` and 7 are dropped, so "4 of 26" is on labels since superseded. It compares like with like only with Oct 20's run on the same file.
- **Worker and prompts:** `main` at `63790c0`, `system@2`, `findings@5`, `indicators@2`, `verify@2`, `guided@1`.
- **Server:** Qwen3.5-9B AWQ (`cyankiwi/Qwen3.5-9B-AWQ-4bit` at `156edc4b…`) with thinking off, in `vllm/vllm-openai@sha256:c1c9f6fd…`, at `--gpu-memory-utilization 0.78` and `--max-num-seqs 2`, 1,003,520 pixels, 2 jobs at once.
- **Desktop:** KDE Plasma (Wayland) freshly logged in on the same card, with no apps open: 330 MiB in 6 graphics processes (`nvidia-smi` before vLLM started).
- **Start-up** (vLLM's log): 8.41 GiB of weights in 15 s, CUDA graphs in 55 s, 2.45 GiB of KV cache, 65,536 tokens, 4.00 times the concurrency at 16,384. Ready after 151 s (polled every 10 s).
- **Memory** (`nvidia-smi` once a second, the whole card): 13,058 of 16,303 MiB at the start-up peak, 149 s in; 12,796 MiB at the run's peak, the table's figure. vLLM's process held 11,730 MiB when ready and 12,458 MiB after the run.
- **Preemptions:** 0 (vLLM's `/metrics` after the run).
- **Against Oct 20's run on the same file** ([`vlm-bench-2026-10-20-qwen35-awq-desk`](vlm-bench-2026-10-20-qwen35-awq-desk.md), headless, on `main`'s prompts of that day: `system@1`, `findings@2`, `indicators@1`): 53 of the 585 answers changed, in 38 cases, and the reasons' median length fell from 51 to 33 characters. [#137](https://github.com/aacruzgon/GreenLens/issues/137)'s headless runs on the same file trace the lower diagnosis count to `indicators@2`'s 40-character reason, which `indicators@3` puts back to 60 ([#142](https://github.com/aacruzgon/GreenLens/pull/142)).

## Results

| Run | Pixels | At once | Finished | Retried calls | Job p50 | Job p95 | Species | Diagnosis (sure) | Photos under 1 MP / 1 MP and up | Ties | Healthy kept clean | Yes on healthy | Unsure | Damage found | Organisms | Peak GPU |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| qwen35-awq-desktop | 1,003,520 | 2 | 65/65 | 0 | 6.0 s | 7.9 s | 98% | 15% (n=26) | 3/11 / 1/15 | 5 | 91% (n=11) | 1% | 27% | other 65, leaf_mines 9, chewed_margins 7, sooty_mould 1, frass 1 | 1 | 12,796 MiB |

Diagnosis is the top candidate by its unweighted share of "yes" answers. The payload carries no indicator weights and no disease-head scores, so it measures the VLM's answers alone, not the app's diagnosis.

## Indicators: qwen35-awq-desktop, 1,003,520 pixels, 2 at once

`sure` cases only: a `probable` case counts on neither side.

| Indicator | Cases with the problem: yes / no / unsure | Other cases: yes / no / unsure |
|---|---|---|
| `corn_common_rust.elongated_pustules` | 0 / 3 / 0 | 0 / 7 / 0 |
| `corn_common_rust.pustules_both_sides` | 0 / 2 / 1 | 0 / 7 / 0 |
| `corn_common_rust.raised_pustules` | 0 / 3 / 0 | 0 / 7 / 0 |
| `corn_gray_leaf_spot.bounded_by_veins` | 0 / 2 / 0 | 3 / 5 / 0 |
| `corn_gray_leaf_spot.grey_tan_colour` | 2 / 0 / 0 | 0 / 8 / 0 |
| `corn_gray_leaf_spot.rectangular_lesions` | 0 / 2 / 0 | 1 / 7 / 0 |
| `corn_northern_leaf_blight.cigar_shaped_lesions` | 1 / 2 / 0 | 1 / 6 / 0 |
| `corn_northern_leaf_blight.lesions_cross_veins` | 0 / 3 / 0 | 0 / 7 / 0 |
| `corn_northern_leaf_blight.lower_leaves_first` | 0 / 0 / 3 | 0 / 1 / 6 |
| `tomato_early_blight.concentric_rings` | 0 / 6 / 0 | 0 / 25 / 0 |
| `tomato_early_blight.lower_leaves_first` | 0 / 0 / 6 | 0 / 0 / 25 |
| `tomato_early_blight.yellow_halo` | 2 / 4 / 0 | 3 / 22 / 0 |
| `tomato_late_blight.dark_stem_lesions` | 0 / 1 / 7 | 0 / 5 / 18 |
| `tomato_late_blight.water_soaked_patches` | 0 / 8 / 0 | 0 / 23 / 0 |
| `tomato_late_blight.white_mould_underside` | 0 / 0 / 8 | 0 / 0 / 23 |
| `tomato_septoria_leaf_spot.dark_specks_in_centre` | 0 / 4 / 0 | 0 / 27 / 0 |
| `tomato_septoria_leaf_spot.many_spots_per_leaf` | 1 / 3 / 0 | 7 / 20 / 0 |
| `tomato_septoria_leaf_spot.small_spots` | 1 / 3 / 0 | 12 / 15 / 0 |
