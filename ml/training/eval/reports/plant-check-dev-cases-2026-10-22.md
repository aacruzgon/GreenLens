# Plant check on the evaluation cases: plant-check-dev-cases-2026-10-22

102 cases, 130 plant shots (flower_fruit 5, leaf_closeup 21, leaf_underside 2, whole_plant 102). Every case has a plant. A case passes when any shot passes (leaf score ≥ τ_leaf or plant probability ≥ τ_plant). Hold-out negatives: `ml/training/eval/reports/plant-check-test-holdout-2026-10-20-probe-v1.json`; val per dataset: `ml/training/eval/reports/plant-check-val-2026-10-20-probe-v1.json`.

| Pair | τ_plant | Cases passing | Shots passing | Hold-out no-plant photos passing | Val, worst dataset's false `no_plant_found` |
|---|---|---|---|---|---|
| A 0.4 / 0.7 | 0.7 | 99.0% | 99.2% | 11.0% | 0.6% |
| A, highest τ_plant keeping 99% of cases | 0.98 | 99.0% | | 5.0% | 3.0% |
| plant-probe-v1-2026-10-20 0.99 / 0.71 | 0.71 | 100.0% | 100.0% | 0.1% | 0.3% |
| plant-probe-v1-2026-10-20, highest τ_plant keeping 99% of cases | 0.96 | 99.0% | | 0.0% | 3.0% |

Failing at A 0.4 / 0.7: `tomato-inat-401855015` (tomato, best leaf 0.36, best p 0.56).
Failing at plant-probe-v1-2026-10-20 0.99 / 0.71: none.
