# Review: Black rot (`grape_black_rot`)

Draft of 2026-10-22 by `claude-opus-5-5`. Check every line against the excerpts under it, fix or delete what they don't support, then set `reviewed_by` and `reviewed_at` in the YAML. The `discriminating` flags come from discriminate.py.

## Model notes (as first drafted)

Excerpts do not say which leaves (lower or upper) show spots first; 'upper_leaves' is a placeholder for leaves in general. Leaf spot size 'about 1/4 inch' converted to about 6 mm; the max of 6.5 mm is a tight bound for an approximate figure, so the reviewer may want to widen it. No temperature or humidity numbers are given, only 'warm, humid weather'. No look-alike problems are named in the excerpts. The spray interval and timing details for mancozeb/captan were left out as instructed. The excerpt line about anthracnose-susceptible varieties concerns a different disease and was not used. No existing treatment matched (they are written for tomato, apple or corn).

## Indicators

- **grape_black_rot.yellow_leaf_spots** (weight 2, discriminating, upper_leaves, leaf_closeup)
  - Text: Tiny yellow spots on the leaves, which grow larger over time.
  - Question: Are there small yellow spots on the grape leaves?
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

- **grape_black_rot.leaf_spot_size** (weight 2, discriminating, upper_leaves, leaf_closeup; measure {'max': 8, 'quantity': 'lesion_diameter_mm', 'stat': 'max'})
  - Text: Leaf spots grow to about 6 mm (a quarter inch) across.
  - Question: Are the leaf spots small, smaller than a pencil eraser?
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

- **grape_black_rot.rusty_red_spot_centres** (weight 2, discriminating, upper_leaves, leaf_closeup)
  - Text: The centres of the leaf spots turn rusty red.
  - Question: Do the leaf spots have a rusty red centre?
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

- **grape_black_rot.black_dots_in_spots** (weight 2, upper_leaves, leaf_closeup)
  - Text: Tiny black dots usually form in the centre of the leaf spots. They hold spores that start new infections.
  - Question: Are there tiny black dots in the middle of the leaf spots?
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

- **grape_black_rot.rotting_berries** (weight 2, fruit, flower_fruit)
  - Text: Infected berries can rot within a few days. Fruit symptoms show when the berries are about half grown.
  - Question: Are some grape berries rotting?
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

- **grape_black_rot.black_mummified_berries** (weight 2, fruit, flower_fruit)
  - Text: Infected berries shrivel and become hard, black mummies.
  - Question: Are some grape berries shrivelled and black, like raisins?
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

- **grape_black_rot.black_dots_on_mummies** (weight 2, fruit, flower_fruit)
  - Text: The mummified berries carry the same tiny black spore dots as the leaf spots.
  - Question: Do the shrivelled black berries have tiny black dots on their surface?
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

## Conditions

`{"notes": "Warm, humid weather favours black rot and can lead to heavy losses."}`

  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.

## Looks like

(none)

## Treatments

- **canopy_drying_grape** (cultural): Help the vines dry quickly after rain or dew. Prune and train the vines, choose a good row direction, and space the plants properly.
  - `isu_black_rot_grape.2` (https://yardandgarden.extension.iastate.edu/encyclopedia/black-rot-grape, withheld at review: recommends mancozeb or captan as home sprays (S1): run fetch and chunk, then read build/chunks/isu_black_rot_grape.jsonl)

- **prune_destroy_dormant_grape** (cultural): While the vines are dormant, prune out and destroy diseased plant material. Be sure to remove all black rot mummies (shrivelled black berries). The fungus survives the winter mainly in these mummies, and also in other infected plant parts.
  - `isu_black_rot_grape.1`: Black rot, caused by the fungus Guignardia bidwellii, can cause heavy losses to infected grapes, especially during warm, humid weather. Black rot starts as tiny yellow spots that grow to about 1/4 inch on the leaves. The centers of the spots turn rusty red. Tiny black dots usually form in the center of the spots. These dots contain thousands of spores that can produce new infections. Symptoms on the fruit become apparent when the berries are about half grown. Infected berries can rot in a few days. They shrivel and become hard, black mummies, which contain the same tiny spore containing structures as leaf spots. Black rot is a common Iowa grape disease. The primary winter survival mode for the fungus is in mummified berries. However, other infected plant material can be a safe haven for the fungus.
  - `isu_black_rot_grape.2` (https://yardandgarden.extension.iastate.edu/encyclopedia/black-rot-grape, withheld at review: recommends mancozeb or captan as home sprays (S1): run fetch and chunk, then read build/chunks/isu_black_rot_grape.jsonl)

- **remove_damaged_clusters_fallen_berries** (mechanical): Prune out damaged shoots and grape clusters and take the prunings away from the planting. In autumn, rake the ground under the vines to remove all fallen berries.
  - `isu_black_rot_grape.2` (https://yardandgarden.extension.iastate.edu/encyclopedia/black-rot-grape, withheld at review: recommends mancozeb or captan as home sprays (S1): run fetch and chunk, then read build/chunks/isu_black_rot_grape.jsonl)

- **remove_wild_grapes** (cultural): Remove wild grapes growing around your planting. They can carry the disease.
  - `isu_black_rot_grape.2` (https://yardandgarden.extension.iastate.edu/encyclopedia/black-rot-grape, withheld at review: recommends mancozeb or captan as home sprays (S1): run fetch and chunk, then read build/chunks/isu_black_rot_grape.jsonl)

## Discriminating indicators

Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under 0.85 cosine similarity. Neighbours: none.

The flags in the YAML were set at review and are kept (`--keep-flags`); the proposal is shown beside them.

| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed | In the YAML |
|---|---|---|---|---|---|---|
| `grape_black_rot.yellow_leaf_spots` | 2 | leaf_closeup | — | 0.00 | ✓ | ✓ |
| `grape_black_rot.leaf_spot_size` | 2 | leaf_closeup | — | 0.00 | ✓ | ✓ |
| `grape_black_rot.rusty_red_spot_centres` | 2 | leaf_closeup | — | 0.00 | ✓ | ✓ |
| `grape_black_rot.black_dots_in_spots` | 2 | leaf_closeup | — | 0.00 |  |  |
| `grape_black_rot.rotting_berries` | 2 | flower_fruit | — | 0.00 |  |  |
| `grape_black_rot.black_mummified_berries` | 2 | flower_fruit | — | 0.00 |  |  |
| `grape_black_rot.black_dots_on_mummies` | 2 | flower_fruit | — | 0.00 |  |  |

## Changes after review (2026-10-22)

First review by the knowledge-base owner, before the signing review.

- **Withheld `isu_black_rot_grape.2`** through `withheld_chunks`: it recommends mancozeb or captan as home sprays (S1). The steps still cite it.
- **`preventive_fungicide_grape_black_rot`:** names no product. The page names mancozeb or captan; S1 keeps mancozeb out of steps, and no excerpt names copper for black rot.
- **`leaf_spot_size`:** "about 6 mm (a quarter inch)", the page's "about 1/4 inch".

### After a second read against the excerpts (2026-10-22)

- **Removed `preventive_fungicide_grape_black_rot`:** a chemical step that names no product can't be checked by the plan validator, and its timing is the withheld mancozeb-or-captan programme (`.2`).
- **`lime_sulfur_delayed_dormant`:** "once" left out; `.2` says "a delayed dormant spray". Still open under S6.
- **`leaf_spot_size`:** max 8 mm, so a spot of about a quarter inch isn't rejected at the edge; the question compares it with a pencil eraser.
- **Questions and text:** mummies are "shrivelled and black" (hardness can't be seen); berries "can" rot in a few days, as `.1` says.

### After Alan's review of #143 (2026-10-23)

- **Removed `lime_sulfur_delayed_dormant` (S6):** nobody has checked a lime sulphur product registered for home use on grapes in Canada, or its place on Ontario's Allowable List, and a home step names only a product checked in the registry. The plan keeps the four cultural steps. The step can come back with a registration number written here. `lime sulfur` stays in products.yaml as a harmful name.

## Open questions

- **One source:** the grape slice rests on one ISU page, with one committed excerpt.
