# Plant check on the evaluation cases: plant-check-dev-cases-2026-10-23-probe-v2

102 cases, 130 plant shots (flower_fruit 5, leaf_closeup 21, leaf_underside 2, whole_plant 102). Every case has a plant. A case passes when any shot passes (leaf score ≥ τ_leaf or plant probability ≥ τ_plant). Hold-out negatives: `ml/training/eval/reports/plant-check-test-holdout-2026-10-23-probe-v2.json`; val per dataset: `ml/training/eval/reports/plant-check-val-2026-10-23-probe-v2.json`.

Made with `python -m ml.training.eval.plant_check --cases ml/training/eval/manifest.jsonl --probe ml/training/runs/heads/plant-probe-v2-2026-10-23/plant-probe-v2-2026-10-23.json --pair A:0.40:0.70 --pair plant-probe-v2-2026-10-23:0.94:0.31 --holdout-report ml/training/eval/reports/plant-check-test-holdout-2026-10-23-probe-v2.json --val-report ml/training/eval/reports/plant-check-val-2026-10-23-probe-v2.json --label plant-check-dev-cases-2026-10-23-probe-v2` on 2026-10-23, over `ml/training/eval/manifest.jsonl` (SHA-256 `e48d59e7852da853`).

| Pair | τ_plant | Cases passing | Shots passing | Hold-out no-plant photos passing | Val, worst dataset's false `no_plant_found` | Val, whole plants failing |
|---|---|---|---|---|---|---|
| A 0.4 / 0.7 | 0.7 | 99.0% | 99.2% | 11.0% | 0.6% | 2.2% |
| A, highest τ_plant keeping 99% of cases | 0.98 | 99.0% | | 5.0% | 3.0% | 7.8% |
| plant-probe-v2-2026-10-23 0.94 / 0.31 | 0.31 | 100.0% | 100.0% | 0.3% | 0.0% | 1.0% |
| plant-probe-v2-2026-10-23, highest τ_plant keeping 99% of cases | 0.91 | 99.0% | | 0.0% | 2.4% | 6.2% |

Failing at A 0.4 / 0.7: `tomato-inat-401855015` (tomato, best leaf 0.36, best p 0.56).
Failing at plant-probe-v2-2026-10-23 0.94 / 0.31: none.
