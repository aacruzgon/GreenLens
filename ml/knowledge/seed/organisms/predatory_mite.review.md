# Review: Predatory mite (`predatory_mite`)

Draft of 2026-10-20 by `claude-opus-5-5`. Check it against the excerpts, then set `reviewed_by` in the YAML. An invasive organism also needs `reporting_url`.

## Model notes (as first drafted)

The excerpts give no body size for this mite, and do not say where on the plant it usually sits. The look described (long legs, fast movement, red or orange colour) is given for commercial predatory mites as a group, not for P. persimilis alone. The excerpts say P. persimilis is the most common commercial predator of spider mites and attacks all life stages, eating up to about 20 eggs or 5 adults a day. The whitefly excerpt names a different predatory mite (Amblyseius swirskii), not P. persimilis, so whitefly is not listed. Nothing links it to hornworms or parasitic wasps. A 10x to 15x hand lens is needed to see spider mites and their predators.

## Recognition (mite, beneficial)

A tiny mite with longer legs than a spider mite. The front pair of legs often points forward. It moves about quickly and is more active than the spider mites it hunts. It is often red or orange, but colour alone does not tell it from a spider mite, which can be orange-red too.

  - `edis_in307_twospotted_spider_mite.4`: The twospotted spider mite is oval in shape, about 1/50 inch long and may be brown or orange-red, but a green, greenish-yellow or an almost translucent color is most common. The female is about 0.4 mm in length with an elliptical body that bears 12 pairs of dorsal setae. Overwintering females are orange to orange-red. The body contents (large dark spots) are often visible through the transparent body wall. Since the spots are accumulation of body wastes, newly molted mites may lack the spots. The male is elliptical with the caudal end tapering and smaller than the female. The axis of knob of aedeagus is parallel or forming a small angle with axis of shaft.
  - `edis_in307_twospotted_spider_mite.9`: Spider mites are commonly attacked by predator mites. Six species are commercially available in the US: Phytoseiulus persimilis, Mesoseiulus longipes, Neoseiulus californicus, Neoseiulus cucumeris, Galendromus occidentalis, and Amblyseius fallicus. Predatory mites can be distinguished from spider mites because of their longer legs. The front pair of legs is often extended forward. They are more active and move about at a fast pace. They are often red or orange in color. P. persimilis is the most common predator and preys on all stages of mites (Osborne 1999). It can consume 20 eggs or five adults daily. Overwintering mites may be reduced in numbers by the destruction of weeds such as pokeweed, Jerusalem oak, Jimson weed, wild blackberry, wild geranium, and others.

## Preys on

two_spotted_spider_mite, whitefly

  - `edis_in286_sweetpotato_whitefly.17`: Bemisia populations can also be reduced by predation. Many species of insects, mites, and spiders feed on immature and adult whiteflies. Some well-known generalist predators that prey on whiteflies include Chrysoperla species larvae (lacewings), Orius species (minute pirate bugs), and Geocoris species (bigeyed bugs). Several coccinellid species are specialist whitefly predators, such as Delphastus catalinae (LeConte) and Nephaspis oculatus (Blatchley). Under certain climatic conditions, usually wet and warm, Bemisia populations can be drastically reduced by natural epizootics of entomopathogens. Beauveria bassiana and Verticillium lecanii, two fungal pathogens with efficacy against Bemisia and other insects, have been commercialized and are available for field and greenhouse application against Bemisia. In protected plant culture, such as greenhouses, it is not possible to rely on natural population regulation. Several effective species of parasitoids, such as Eretmocerus eremicus, and predators, such as the predatory mite Amblyseius swirskii and the coccinellid D. catalinae, are available commercially for control of Bemisia.
  - `edis_in307_twospotted_spider_mite.9`: Spider mites are commonly attacked by predator mites. Six species are commercially available in the US: Phytoseiulus persimilis, Mesoseiulus longipes, Neoseiulus californicus, Neoseiulus cucumeris, Galendromus occidentalis, and Amblyseius fallicus. Predatory mites can be distinguished from spider mites because of their longer legs. The front pair of legs is often extended forward. They are more active and move about at a fast pace. They are often red or orange in color. P. persimilis is the most common predator and preys on all stages of mites (Osborne 1999). It can consume 20 eggs or five adults daily. Overwintering mites may be reduced in numbers by the destruction of weeds such as pokeweed, Jerusalem oak, Jimson weed, wild blackberry, wild geranium, and others.

## Changes after review (2026-10-20)

- Point 23: checked, no change. `preys_on: [two_spotted_spider_mite]` and `recognition` (longer legs, front legs held forward, fast, often red or orange) come from `edis_in307_twospotted_spider_mite.9`: "P. persimilis is the most common predator and preys on all stages of mites"; `role: beneficial`.

## Changes after signing review (2026-10-20)

- **S7:** the recognition leads with the longer legs, the forward front pair and the fast movement, and says colour alone does not tell it from a spider mite, which can be orange-red too (`edis_in307_twospotted_spider_mite.4`, `.9`).
- **U8:** `scientific_name` and `bioclip_label` are `Phytoseiidae`, since the description is for predatory mites as a group (.9); `preys_on` adds `whitefly`: "the predatory mite *Amblyseius swirskii*… available commercially for control of *Bemisia*" (`edis_in286_sweetpotato_whitefly.17`, now a source).
