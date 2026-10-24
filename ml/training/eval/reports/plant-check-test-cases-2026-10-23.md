# Plant check on the evaluation cases: plant-check-test-cases-2026-10-23

58 cases, 73 plant shots (flower_fruit 3, leaf_closeup 8, leaf_underside 4, whole_plant 58). Every case has a plant. A case passes when any shot passes (leaf score ≥ τ_leaf or plant probability ≥ τ_plant). Hold-out negatives: `None`; val per dataset: `None`.

Made with `python -m ml.training.eval.plant_check --cases ml/training/eval/manifest.jsonl --case-split test --probe ml/models/files/plant_probe/plant-probe-v2-2026-10-23.json --pair plant-probe-v2-2026-10-23:0.94:0.31 --label plant-check-test-cases-2026-10-23` on 2026-10-23, over `ml/training/eval/manifest.jsonl` (SHA-256 `e48d59e7852da853`).

| Pair | τ_plant | Cases passing | Shots passing | Hold-out no-plant photos passing | Val, worst dataset's false `no_plant_found` | Val, whole plants failing |
|---|---|---|---|---|---|---|
| plant-probe-v2-2026-10-23 0.94 / 0.31 | 0.31 | 100.0% | 100.0% | – | – | – |
| plant-probe-v2-2026-10-23, highest τ_plant keeping 99% of cases | 0.98 | 100.0% | | – | – | – |

Failing at plant-probe-v2-2026-10-23 0.94 / 0.31: none.

Added by hand after Alan's review of #192: `plant_check` prints the "highest τ_plant keeping 99% of cases" row on every run. On the test split nothing reads it, and the shipped `τ_plant` stays 0.31 (chosen on dev and the hold-out, M17).
