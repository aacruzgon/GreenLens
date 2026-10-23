# Species stage on the dev cases, 2026-10-22

`python -m ml.training.eval.species_check --head species-head-v1-2026-10-20 --out species-check-2026-10-22` (species-check-2026-10-22.json has every number). 152 plant shots of 115 dev cases (flower_fruit 3, leaf_closeup 30, leaf_underside 4, whole_plant 115); the species head `species-head-v1-2026-10-20` names tomato, apple, grape, pepper, corn.

Zero-shot is over the matrix's species rows at the manifest's `species_temperature` 1; "head" is the species head alone; "average" is models.md 6's rule (with every species of the matrix one of the head's crops, the plain average). Rows without "all" count only the head's crops, so the three compare.

## Matrix `233482ce598f591b`: 20 species

STAND-IN: 20 species from ml/training/labels/standin_labels.yaml, BioCLIP 2 text tower, 80 templates.

| Scored | Method | n | Top-1 | Top-3 | Per species (right / n) |
|---|---|---|---|---|---|
| per shot | zero-shot, all shots | 152 | 0.572 | 0.743 | apple 24/28, corn 14/16, grape 18/19, squash 1/19, tomato 30/70 |
| per shot | zero-shot | 133 | 0.647 | 0.789 | apple 24/28, corn 14/16, grape 18/19, tomato 30/70 |
| per shot | head | 133 | 0.902 | 1.000 | apple 27/28, corn 16/16, grape 17/19, tomato 60/70 |
| per shot | average | 133 | 0.669 | 0.820 | apple 24/28, corn 14/16, grape 18/19, tomato 33/70 |
| per case | zero-shot, all cases | 115 | 0.557 | 0.722 | apple 12/15, corn 14/16, grape 10/11, squash 0/10, tomato 28/63 |
| per case | zero-shot | 105 | 0.610 | 0.752 | apple 12/15, corn 14/16, grape 10/11, tomato 28/63 |
| per case | head | 105 | 0.905 | 1.000 | apple 14/15, corn 16/16, grape 9/11, tomato 56/63 |
| per case | average | 105 | 0.648 | 0.810 | apple 12/15, corn 14/16, grape 10/11, tomato 32/63 |

## Matrix `edb29fe878952e7b`: 20 species

the knowledge seed (#51, #131): 20 species (tomato full, the rest label_only) and 5 organisms, BioCLIP 2 text tower, 80 templates; ml/training/labels/bioclip_text.py.

| Scored | Method | n | Top-1 | Top-3 | Per species (right / n) |
|---|---|---|---|---|---|
| per shot | zero-shot, all shots | 152 | 0.586 | 0.743 | apple 24/28, corn 14/16, grape 18/19, squash 3/19, tomato 30/70 |
| per shot | zero-shot | 133 | 0.647 | 0.789 | apple 24/28, corn 14/16, grape 18/19, tomato 30/70 |
| per shot | head | 133 | 0.902 | 1.000 | apple 27/28, corn 16/16, grape 17/19, tomato 60/70 |
| per shot | average | 133 | 0.669 | 0.820 | apple 24/28, corn 14/16, grape 18/19, tomato 33/70 |
| per case | zero-shot, all cases | 115 | 0.557 | 0.713 | apple 12/15, corn 14/16, grape 10/11, squash 0/10, tomato 28/63 |
| per case | zero-shot | 105 | 0.610 | 0.752 | apple 12/15, corn 14/16, grape 10/11, tomato 28/63 |
| per case | head | 105 | 0.905 | 1.000 | apple 14/15, corn 16/16, grape 9/11, tomato 56/63 |
| per case | average | 105 | 0.648 | 0.800 | apple 12/15, corn 14/16, grape 10/11, tomato 32/63 |
