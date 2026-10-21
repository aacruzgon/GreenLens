# Review: Blossom-end rot (`tomato_blossom_end_rot`)

Draft of 2026-10-20 by `claude-opus-5-5`. Check every line against the excerpts under it, fix or delete what they don't support, then set `reviewed_by` and `reviewed_at` in the YAML. The `discriminating` flags come from discriminate.py.

## Model notes (as first drafted)

Excerpts describe fruit symptoms only; no leaf, stem or whole-plant signs, and no spot sizes, so no measures. No excerpt compares blossom-end rot with any listed problem, so looks_like is empty. Prevalence is not stated in the excerpts; 'common' is a guess for the reviewer to check. Conditions give no temperature or humidity numbers. The resistant varieties are in 'Table 8', which is not included, so no variety names are given. Gypsum and calcium nitrate rates in the excerpts were left out on purpose; these are fertilisers, not pest-control products, so products is empty. The existing soil_test_feed_properly was not reused because it advises keeping potassium up, while these excerpts warn against excess potassium. 'Bottom tip of the fruit, away from the stem' explains 'blossom end' in plain words.

## Indicators

- **tomato_blossom_end_rot.water_soaked_blossom_end_spot** (weight 2, discriminating, fruit, flower_fruit)
  - Text: Water-soaked spots on the blossom end of the fruit.
  - Question: Is there a wet-looking, soaked spot on the bottom tip of the fruit, the end away from the stem?
  - `clemson_hgic_tomato_diseases.39` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **tomato_blossom_end_rot.blossom_end_spot_black** (weight 2, discriminating, fruit, flower_fruit)
  - Text: The spots on the blossom end grow larger and turn black.
  - Question: Is there a large black patch on the bottom tip of the fruit?
  - `clemson_hgic_tomato_diseases.39` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **tomato_blossom_end_rot.secondary_rot** (weight 2, discriminating, fruit, flower_fruit)
  - Text: Rot from other decay organisms usually sets in on the damaged blossom end.
  - Question: Does the dark patch on the bottom of the fruit look rotten or decayed?
  - `clemson_hgic_tomato_diseases.39` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

## Conditions

`{"notes": "Favoured by big swings in soil moisture, rainy or cloudy weather with high humidity, and cool temperatures, especially early in the season. Low soil calcium, root damage from hoeing or digging near the plants, and too much ammonium nitrogen, potassium or magnesium fertiliser also raise the risk."}`

  - `clemson_hgic_tomato_diseases.39` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

## Looks like

(none)

## Treatments

- **avoid_excess_potassium_magnesium** (cultural): Do not overfeed with potassium or magnesium. They compete with calcium for uptake by the plant. Epsom salt is a magnesium source, so do not use it on garden soil unless a recent soil report shows a magnesium shortage.
  - `clemson_hgic_tomato_diseases.40` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **calcium_nitrate_sidedress_no_ammonium** (cultural): When feeding beside or around the plants, avoid fertilisers with ammonium nitrogen, such as ammonium sulfate, ammonium nitrate and most complete (all-purpose) fertilisers. Ammonium competes with calcium. Calcium nitrate is usually the best choice. Follow the fertiliser label.
  - `clemson_hgic_tomato_diseases.41` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **even_moisture_irrigation_mulch** (cultural): Keep the soil evenly moist by watering and mulching. Mulch keeps the soil cooler and more evenly moist, and it holds back weeds, so you hoe less near the plants and damage fewer roots.
  - `clemson_hgic_tomato_diseases.41` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **gypsum_preplant_ber** (cultural): If the soil pH is right but calcium is low, dig gypsum (calcium sulfate) into the soil before planting. Follow the soil report and the product label.
  - `clemson_hgic_tomato_diseases.40` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **gypsum_sidedress_rescue** (cultural): If the soil was not tested and no lime or gypsum went in before planting, putting gypsum beside the plants during the season has helped. Follow the product label.
  - `clemson_hgic_tomato_diseases.41` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **plant_at_recommended_date_ber** (cultural): Plant tomatoes in late spring at the date recommended for your area.
  - `clemson_hgic_tomato_diseases.40` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **remove_ber_fruit** (mechanical): Pick off fruit that shows blossom-end rot.
  - `clemson_hgic_tomato_diseases.41` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **resistant_cultivars_ber** (cultural): Grow tomato varieties with resistance to blossom-end rot.
  - `clemson_hgic_tomato_diseases.41` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

- **soil_test_lime_ber** (cultural): Have the soil tested. Lime it as the soil report advises, to bring the pH to 6.5 and give enough calcium. Lime works best when put down 3 to 6 months before planting and dug into the soil. Follow the soil report for other fertiliser before planting.
  - `clemson_hgic_tomato_diseases.40` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)

## Changes after review (2026-10-20)

- Point 26: checked the new draft against `clemson_hgic_tomato_diseases.39` (symptoms and conditions) and `.40` and `.41` (prevention; withheld because they give rates, so ids only). No amount, rate or monthly interval was copied into any step.
- Point 26: `water_soaked_blossom_end_spot` went from weight 3 to 2. `clemson_hgic_tomato_diseases.39` gives it as the symptom but does not call it characteristic, and `clemson_hgic_tomato_diseases.17` says that a greasy, water-soaked area on the blossom end is also a fruit sign of pith necrosis, late blight and buckeye rot, so it does not set blossom-end rot apart. No `looks_like` pair with late blight was written, because no excerpt compares the two or says how to tell them apart.
- Point 26: gypsum and lime steps (`gypsum_preplant_ber`, `gypsum_sidedress_rescue`, `soil_test_lime_ber`) give no amounts and say to follow the soil report or label (`clemson_hgic_tomato_diseases.40`, `.41`). `soil_test_lime_ber` keeps the target pH of 6.5 and liming 3 to 6 months before planting: a target and a timing, not an amount.
- `calcium_nitrate_sidedress_no_ammonium` dropped "like 10-10-10": a fertiliser grade is a set of nutrient percentages, so it now says "most complete (all-purpose) fertilisers" (`clemson_hgic_tomato_diseases.41`).
- Still open: `prevalence: common` is not stated in any excerpt.

## Discriminating indicators

Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under 0.85 cosine similarity. Neighbours: none.

The flags in the YAML were set at review and are kept (`--keep-flags`); the proposal is shown beside them.

| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed | In the YAML |
|---|---|---|---|---|---|---|
| `tomato_blossom_end_rot.water_soaked_blossom_end_spot` | 2 | flower_fruit | — | 0.00 | ✓ | ✓ |
| `tomato_blossom_end_rot.blossom_end_spot_black` | 2 | flower_fruit | — | 0.00 | ✓ | ✓ |
| `tomato_blossom_end_rot.secondary_rot` | 2 | flower_fruit | — | 0.00 | ✓ | ✓ |
