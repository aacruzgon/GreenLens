# Review: Common rust (`corn_common_rust`)

Draft of 2026-10-22 by `claude-opus-5-5`. Check every line against the excerpts under it, fix or delete what they don't support, then set `reviewed_by` and `reviewed_at` in the YAML. The `discriminating` flags come from discriminate.py.

## Model notes (as first drafted)

Excerpts do not compare common rust with grey leaf spot or northern leaf blight, so looks_like is empty. Excerpts do not say whether symptoms start on lower or upper leaves; 'upper_leaves' was chosen only because a value is required, and the reviewer should check it. No symptom is called characteristic or diagnostic, so no weight 3 was given, though brick-red pustules are the key sign. Prevalence: the excerpts say rust is of minor economic importance in most years but do not say how often it occurs; 'occasional' is a guess. Temperature range 14 to 18 °C is for cool evenings; daytime is only 'moderate'; no humidity figure is given. The excerpt says foliar fungicides may help in field corn if rust appears early, and can be worth it on very susceptible corn, but are not usually needed; no active ingredient is named, so no chemical treatment or product was written. The excerpt also says crop rotation and reduced tillage do not affect common rust in Ontario, because it does not overwinter there; so rotate_and_till_corn_residue was not reused. No damage pattern from the list fits.

## Indicators

- **corn_common_rust.yellow_flecks_early** (weight 2, discriminating, upper_leaves, leaf_closeup, leaf_underside)
  - Text: Early on, small yellow flecks or spots appear on either side of the leaf.
  - Question: Are there small yellow flecks or spots on the leaf?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

- **corn_common_rust.brick_red_pustules** (weight 2, discriminating, upper_leaves, leaf_closeup, leaf_underside)
  - Text: Small brick-red pustules break through the leaf surface and release brick-red spores.
  - Question: Are there small brick-red bumps breaking through the leaf surface?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

- **corn_common_rust.oval_elongated_lesions** (weight 2, discriminating, upper_leaves, leaf_closeup, leaf_underside)
  - Text: The pustules are oval or elongated.
  - Question: Are the bumps oval or long and narrow in shape?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

- **corn_common_rust.lesion_size_2_10mm** (weight 2, upper_leaves, leaf_closeup, leaf_underside)
  - Text: The pustules are small, about 2 to 10 mm long.
  - Question: Are the rust-coloured bumps small, no longer than a grain of rice?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

- **corn_common_rust.yellowing_around_lesions** (weight 2, upper_leaves, leaf_closeup)
  - Text: The leaf turns yellow around the pustules.
  - Question: Is the leaf yellow around the red or dark bumps?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

- **corn_common_rust.pustules_turn_black** (weight 2, upper_leaves, leaf_closeup, leaf_underside)
  - Text: As they mature, the brick-red spores turn black, so the pustules and leaf surface look black.
  - Question: Have the bumps turned black, making the leaf look black in places?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

- **corn_common_rust.brown_dead_areas** (weight 2, upper_leaves, leaf_closeup)
  - Text: Dead, brown areas develop on the leaf.
  - Question: Are there dead, brown patches on the leaf?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

- **corn_common_rust.whole_leaf_dies** (weight 1, whole_plant, whole_plant)
  - Text: In severe cases the whole leaf dies.
  - Question: Are whole leaves on the plant dead and brown?
  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

## Conditions

`{"notes": "Favoured by high humidity, with cool evenings of 14 to 18 °C followed by moderate daytime temperatures. Spores do not survive winter in Ontario; they blow in from the southern U.S. and Mexico, and early spring storm fronts can bring early infection."}`

  - `ontario_field_crop_diseases.20`: Incidence: Common rust, as well as southern corn rust, do not overwinter in Ontario but originate from infected corn in the southern U.S. and Mexico. Rust spores are blown into Ontario from these infected corn plants. In most years, rust is of minor economic importance, however development of early spring storm fronts can distribute spores into the province and cause early season infection. The disease is favoured by high humidity, with cool evening temperatures (14°C–18°C) followed by moderate daytime temperatures. Appearance: Early symptoms of rust infection are yellow flecks or spots on either side of the leaf. These develop into small, brick-red pustules that break through the surface, or epidermis (Photo 16–6). The brick-red colour is the result of spores being released from these oval or elongated (2–10 mm (0.1–0.4 in.) long) lesions. Yellowing of the leaf occurs around these lesions. Dead, brown (necrotic) areas of the leaf develop and, in severe cases, the entire leaf dies. The brick-red spores turn black as they mature, causing the lesions and leaf surface to appear black.

## Looks like

(none)

## Treatments

- **tolerant_corn_hybrids_rust** (cultural): Choose corn hybrids with good tolerance to common rust. Commercial field corn hybrids have good tolerance. Many sweet corn and specialty corn hybrids, and seed corn inbreds, are very susceptible. Rotating crops and tilling don't help against rust: its spores blow in from the south each year.
  - `ontario_field_crop_diseases.21`: Management Strategies: Since common rust does not survive in Ontario, cultural practices such as reduced tillage and crop rotation do not influence disease development. Commercial corn hybrids have good tolerance, whereas many seed corn inbreds, sweet corn and specialty corn hybrids are very susceptible to the disease. Foliar fungicides in field corn may be beneficial if rust symptoms appear early but are not usually needed. They can be economical in highly susceptible corn hybrids, seed corn inbreds or specialty corn hybrids. Refer to OMAFRA Publication 812, Field Crop Protection Guide, for specific product information.

## Discriminating indicators

Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under 0.85 cosine similarity. Neighbours: none.

The flags in the YAML were set at review and are kept (`--keep-flags`); the proposal is shown beside them.

| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed | In the YAML |
|---|---|---|---|---|---|---|
| `corn_common_rust.yellow_flecks_early` | 2 | leaf_closeup, leaf_underside | — | 0.00 | ✓ | ✓ |
| `corn_common_rust.brick_red_pustules` | 2 | leaf_closeup, leaf_underside | — | 0.00 | ✓ | ✓ |
| `corn_common_rust.oval_elongated_lesions` | 2 | leaf_closeup, leaf_underside | — | 0.00 | ✓ | ✓ |
| `corn_common_rust.lesion_size_2_10mm` | 2 | leaf_closeup, leaf_underside | — | 0.00 |  |  |
| `corn_common_rust.yellowing_around_lesions` | 2 | leaf_closeup | — | 0.00 |  |  |
| `corn_common_rust.pustules_turn_black` | 2 | leaf_closeup, leaf_underside | — | 0.00 |  |  |
| `corn_common_rust.brown_dead_areas` | 2 | leaf_closeup | — | 0.00 |  |  |
| `corn_common_rust.whole_leaf_dies` | 1 | whole_plant | — | 0.00 |  |  |

## Changes after review (2026-10-22)

First review by the knowledge-base owner, before the signing review.

- **Conditions:** `temp_c` (14 to 18 °C) removed. The page gives that range for cool evenings followed by moderate days (`ontario_field_crop_diseases.20`), and the weather check compares `temp_c` with every hour, so the range stays in the notes only.

### After a second read against the excerpts (2026-10-22)

- **`lesion_size_2_10mm`:** measure removed: `.20` gives the pustules' length, not their equal-area diameter. The question says "small".
- **`brick_red_pustules`:** "powdery" left out of the question; `.20` doesn't say it.
- **`tolerant_corn_hybrids_rust`:** adds `.21`'s point that rotation and tillage don't affect rust.
