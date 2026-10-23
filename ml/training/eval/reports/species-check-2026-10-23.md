# Species stage on the dev cases, 2026-10-23

`python -m ml.training.eval.species_check --head species-head-v1-2026-10-20 --out species-check-2026-10-23` (species-check-2026-10-23.json has every number). 130 plant shots of 102 dev cases (flower_fruit 5, leaf_closeup 21, leaf_underside 2, whole_plant 102); the species head `species-head-v1-2026-10-20` names tomato, apple, grape, pepper, corn.

Zero-shot is over the matrix's species rows at the manifest's `species_temperature` 1.3747; "head" is the species head alone; "average" is models.md 6's rule (with every species of the matrix one of the head's crops, the plain average). Rows without "all" count only the head's crops, so the three compare.

## Matrix `233482ce598f591b`: 20 species

STAND-IN: 20 species from ml/training/labels/standin_labels.yaml, BioCLIP 2 text tower, 80 templates.

Zero-shot's share of the head's crops, on their 129 shots: median 0.935, lowest 0.024, under one half on 29.

| Scored | Method | n | Top-1 | Top-3 | Per species (right / n) |
|---|---|---|---|---|---|
| per shot | zero-shot, all shots | 130 | 0.754 | 0.908 | apple 29/31, corn 15/18, grape 19/22, squash 0/1, tomato 35/58 |
| per shot | zero-shot | 129 | 0.760 | 0.907 | apple 29/31, corn 15/18, grape 19/22, tomato 35/58 |
| per shot | head | 129 | 0.969 | 1.000 | apple 30/31, corn 18/18, grape 19/22, tomato 58/58 |
| per shot | average | 129 | 0.783 | 0.930 | apple 29/31, corn 15/18, grape 19/22, tomato 38/58 |
| per shot | head in share | 129 | 0.791 | 0.938 | apple 28/31, corn 16/18, grape 19/22, tomato 39/58 |
| per case | zero-shot, all cases | 102 | 0.735 | 0.902 | apple 14/15, corn 15/18, grape 12/13, squash 0/1, tomato 34/55 |
| per case | zero-shot | 101 | 0.743 | 0.901 | apple 14/15, corn 15/18, grape 12/13, tomato 34/55 |
| per case | head | 101 | 0.990 | 1.000 | apple 15/15, corn 18/18, grape 12/13, tomato 55/55 |
| per case | average | 101 | 0.772 | 0.931 | apple 14/15, corn 15/18, grape 12/13, tomato 37/55 |
| per case | head in share | 101 | 0.782 | 0.941 | apple 14/15, corn 16/18, grape 12/13, tomato 37/55 |

Zero-shot's top 1 per case, by true species:

- apple: apple 14, peach 1
- corn: corn 15, hosta 2, soybean 1
- grape: grape 12, apple 1
- squash: cucumber 1
- tomato: tomato 34, potato 7, pepper 3, soybean 3, peach 2, blueberry 2, bean 2, cucumber 1, apple 1

## Matrix `edb29fe878952e7b`: 20 species

the knowledge seed (#51, #131): 20 species (tomato full, the rest label_only) and 5 organisms, BioCLIP 2 text tower, 80 templates; ml/training/labels/bioclip_text.py.

Zero-shot's share of the head's crops, on their 129 shots: median 0.935, lowest 0.024, under one half on 29.

| Scored | Method | n | Top-1 | Top-3 | Per species (right / n) |
|---|---|---|---|---|---|
| per shot | zero-shot, all shots | 130 | 0.754 | 0.908 | apple 29/31, corn 15/18, grape 19/22, squash 0/1, tomato 35/58 |
| per shot | zero-shot | 129 | 0.760 | 0.907 | apple 29/31, corn 15/18, grape 19/22, tomato 35/58 |
| per shot | head | 129 | 0.969 | 1.000 | apple 30/31, corn 18/18, grape 19/22, tomato 58/58 |
| per shot | average | 129 | 0.783 | 0.930 | apple 29/31, corn 15/18, grape 19/22, tomato 38/58 |
| per shot | head in share | 129 | 0.791 | 0.938 | apple 28/31, corn 16/18, grape 19/22, tomato 39/58 |
| per case | zero-shot, all cases | 102 | 0.735 | 0.902 | apple 14/15, corn 15/18, grape 12/13, squash 0/1, tomato 34/55 |
| per case | zero-shot | 101 | 0.743 | 0.901 | apple 14/15, corn 15/18, grape 12/13, tomato 34/55 |
| per case | head | 101 | 0.990 | 1.000 | apple 15/15, corn 18/18, grape 12/13, tomato 55/55 |
| per case | average | 101 | 0.772 | 0.931 | apple 14/15, corn 15/18, grape 12/13, tomato 37/55 |
| per case | head in share | 101 | 0.782 | 0.941 | apple 14/15, corn 16/18, grape 12/13, tomato 37/55 |

Zero-shot's top 1 per case, by true species:

- apple: apple 14, peach 1
- corn: corn 15, hosta 2, soybean 1
- grape: grape 12, apple 1
- squash: cucumber 1
- tomato: tomato 34, potato 7, pepper 3, soybean 3, peach 2, blueberry 2, bean 2, squash 1, apple 1
