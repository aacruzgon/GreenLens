# Review: Gray leaf spot (`corn_gray_leaf_spot`)

Draft of 2026-10-22 by `claude-opus-5-5`. Check every line against the excerpts under it, fix or delete what they don't support, then set `reviewed_by` and `reviewed_at` in the YAML. The `discriminating` flags come from discriminate.py.

## Model notes (as first drafted)

The 2–7 cm lesion length is mapped to lesion_diameter_mm (20–70 mm) since no length quantity for lesions exists; reviewer should check this fits how the phone measures long, narrow spots. No temperature or humidity figures are given, so conditions only hold words. The excerpts compare grey leaf spot with NCLB only on crop losses, not on appearance, and never mention common rust, so looks_like is empty. The excerpts say chemical control is seldom needed but may be warranted for a very susceptible hybrid with early disease; they name no active ingredient or fungicide group, so no chemical treatment or product is drafted. Prevalence set to occasional: destructive in the U.S. Corn Belt, increasing in Ontario but rarely causing significant losses there. No damage pattern from the list fits.

## Indicators

- **corn_gray_leaf_spot.lesion_length_2_to_7_cm** (weight 2, lower_leaves, leaf_closeup)
  - Text: Lesions are about 2 to 7 cm long.
  - Question: Are the spots short stripes, a few centimetres long?
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

- **corn_gray_leaf_spot.narrow_rectangular_lesions** (weight 3, discriminating, lower_leaves, leaf_closeup)
  - Text: Lesions are narrow and rectangular, with straight edges.
  - Question: Are the spots long, narrow and shaped like rectangles with straight sides?
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

- **corn_gray_leaf_spot.lesions_parallel_to_veins** (weight 3, discriminating, lower_leaves, leaf_closeup)
  - Text: Lesions run parallel with the leaf veins.
  - Question: Do the long spots run in the same direction as the leaf veins?
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

- **corn_gray_leaf_spot.light_tan_lesions** (weight 2, discriminating, lower_leaves, leaf_closeup)
  - Text: Young lesions are light tan.
  - Question: Are the spots a light tan colour?
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

- **corn_gray_leaf_spot.grey_mature_lesions** (weight 2, lower_leaves, leaf_closeup)
  - Text: As lesions mature they turn grey.
  - Question: Are the spots grey?
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

- **corn_gray_leaf_spot.lesions_merge_kill_leaves** (weight 2, whole_plant, leaf_closeup, whole_plant)
  - Text: Mature lesions join together and kill or blight entire leaves.
  - Question: Have the spots run together into large dead patches?
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

- **corn_gray_leaf_spot.lower_leaves_after_tasselling** (weight 2, lower_leaves, whole_plant)
  - Text: Symptoms start on the lower leaves, soon after the plant tassels.
  - Question: Are the spots mostly on the lower leaves?
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

## Conditions

`{"notes": "Warm, wet, humid weather favours the disease and spore production. Worst where corn follows corn in fields with much corn residue."}`

  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

## Looks like

- **corn_northern_leaf_blight** (written in `corn_northern_leaf_blight.yaml`): Grey leaf spot makes narrow, rectangular, light-tan spots with straight sides that run along the leaf veins, about 2 to 7 cm long, turning grey as they age; it shows soon after tasselling. Northern corn leaf blight makes oval (elliptical), grey-green or tan streaks that can be much longer, about 2 to 15 cm.
  - `ontario_field_crop_diseases.10`: Incidence: Northern corn leaf blight (NCLB) has traditionally been one of the most damaging corn leaf diseases. Use of resistant/tolerant hybrids has limited yield losses from this disease in commercial corn. However, in recent years the disease has increased, which suggests a decline in tolerance levels due to the development of new NCLB races that can bypass NCLB resistance genes. Significant losses continue to occur in seed corn production when highly susceptible corn inbreds are planted. Appearance: The disease appears as long, elliptical (2–15 cm (1–6 in.)) greyish-green or tan streaks. Lesions most often begin on the lower leaves. As the disease develops, individual lesions may join, forming large blighted areas (Photo 16–2). In some cases the entire leaves may become blighted or “burned.” Losses due to NCLB are most severe when the leaves above the ear are infected at or slightly after pollination. The disease is often confused with Stewart’s wilt (see the section Bacterial Leaf Blight or Stewart’s Wilt).
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.

## Treatments

- **resistant_hybrids_gls** (cultural): Grow corn hybrids that resist grey leaf spot. This matters most if you do not till the soil.
  - `ontario_field_crop_diseases.19`: Management Strategies: Crop rotation and tillage will reduce inoculum levels in surface residues. In reduced tillage systems, rotation and use of hybrid resistance may be necessary. Chemical control is not usually needed, but if a highly susceptible hybrid is planted and disease development begins early in the season it may be warranted. For more details, refer to OMAFRA Publication 812, Field Crop Protection Guide.

- **rotate_and_till_corn_residue** (cultural): Rotate crops and till old corn residue into the soil. The fungus survives the winter in corn residue, and rain splash and wind carry spores from it to new plants. If you do not till, rotating and growing resistant varieties matter even more.
  - `ontario_field_crop_diseases.18`: Incidence: Grey leaf spot is a destructive and economically important disease in the U.S. Corn Belt and surrounding Great Lakes states. The disease has been increasing in Ontario (particularly in southwestern Ontario) but unlike NCLB, rarely do significant losses occur. As with most foliar diseases warm, wet, humid conditions favour development. Appearance: Soon after tasselling symptoms develop on the lower leaves. The disease has unique, elongated (2–7 cm (1–3 in.) long), narrow, light-tan, rectangular lesions. These lesions run parallel with the leaf veins. As the lesions mature, they become grey and join together, killing or blighting entire leaves. Disease Cycle: Grey leaf spot is most problematic when corn follows corn in fields with a considerable amount of corn residue. The fungus survives as fungal strands (Mycelium) in corn residue. Spores produced on the residue are dispersed by wind and rain splash. Warm, humid weather helps spore and disease development.
  - `ontario_field_crop_diseases.19`: Management Strategies: Crop rotation and tillage will reduce inoculum levels in surface residues. In reduced tillage systems, rotation and use of hybrid resistance may be necessary. Chemical control is not usually needed, but if a highly susceptible hybrid is planted and disease development begins early in the season it may be warranted. For more details, refer to OMAFRA Publication 812, Field Crop Protection Guide.

## Discriminating indicators

Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under 0.85 cosine similarity. Neighbours: corn_northern_leaf_blight.

The flags in the YAML were set at review and are kept (`--keep-flags`); the proposal is shown beside them.

**Flagged:** fewer than 3 indicators differ from the neighbours; the set is filled with the least shared ones. Look for a sign the sources give that tells them apart.

| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed | In the YAML |
|---|---|---|---|---|---|---|
| `corn_gray_leaf_spot.lesion_length_2_to_7_cm` | 2 | leaf_closeup | `corn_northern_leaf_blight.long_lesion_size` | 0.87 |  |  |
| `corn_gray_leaf_spot.narrow_rectangular_lesions` | 3 | leaf_closeup | `corn_northern_leaf_blight.elliptical_grey_tan_streaks` | 0.83 | ✓ | ✓ |
| `corn_gray_leaf_spot.lesions_parallel_to_veins` | 3 | leaf_closeup | `corn_northern_leaf_blight.lesions_merge_blighted_areas` | 0.86 |  | ✓ |
| `corn_gray_leaf_spot.light_tan_lesions` | 2 | leaf_closeup | `corn_northern_leaf_blight.elliptical_grey_tan_streaks` | 0.82 | ✓ | ✓ |
| `corn_gray_leaf_spot.grey_mature_lesions` | 2 | leaf_closeup | `corn_northern_leaf_blight.lesions_merge_blighted_areas` | 0.86 | ✓ |  |
| `corn_gray_leaf_spot.lesions_merge_kill_leaves` | 2 | leaf_closeup, whole_plant | `corn_northern_leaf_blight.lesions_merge_blighted_areas` | 0.89 |  |  |
| `corn_gray_leaf_spot.lower_leaves_after_tasselling` | 2 | whole_plant | `corn_northern_leaf_blight.starts_on_lower_leaves` | 0.88 |  |  |

## Changes after review (2026-10-22)

First review by the knowledge-base owner, before the signing review.

- **Looks like:** the pair with `corn_northern_leaf_blight` is written in that file.
- **Discriminating:** narrow rectangular spots, spots that run parallel with the veins, and light tan when young. The script's "turn grey" overlaps northern leaf blight's grey-green; "parallel with the leaf veins" is the sign the page sets against it (`.18`).

### After a second read against the excerpts (2026-10-22)

- **`lesion_length_2_to_7_cm`:** measure removed (see `corn_northern_leaf_blight`'s file).
- **Questions:** the lower-leaves question no longer asks about tassels too, and the joined-spots question asks one sign.
- **`rotate_and_till_corn_residue`:** cites `.18` and `.19`.
