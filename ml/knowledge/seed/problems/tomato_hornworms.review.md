# Review: Hornworms (`tomato_hornworms`)

Draft of 2026-10-20 by `claude-opus-5-5`. Check every line against the excerpts under it, fix or delete what they don't support, then set `reviewed_by` and `reviewed_at` in the YAML. The `discriminating` flags come from discriminate.py.

## Model notes

Damage pattern 'other' = leaves stripped from the plant (defoliation) and large open scars eaten into the fruit surface; the excerpts do not mention frass or chewed margins. No weather (temperature, humidity) is given, only timing in the season. No excerpt compares hornworms with any listed problem, so looks_like is empty. Caterpillar length differs between sources (Clemson about 7.5–9 cm, ISU 10–13 cm) and egg size is about 1.6 mm; none matches a phone-measured quantity, so no measures were set. Horn colour differs: Clemson says red and curved (tobacco hornworm) or black and straight (tomato hornworm); ISU says red or green. ISU also mentions an unnamed 'synthetic home garden insecticide', not drafted as a separate treatment. Spinosad set broad_spectrum false because no excerpt says it kills beneficials.

## Indicators

- **tomato_hornworms.leaves_stripped** (weight 2, whole_plant, whole_plant)
  - Text: Leaves are eaten away, and parts of the plant are quickly stripped bare.
  - Question: Are parts of the plant stripped bare, with many leaves eaten away?
  - `clemson_hgic_tomato_insect_pests.9` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_tomato_hornworm.2`: Tomato hornworms feed on the leaves and fruit of tomatoes and other vegetables including eggplant, potatoes and peppers. They can quickly defoliate portions of the plant, reduce its productivity, and heavily scar the fruit. Toward the end of the summer when the caterpillars are fully grown, it seems they can eat about a leaf an hour!

- **tomato_hornworms.large_green_caterpillar** (weight 2, whole_plant, whole_plant, leaf_closeup)
  - Text: A very large, fat, bright green caterpillar, about as thick as a thumb, is on the plant. It is the same colour as the leaves, so it is hard to see.
  - Question: Is there a large, fat, green caterpillar on the plant?
  - `clemson_hgic_tomato_insect_pests.8` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_tomato_hornworm.1`: The tomato hornworm is one of our best known garden pests. It’s almost as big around as your thumb and can be 4 to 5 inches long. It’s bright green and has a hornlike hook at one end that can be either red or green depending on which of the two species you have on your plant. After feeding, hornworms move to the soil where they pupate and spend the winter. The following summer the pupae transform into five-spotted hawk moths and start the cycle over.
  - `isu_tomato_hornworm.3`: The first control option is to pick the caterpillars off your plant by hand. They won’t hurt you but the challenge is that they are well camouflaged. They are the same color as the foliage so it will take some hunting to find them when they are small. When they are big, they will be easier to find but then that means they will have grown and developed by feeding on your tomato foliage already! Another control option is to use a biological insecticide known as Bacillus thuringiensis or Bt or a synthetic home garden insecticide available in local garden centers. Be sure to follow label directions.

- **tomato_hornworms.horn_on_rear_end** (weight 3, whole_plant, whole_plant, leaf_closeup)
  - Text: The caterpillar has a horn sticking out from its back end. The horn may be red and curved, or black and straight, or green.
  - Question: Does the caterpillar have a horn sticking out from its back end?
  - `clemson_hgic_tomato_insect_pests.8` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_tomato_hornworm.1`: The tomato hornworm is one of our best known garden pests. It’s almost as big around as your thumb and can be 4 to 5 inches long. It’s bright green and has a hornlike hook at one end that can be either red or green depending on which of the two species you have on your plant. After feeding, hornworms move to the soil where they pupate and spend the winter. The following summer the pupae transform into five-spotted hawk moths and start the cycle over.

- **tomato_hornworms.side_stripes_or_v_marks** (weight 2, whole_plant, whole_plant, leaf_closeup)
  - Text: The caterpillar has white slanted stripes or V-shaped marks along each side of its body.
  - Question: Does the green caterpillar have white slanted stripes or V-shaped marks along its sides?
  - `clemson_hgic_tomato_insect_pests.8` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **tomato_hornworms.eggs_under_leaves** (weight 2, leaf_underside, leaf_underside)
  - Text: Smooth, round eggs, about 1.6 mm across, sit on the underside of leaves. They are light green at first and turn white before hatching.
  - Question: Are there tiny, smooth, round, pale green or white eggs on the underside of the leaf?
  - `clemson_hgic_tomato_insect_pests.8` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `clemson_hgic_tomato_insect_pests.9` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **tomato_hornworms.open_scars_on_fruit** (weight 1, fruit, flower_fruit)
  - Text: In heavy infestations, fruit has large, open scars where the caterpillars ate the surface. They do not bore inside. This is much less common than leaf loss.
  - Question: Does the fruit have large, open, eaten scars on its surface?
  - `clemson_hgic_tomato_insect_pests.9` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_tomato_hornworm.2`: Tomato hornworms feed on the leaves and fruit of tomatoes and other vegetables including eggplant, potatoes and peppers. They can quickly defoliate portions of the plant, reduce its productivity, and heavily scar the fruit. Toward the end of the summer when the caterpillars are fully grown, it seems they can eat about a leaf an hour!

- **tomato_hornworms.white_cocoons_on_back** (weight 2, whole_plant, whole_plant, leaf_closeup)
  - Text: Some caterpillars carry many small white projections on their backs. These are the cocoons of tiny parasitic wasps.
  - Question: Does the caterpillar have many small white bumps sticking up from its back?
  - `clemson_hgic_tomato_insect_pests.10` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

## Conditions

`{"notes": "No weather is given. Damage usually starts in midsummer and goes on until the end of the season. Heavy egg laying is common in August and early September (South Carolina). Eggs hatch in about four days, depending on temperature."}`

  - `clemson_hgic_tomato_insect_pests.10` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `clemson_hgic_tomato_insect_pests.9` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

## Looks like

(none)

## Treatments

- **bt_hornworms** (biological, new): Spray a biological insecticide containing Bacillus thuringiensis (Bt), labelled for tomatoes. Follow the label.
  - `isu_tomato_hornworm.3`: The first control option is to pick the caterpillars off your plant by hand. They won’t hurt you but the challenge is that they are well camouflaged. They are the same color as the foliage so it will take some hunting to find them when they are small. When they are big, they will be easier to find but then that means they will have grown and developed by feeding on your tomato foliage already! Another control option is to use a biological insecticide known as Bacillus thuringiensis or Bt or a synthetic home garden insecticide available in local garden centers. Be sure to follow label directions.

- **contact_insecticide_hornworms** (chemical, new): If you need a spray, contact insecticides such as bifenthrin, cyhalothrin, cyfluthrin or permethrin control hornworms. Insecticides can kill the helpful insects that also control hornworms, so use them only when needed. Do not use permethrin on tomato varieties whose fruit is smaller than about 2.5 cm across. Use a product labelled for tomatoes and follow its label.
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **handpick_hornworms** (mechanical, new): Pick the caterpillars off by hand and destroy them. They do not hurt you. They are green like the leaves, so search carefully, especially when they are small.
  - `clemson_hgic_tomato_insect_pests.10` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_tomato_hornworm.3`: The first control option is to pick the caterpillars off your plant by hand. They won’t hurt you but the challenge is that they are well camouflaged. They are the same color as the foliage so it will take some hunting to find them when they are small. When they are big, they will be easier to find but then that means they will have grown and developed by feeding on your tomato foliage already! Another control option is to use a biological insecticide known as Bacillus thuringiensis or Bt or a synthetic home garden insecticide available in local garden centers. Be sure to follow label directions.

- **keep_plants_vigorous** (cultural, merged): Keep plants growing strongly. Stressed plants attract more insect pests, suffer more from spider mite feeding and catch diseases more easily. Plant in well-prepared, fertile beds that drain well, and after heavy rain get standing water away quickly. Mulch the plants, and water and feed them well; follow the fertiliser label. For early blight, test the soil every year, keep potassium at a good level, lime as the soil test advises and side-dress with calcium nitrate; this makes the disease less severe.
  - `clemson_hgic_tomato_diseases.6` (https://hgic.clemson.edu/factsheet/tomato-diseases-disorders/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_diseases.jsonl)
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_two_spotted_spider_mite.4`: Several miticides (pesticides that control mites) are available for mite control. Spraying to reduce mite numbers is advised when plants show the characteristic speckling and the white sheet of paper technique turns up a large number of mites (a dozen or more on each sheet). Sprays available to homeowners include insecticidal soap, horticulture oil (summer rate), malathion, permethrin, and others available at your garden supply store. Soap and oil sprays have no residual activity and only control mites and insects that are contacted directly. Thorough spraying is important for control. On smaller trees and shrubs it may be possible to reduce light mite populations with periodic “hosings.” Use the garden hose to apply a forceful stream of water to dislodge mites from an infested plant. Repeat 3 or 4 times on consecutive days. Keep plants watered and mulched to promote health and vigor and to reduce the impact of mite feeding.
  - `ontario_tomato_bacterial_diseases.23`: Experience has shown that if a bacterial disease outbreak can be delayed until after the main fruit set, the crop will be minimally affected. Once the plant has reached a full canopy, a low level of bacterial disease on the foliage can be tolerated. Fruit lesions, which have a major impact on marketable yield, can only be initiated on young green fruit, so control measures used prior to fruiting are most beneficial. Plants under stress are more susceptible to disease than plants growing under ideal conditions. Some environmental stresses are out of the grower's control, but management practices can reduce many stress factors, including those associated with dry weather, excess water, compacted or poorly structured soils, weeds, insects, other diseases, and field operations. Bacterial disease often seems to affect early cultivars most severely. The cool temperatures and stressful conditions frequently experienced by early plantings are likely a contributing factor. The specific management recommendations below are based on an understanding of the biology of the pathogens and the response of the crop, as described above.
  - `ontario_tomato_bacterial_diseases.31`: Since all three bacterial pathogens can survive in crop debris, rotate tomatoes with non-host crops. If tomato crop debris is well worked into the top 15 cm (6 in.) of soil to speed decomposition, a 3-year rotation should be sufficient. Control weeds and volunteer tomatoes in and around the field, as they can act as reservoirs of disease. Ensure good drainage and adequate fertility. After heavy rains, get excess water off the field as soon as possible, and address problem areas with drop pipes or other measures. Separate plant lots into different fields if possible. Consider using tall, barrier crops between plant lots and neighbouring fields, but take care to maintain good air circulation within the field. Try to avoid wet foliage during transplanting. It is difficult to dip the plug trays to wet the plugs without wetting the foliage, but it is beneficial if it can be achieved.
  - `ontario_tomato_bacterial_diseases.32`: Ideally growers would be able to keep workers out of the field when the foliage is wet. This, however, is not always practical. Keep in mind that earlier infections have more time to cause damage and that fruit lesions are initiated on young, green fruit. Late-season foliar symptoms are not a major concern. Low-pressure systems are better if overhead irrigation is used, as they minimize splashing and plant damage. The potential for overhead watering to spread disease must be balanced with the potential benefits from irrigation if an overhead system is all that is available. Plants under stress are less able to withstand a disease outbreak.

- **leave_parasitized_hornworms** (biological, new): Leave caterpillars that carry white cocoons on their backs. They have stopped feeding, and the next generation of helpful wasps will hatch from them.
  - `clemson_hgic_tomato_insect_pests.10` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **protect_natural_enemies** (biological, merged): Helpful insects and mites keep many tomato pests down, so protect them and use insecticides only when needed. Helpful insects, among them small wasps, control hornworms. Spider mites are eaten by predatory mites, small lady beetles, minute pirate bugs, predatory thrips and lacewing larvae; predatory mites have longer legs, move fast and are often red or orange, so do not mistake them for pests. Whiteflies are killed by tiny parasitic wasps, lacewing larvae, minute pirate bugs, bigeyed bugs and small lady beetles; where no insecticides are used, these usually keep whiteflies low. An empty whitefly shell with a round hole means a helpful wasp hatched from it.
  - `clemson_hgic_tomato_insect_pests.10` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `edis_in286_sweetpotato_whitefly.16`: In Florida, Bemisia rarely reaches outbreak population levels in natural environments or in agricultural situations where no insecticides are applied. In these situations, natural biological control keeps Bemisia below economically injurious levels. Much mortality is caused by minute parasitic wasps (parasitoids) in the aphelinid family. Female parasitoids lay their eggs inside the whitefly nymph or between the whitefly and the leaf surface, depending on the genus of wasp. The immature parasitoids develop within the whitefly host, eventually consuming the entire host, except the integument. The immature parasitoid pupates within the integument of the host and the adult parasitoid emerges through a round hole. The most common parasitoids attacking Bemisia in Florida are in two genera, Encarsia and Eretmocerus. Encarsia pergandiella (Howard) and Encarsia sophia (Girault & Dodd) (formerly E. transvena (Timberlake)) are common throughout the state, while Encarsia nigricephala Dozier is common in north-central Florida. Several species of Eretmocerus are also common throughout the state. Eretmocerus species cause mortality to whiteflies by host-feeding in addition to parasitism. Females stab the immature whitefly repeatedly with their ovipositor and then turn around and feed from the wound, obtaining valuable protein with which to provision their eggs.
  - `edis_in286_sweetpotato_whitefly.17`: Bemisia populations can also be reduced by predation. Many species of insects, mites, and spiders feed on immature and adult whiteflies. Some well-known generalist predators that prey on whiteflies include Chrysoperla species larvae (lacewings), Orius species (minute pirate bugs), and Geocoris species (bigeyed bugs). Several coccinellid species are specialist whitefly predators, such as Delphastus catalinae (LeConte) and Nephaspis oculatus (Blatchley). Under certain climatic conditions, usually wet and warm, Bemisia populations can be drastically reduced by natural epizootics of entomopathogens. Beauveria bassiana and Verticillium lecanii, two fungal pathogens with efficacy against Bemisia and other insects, have been commercialized and are available for field and greenhouse application against Bemisia. In protected plant culture, such as greenhouses, it is not possible to rely on natural population regulation. Several effective species of parasitoids, such as Eretmocerus eremicus, and predators, such as the predatory mite Amblyseius swirskii and the coccinellid D. catalinae, are available commercially for control of Bemisia.
  - `edis_in307_twospotted_spider_mite.8`: For detection of spider mites, a 10X to 15X magnifying glass is a necessity. Examine the undersides of the leaves closely for mites, cast skins, and webbing. A more efficient technique is to place a sheet of white typing paper beneath the leaves and strike the foliage sharply. The mites will fall onto the paper and can be more easily observed and identified than on the green foliage. Predators are very important in regulating spider mite populations and should be protected whenever possible. Important genera include the predatory mites, Amblyseius, Neoseiulus, Metaseiulus, and Phytoseiulus; the lady beetles, Stethorus; the minute pirate bugs, Orius; the thrips, Leptothrips; and the lacewing larvae, Chrysopa. In greenhouses, the ghost ant, Tapinoma melanocephalum (Fabricius), a pest in itself, was also reported as a significant predator (Osborne et al. 1995).
  - `edis_in307_twospotted_spider_mite.9`: Spider mites are commonly attacked by predator mites. Six species are commercially available in the US: Phytoseiulus persimilis, Mesoseiulus longipes, Neoseiulus californicus, Neoseiulus cucumeris, Galendromus occidentalis, and Amblyseius fallicus. Predatory mites can be distinguished from spider mites because of their longer legs. The front pair of legs is often extended forward. They are more active and move about at a fast pace. They are often red or orange in color. P. persimilis is the most common predator and preys on all stages of mites (Osborne 1999). It can consume 20 eggs or five adults daily. Overwintering mites may be reduced in numbers by the destruction of weeds such as pokeweed, Jerusalem oak, Jimson weed, wild blackberry, wild geranium, and others.

- **pyrethrin_hornworms** (chemical, new): Pyrethrin is a natural product that controls caterpillars. Insecticides can kill helpful insects too, so use it only when needed. Use a product labelled for tomatoes and follow its label.
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **spinosad_hornworms** (chemical, new): Spinosad is a natural product that controls caterpillars. Insecticides can kill helpful insects too, so use it only when needed. Use a product labelled for tomatoes and follow its label.
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

## Products

- **bacillus thuringiensis**, broad-spectrum: False
  - `isu_tomato_hornworm.3`: The first control option is to pick the caterpillars off your plant by hand. They won’t hurt you but the challenge is that they are well camouflaged. They are the same color as the foliage so it will take some hunting to find them when they are small. When they are big, they will be easier to find but then that means they will have grown and developed by feeding on your tomato foliage already! Another control option is to use a biological insecticide known as Bacillus thuringiensis or Bt or a synthetic home garden insecticide available in local garden centers. Be sure to follow label directions.

- **spinosad**, broad-spectrum: False
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **pyrethrin**, broad-spectrum: True
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **bifenthrin**, broad-spectrum: True
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **cyhalothrin**, broad-spectrum: True
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **cyfluthrin**, broad-spectrum: True
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

- **permethrin**, broad-spectrum: True
  - `clemson_hgic_tomato_insect_pests.15` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

## Changes after review (2026-10-20)

- Point 15: `pyrethroid_hornworms` is renamed `contact_insecticide_hornworms`. No excerpt uses the word pyrethroid; `clemson_hgic_tomato_insect_pests.15` calls bifenthrin, cyhalothrin, cyfluthrin and permethrin contact insecticides. The step names all four, so the plan validator flags it, and keeps the warning not to use permethrin on varieties with fruit under about 2.5 cm (one inch). "As a last step" became "if you need a spray", and the reason is the excerpt's general warning to use insecticides only when necessary so as not to kill beneficial insects. bifenthrin, cyhalothrin, cyfluthrin and permethrin are in products.yaml with `broad_spectrum: true`; the `pyrethroid` entry is gone because no treatment names it.
- Point 15: `pyrethrin_hornworms` said pyrethrin "can also kill helpful insects"; no excerpt says that of pyrethrin itself, so it now gives the general warning from `clemson_hgic_tomato_insect_pests.15`. Pyrethrin stays `broad_spectrum: true` as a class decision at review.
- Point 15: `spinosad_hornworms` gets the same general warning. Spinosad stays `broad_spectrum: false`: `clemson_hgic_tomato_insect_pests.15` only calls it a natural product for caterpillars and thrips, and no excerpt says it harms beneficials. Pyrethrin is described the same way in that excerpt, so the reviewer should confirm the split.
- Point 15: esfenvalerate, named in the review, is in none of the excerpts, so it has no product record.
- Point 26: `eggs_under_leaves` now gives the egg size, about 1.6 mm across (1/16 inch, `clemson_hgic_tomato_insect_pests.8`), the same as the `hornworm` organism.
- `vigorous_plants_fewer_pests`: fixed the run-on "ones.Remember" (`clemson_hgic_tomato_insect_pests.15`).
- **Point 25 (merge pass), `keep_plants_vigorous`:** replaces `vigorous_plants_fewer_pests`. One vigour step, also linked from speck, spot, early blight, spider mites: fertile, well-drained beds, standing water off after heavy rain, mulch, water and feed; stressed plants attract more pests, suffer more from mites and catch diseases more easily; for early blight, the yearly soil test, potassium, lime and calcium nitrate side-dressing. Chunks `clemson_hgic_tomato_diseases.6`; `clemson_hgic_tomato_insect_pests.15`; `isu_two_spotted_spider_mite.4`; `ontario_tomato_bacterial_diseases.23`, `.31`, `.32`. Not merged: blossom-end rot's `even_moisture_irrigation_mulch` and its calcium steps.
- **Point 25 (merge pass), `protect_natural_enemies`:** replaces `protect_beneficials_spray_only_when_needed`. One step for protecting helpful insects and mites, also linked from spider mites, whiteflies; it keeps each pest's enemies and the advice to use insecticides only when needed. Chunks `clemson_hgic_tomato_insect_pests.10`, `.15`; `edis_in307_twospotted_spider_mite.8`, `.9`; `edis_in286_sweetpotato_whitefly.16`, `.17`. Not merged: `leave_parasitized_hornworms`, `buy_predatory_mites` and `buy_whitefly_biocontrols_greenhouse`, which are separate actions.
- **Point 25 (merge pass), review file:** the Treatments section above now lists exactly the steps in `tomato_hornworms.yaml`, with each step's current text; merged steps quote the excerpts behind the merged text.

## Discriminating indicators

Proposed by discriminate.py: the top 3 by weight whose closest neighbour indicator is under 0.85 cosine similarity. Neighbours: none.

| Indicator | Weight | Shots | Closest neighbour indicator | Similarity | Proposed |
|---|---|---|---|---|---|
| `tomato_hornworms.leaves_stripped` | 2 | whole_plant | — | 0.00 | ✓ |
| `tomato_hornworms.large_green_caterpillar` | 2 | whole_plant, leaf_closeup | — | 0.00 | ✓ |
| `tomato_hornworms.horn_on_rear_end` | 3 | whole_plant, leaf_closeup | — | 0.00 | ✓ |
| `tomato_hornworms.side_stripes_or_v_marks` | 2 | whole_plant, leaf_closeup | — | 0.00 |  |
| `tomato_hornworms.eggs_under_leaves` | 2 | leaf_underside | — | 0.00 |  |
| `tomato_hornworms.open_scars_on_fruit` | 1 | flower_fruit | — | 0.00 |  |
| `tomato_hornworms.white_cocoons_on_back` | 2 | whole_plant, leaf_closeup | — | 0.00 |  |
