# Review: Hornworm (`hornworm`)

Draft of 2026-10-20 by `claude-opus-5-5`. Check it against the excerpts, then set `reviewed_by` in the YAML. An invasive organism also needs `reporting_url`.

## Model notes

Hornworm is a leaf-eating caterpillar pest, not a predator. No excerpt says it eats or parasitises any of the candidate organisms, so preys_on is empty. The relationship runs the other way: braconid parasitic wasps lay eggs inside hornworms. Parasitised hornworms carry white cocoons, like small grains of rice, on their backs and stop feeding. The gardener should leave them so the beneficial wasps can hatch [clemson_hgic_tomato_insect_pests.10]. The sources conflict on two points for the reviewer. Size: Clemson gives about 7.5–9 cm when fully grown [clemson_hgic_tomato_insect_pests.8]; ISU gives 10–13 cm [isu_tomato_hornworm.1]. Horn colour of the tomato hornworm: Clemson says straight and black [clemson_hgic_tomato_insect_pests.8]; ISU says the horn is red or green depending on species [isu_tomato_hornworm.1]. Eggs are smooth, round and about 1.5 mm across. They are light green at first and turn white before hatching. They are laid singly or in small numbers on leaf undersides [clemson_hgic_tomato_insect_pests.8, clemson_hgic_tomato_insect_pests.9]. The adult is named only as a five-spotted hawk moth, seen hovering at dusk [isu_tomato_hornworm.1, clemson_hgic_tomato_insect_pests.9]. No excerpt describes how the adult looks.

## Recognition (insect, pest)

A large, bright green caterpillar, about as thick as a thumb and roughly 7.5–13 cm long when fully grown. It has a horn at its rear end. The tobacco hornworm has seven diagonal white stripes on each side and a curved red horn. The tomato hornworm has eight V-shaped markings on each side, and the sources describe its horn as black or green. It is well camouflaged on the leaves of tomato, pepper, eggplant and potato, where it strips foliage and may leave large open scars on fruit.

  - `clemson_hgic_tomato_insect_pests.8` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `clemson_hgic_tomato_insect_pests.9` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_tomato_hornworm.1`: The tomato hornworm is one of our best known garden pests. It’s almost as big around as your thumb and can be 4 to 5 inches long. It’s bright green and has a hornlike hook at one end that can be either red or green depending on which of the two species you have on your plant. After feeding, hornworms move to the soil where they pupate and spend the winter. The following summer the pupae transform into five-spotted hawk moths and start the cycle over.
  - `isu_tomato_hornworm.2`: Tomato hornworms feed on the leaves and fruit of tomatoes and other vegetables including eggplant, potatoes and peppers. They can quickly defoliate portions of the plant, reduce its productivity, and heavily scar the fruit. Toward the end of the summer when the caterpillars are fully grown, it seems they can eat about a leaf an hour!
  - `isu_tomato_hornworm.3`: The first control option is to pick the caterpillars off your plant by hand. They won’t hurt you but the challenge is that they are well camouflaged. They are the same color as the foliage so it will take some hunting to find them when they are small. When they are big, they will be easier to find but then that means they will have grown and developed by feeding on your tomato foliage already! Another control option is to use a biological insecticide known as Bacillus thuringiensis or Bt or a synthetic home garden insecticide available in local garden centers. Be sure to follow label directions.

## Preys on

(none)

  - (no excerpt cited)

## Changes after review (2026-10-20)

- Point 26: `recognition` now gives the egg: smooth, round, about 1.6 mm across, light green turning white, laid under the leaves (`clemson_hgic_tomato_insect_pests.8`, 1/16 inch; `clemson_hgic_tomato_insect_pests.9` for the leaf underside). The `tomato_hornworms.eggs_under_leaves` indicator gives the same size.
- Horn colour: the draft said the sources describe the tomato hornworm's horn as "black or green". `clemson_hgic_tomato_insect_pests.8` says it is straight and black; `isu_tomato_hornworm.1` says the horn "can be either red or green depending on which of the two species you have" without saying which. `recognition` now gives Clemson's colours per species and adds that one source says the horn can also be green.
- The model notes above compare the cocoons to grains of rice; no excerpt says that, and it is not in the YAML.
