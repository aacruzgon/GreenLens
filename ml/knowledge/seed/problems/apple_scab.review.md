# Review: Apple scab (`apple_scab`)

Draft of 2026-10-22 by `claude-opus-5-5`. Check every line against the excerpts under it, fix or delete what they don't support, then set `reviewed_by` and `reviewed_at` in the YAML. The `discriminating` flags come from discriminate.py.

## Model notes (as first drafted)

Excerpts say fungicides are labelled for apple scab and that preventive spraying, started before symptoms, may help, and that scab is usually not deadly. But they name no active ingredient or fungicide group, so no chemical treatment or product was drafted. A source naming the fungicides is needed. No temperatures or wet-hour figures are given for the conditions. The excerpts list no damage pattern that fits the enum. Cedar-apple rust is not compared with scab by its symptoms in any excerpt; isu_cedar_apple_rust.7 only lists varieties resistant to both. So looks_like is left empty. The 13 mm spot size is the converted half-inch maximum from isu_apple_scab.3. 'Upper leaves' was chosen for the leaf spots because isu_apple_scab.5 says new leaves are infected; the excerpts do not say which leaves show spots.

## Indicators

- **apple_scab.black_olive_leaf_spots** (weight 2, discriminating, upper_leaves, leaf_closeup)
  - Text: Black or olive-green spots on the leaves
  - Question: Are there black or olive-green spots on the leaves?
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.velvety_spots** (weight 2, discriminating, upper_leaves, leaf_closeup)
  - Text: Early spots look velvety
  - Question: Do the dark spots on the leaves look soft and velvety, like felt?
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.irregular_spot_margins** (weight 2, discriminating, upper_leaves, leaf_closeup)
  - Text: Leaf spots have irregular edges
  - Question: Do the dark leaf spots have uneven, ragged edges instead of round, smooth edges?
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.spot_size** (weight 1, upper_leaves, leaf_closeup; measure {'max': 13, 'quantity': 'lesion_diameter_mm', 'stat': 'max'})
  - Text: Spots start small and may grow up to about 13 mm across
  - Question: Are the leaf spots small, no wider than about a fingernail?
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.yellowing_leaves** (weight 2, whole_plant, whole_plant, leaf_closeup)
  - Text: Infected leaves turn yellow
  - Question: Are the spotted leaves turning yellow?
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.early_leaf_drop** (weight 2, whole_plant, whole_plant)
  - Text: Infected leaves fall off early
  - Question: Has the tree lost many leaves early?
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.fruit_spots** (weight 2, fruit, flower_fruit)
  - Text: Distinct brown or black spots on the fruit, often with irregular edges
  - Question: Are there brown or black spots on the fruit?
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.fruit_skin_split** (weight 1, fruit, flower_fruit)
  - Text: In severe cases the fruit skin splits open
  - Question: Is the skin of the fruit cracked or split open?
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

- **apple_scab.misshapen_fruit** (weight 1, fruit, flower_fruit)
  - Text: In severe cases the fruit is misshapen
  - Question: Is the fruit lopsided or oddly shaped?
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.

## Conditions

`{"notes": "Cool, wet weather in spring favours apple scab. How bad it gets each year depends on the weather."}`

  - `isu_apple_scab.5`: Apple scab is caused by the fungus Venturia inaequalis (in apples and crabapples). This fungus survives the winter in infected leaves on the ground. In the spring, the fungus produces sexual spores (ascospores) that can travel by wind to infect newly-developing leaves. Once an infection has begun, the fungus on the new leaves develops asexual spores (conidia) to reinfect the leaf and initiate infection of other nearby leaves. Cool, wet conditions in the spring favor apple scab, so the severity of disease seen in a given year can vary with the weather.
  - `isu_apple_scab.6`: Cultural methods of disease management produce better results in relatively dry years and, in some situations, may even eliminate the need for fungicides. A properly pruned open tree canopy has better ventilation which allows for faster drying of leaves, reducing the number of infections. Because the apple scab fungus overwinters in leaf litter, sanitation is the primary measure to manage scab. Satiation means removing fallen leaves where the fungus will overwinter. In the autumn, removing and destroying fallen foliage (chopped and composted to proper temperatures or discarded as trash) will dramatically reduce the number of this pathogen available to infect trees next year. The best line of defense against this disease would be to plant scab-resistant cultivars and prune and training trees to allow good air circulation. There are fungicides labeled for the control of apple scab. Spraying scab-susceptible trees with preventive treatment (before the symptoms are observed) may help reduce this disease, but scab is usually not a deadly disease.

## Looks like

- **apple_cedar_apple_rust**: Both spot apple leaves in spring. Scab spots are black or olive-green, look velvety when young and have ragged edges. Cedar-apple rust spots are bright yellow-orange, with tiny black dots in the middle, and later small tubes on the underside of the leaf.
  - `isu_apple_scab.3`: The causal fungus causes spots on leaves. In the early stages, spots appear as small black or olive-green, velvety lesions with irregular margins. Later the spots become more distinct and may grow up to a half-inch in diameter. Infected leaves eventually turn yellow and fall prematurely. Infected fruit show distinct brown or black spots with margins that are often irregular. When severe, the skin splits open, and irregularly shaped fruit results.
  - `isu_cedar_apple_rust.3`: Apple trees will present small, yellow spots that appear on the upper leaf surface shortly after bloom. Spots gradually enlarge and become a bright yellow-orange color.
  - `isu_cedar_apple_rust.4`: Galls in junipers will germinate into orange tendrills in the spring. Small black structures (spermagonia) begin to form in the center of the lesions in apple leaves in late spring, early summer.
  - `isu_cedar_apple_rust.5`: During warm rainy days in late April and early May, cedar trees infected with the cedar-apple rust fungus (Gymnosporangium juniperi-virginianae) will develop bright orange gelatinous projection from overwintering brown galls It requires both an apple and cedar or juniper to complete its life cycle. On the cedar, the fungus produces reddish-brown galls that are up to golf-ball size on young twigs. During wet weather, these galls swell and begin to push out bright orange gelatinous tubular structures. The wind carries fungal spores from these gelatinous structures to susceptible apple or crabapple cultivars. Infection occurs when these spores land on a susceptible apple cultivar and moist conditions prevail. Small, yellow spots begin to appear on the upper leaf surface shortly after bloom. Spots gradually enlarge and become a bright yellow-orange color. These brightly colored spots make the disease easy to identify on leaves. Heavily infected leaves may drop prematurely. In late summer small tube-like structures develop on the underside of the apple leaves. Spores are released from these structures and are blown by wind back to susceptible cedars or junipers, completing the disease cycle.

## Treatments

- **prune_open_canopy_apple** (cultural): Prune and train the tree to keep the canopy open. Better airflow lets the leaves dry faster, so fewer infections start.
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.6`: Cultural methods of disease management produce better results in relatively dry years and, in some situations, may even eliminate the need for fungicides. A properly pruned open tree canopy has better ventilation which allows for faster drying of leaves, reducing the number of infections. Because the apple scab fungus overwinters in leaf litter, sanitation is the primary measure to manage scab. Satiation means removing fallen leaves where the fungus will overwinter. In the autumn, removing and destroying fallen foliage (chopped and composted to proper temperatures or discarded as trash) will dramatically reduce the number of this pathogen available to infect trees next year. The best line of defense against this disease would be to plant scab-resistant cultivars and prune and training trees to allow good air circulation. There are fungicides labeled for the control of apple scab. Spraying scab-susceptible trees with preventive treatment (before the symptoms are observed) may help reduce this disease, but scab is usually not a deadly disease.

- **remove_fallen_apple_leaves** (cultural): In autumn, rake up and destroy fallen leaves. The scab fungus survives the winter in them. Either chop them and compost them in a hot compost heap, or throw them out with the rubbish. This is the main way to control scab, because it greatly cuts the spores that infect the tree next spring.
  - `isu_apple_scab.1`: Apple scab is the most common disease of crab apples in Iowa, but can also be found apple trees. The fungus causes spots on leaves that appear black or olive-green, and infected leaves eventually turn yellow and fall off. Infected fruit has brown or black spots, and can result in split skin and misshapen fruit. The fungus survives the winter on infected leaves. Sanitation, proper pruning, and other Integrated Pest Management methods can successfully manage this disease.
  - `isu_apple_scab.5`: Apple scab is caused by the fungus Venturia inaequalis (in apples and crabapples). This fungus survives the winter in infected leaves on the ground. In the spring, the fungus produces sexual spores (ascospores) that can travel by wind to infect newly-developing leaves. Once an infection has begun, the fungus on the new leaves develops asexual spores (conidia) to reinfect the leaf and initiate infection of other nearby leaves. Cool, wet conditions in the spring favor apple scab, so the severity of disease seen in a given year can vary with the weather.
  - `isu_apple_scab.6`: Cultural methods of disease management produce better results in relatively dry years and, in some situations, may even eliminate the need for fungicides. A properly pruned open tree canopy has better ventilation which allows for faster drying of leaves, reducing the number of infections. Because the apple scab fungus overwinters in leaf litter, sanitation is the primary measure to manage scab. Satiation means removing fallen leaves where the fungus will overwinter. In the autumn, removing and destroying fallen foliage (chopped and composted to proper temperatures or discarded as trash) will dramatically reduce the number of this pathogen available to infect trees next year. The best line of defense against this disease would be to plant scab-resistant cultivars and prune and training trees to allow good air circulation. There are fungicides labeled for the control of apple scab. Spraying scab-susceptible trees with preventive treatment (before the symptoms are observed) may help reduce this disease, but scab is usually not a deadly disease.

- **scab_resistant_apple_cultivars** (cultural): Plant scab-resistant apple or crabapple varieties. This is the best defence. Redfree, Liberty, William's Pride and Freedom are apple varieties that are immune to apple scab and to cedar-apple rust.
  - `isu_apple_scab.6`: Cultural methods of disease management produce better results in relatively dry years and, in some situations, may even eliminate the need for fungicides. A properly pruned open tree canopy has better ventilation which allows for faster drying of leaves, reducing the number of infections. Because the apple scab fungus overwinters in leaf litter, sanitation is the primary measure to manage scab. Satiation means removing fallen leaves where the fungus will overwinter. In the autumn, removing and destroying fallen foliage (chopped and composted to proper temperatures or discarded as trash) will dramatically reduce the number of this pathogen available to infect trees next year. The best line of defense against this disease would be to plant scab-resistant cultivars and prune and training trees to allow good air circulation. There are fungicides labeled for the control of apple scab. Spraying scab-susceptible trees with preventive treatment (before the symptoms are observed) may help reduce this disease, but scab is usually not a deadly disease.
  - `isu_cedar_apple_rust.7`: If you have followed along with this cycle, you might immediately think that a good way to break it would be to avoid planting susceptible hosts next to each other. Unfortunately, this is often impractical, because the fungal spores can travel as far as two miles. An easy way to avoid this disease is to plant disease-resistant apple or crabapple varieties. Fortunately, there are many varieties available now that show good disease resistance. Redfree, Liberty, William's Pride and Freedom are examples of new apple varieties that are immune to cedar-apple rust. These varieties are also immune to apple scab and show good resistance to powdery mildew and fire blight. Examples of apples that are susceptible to cedar-apple rust include Jonathan, Rome, Wealthy, York Imperial, and Golden Delicious. Many mail-order nurseries offer disease-resistant apples Rust-resistant crabapples are also readily available. A sampler of disease-resistant varieties includes Adams, David, Donald Wyman, Indian Summer, Prairie Fire, Professor Sprenger and White Angel. These varieties vary in flower color, fruit color, height, and width, so consider these traits before making a selection. Avoid crabapple varieties that have serious disease problems, such as Hopa, Radiant, and Vanguard.

## Discriminating indicators

Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under 0.85 cosine similarity. Neighbours: apple_cedar_apple_rust.

The flags in the YAML were set at review and are kept (`--keep-flags`); the proposal is shown beside them.

| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed | In the YAML |
|---|---|---|---|---|---|---|
| `apple_scab.black_olive_leaf_spots` | 2 | leaf_closeup | `apple_cedar_apple_rust.black_dots_in_spot_centres` | 0.88 |  | ✓ |
| `apple_scab.velvety_spots` | 2 | leaf_closeup | `apple_cedar_apple_rust.black_dots_in_spot_centres` | 0.83 | ✓ | ✓ |
| `apple_scab.irregular_spot_margins` | 2 | leaf_closeup | `apple_cedar_apple_rust.black_dots_in_spot_centres` | 0.87 |  | ✓ |
| `apple_scab.spot_size` | 1 | leaf_closeup | `apple_cedar_apple_rust.bright_yellow_orange_spots` | 0.85 |  |  |
| `apple_scab.yellowing_leaves` | 2 | whole_plant, leaf_closeup | `apple_cedar_apple_rust.early_leaf_drop` | 0.91 |  |  |
| `apple_scab.early_leaf_drop` | 2 | whole_plant | `apple_cedar_apple_rust.early_leaf_drop` | 0.95 |  |  |
| `apple_scab.fruit_spots` | 2 | flower_fruit | `apple_cedar_apple_rust.black_dots_in_spot_centres` | 0.88 |  |  |
| `apple_scab.fruit_skin_split` | 1 | flower_fruit | `apple_cedar_apple_rust.small_yellow_spots_upper_surface` | 0.79 | ✓ |  |
| `apple_scab.misshapen_fruit` | 1 | flower_fruit | `apple_cedar_apple_rust.small_yellow_spots_upper_surface` | 0.82 | ✓ |  |

## Changes after review (2026-10-22)

First review by the knowledge-base owner, before the signing review.

- **Looks like:** added `apple_cedar_apple_rust`, written once here (the loader writes both directions). The two ISU pages don't compare them, but both spot apple leaves in spring and the camera sees one leaf; the `how_to_tell` uses only each page's own signs (`isu_apple_scab.3`, `isu_cedar_apple_rust.3`, `.4`, `.5`).
- **Discriminating:** kept black or olive-green spots, velvety spots and ragged edges, all on a leaf close-up. The script's set (velvety, dusty residue, split fruit skin) needs a magnifier for one and a fruit shot for another.

### After a second read against the excerpts (2026-10-22)

- **Deleted `olive_dusty_residue`:** `isu_apple_scab.4` says a magnifier is needed to see it, so no photo can answer it.
- **Weights:** black or olive-green spots and velvety spots 3 → 2: `.1` and `.3` don't call either characteristic.
- **Questions:** `early_leaf_drop`, `fruit_spots` and `yellowing_leaves` ask one sign each, without words the page doesn't use ("thin branches", "many").
- **Looks like:** cites `isu_apple_scab.3` and `isu_cedar_apple_rust.3`, `.4`, `.5`.
