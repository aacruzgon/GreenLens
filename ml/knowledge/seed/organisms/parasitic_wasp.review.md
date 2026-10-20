# Review: Parasitic wasp (`parasitic_wasp`)

Draft of 2026-10-20 by `claude-opus-5-5`. Check it against the excerpts, then set `reviewed_by` in the YAML. An invasive organism also needs `reporting_url`.

## Model notes

Excerpts give no exact size in mm for adult braconid wasps. The size and colour in `recognition` come from a fact sheet on parasitic wasps in general, not braconids alone. The most useful photo sign is the white cocoons on the backs of hornworms. Parasitised hornworms stop feeding, so leave them on the plant and let the next generation of wasps hatch [clemson_hgic_tomato_insect_pests.10]. Parasitic wasps are harmless and beneficial. They generally do not sting, but can sting if handled or threatened [isu_parasitic_wasps.2]. Whitefly is not listed as prey. The whitefly excerpts name aphelinid wasps (Encarsia, Eretmocerus) as the parasitoids, not braconids, and those excerpts are about a different wasp family. No excerpt says braconids attack spider mites or predatory mites.

## Recognition (insect, beneficial)

Adults are small to medium-sized wasps, usually brown or black, though size and colour vary a lot. The easiest sign is on hornworms: small white projections sticking up from the caterpillar's back. These are the wasps' pupal cases.

  - `clemson_hgic_tomato_insect_pests.10` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)
  - `isu_parasitic_wasps.1`: Parasitic wasps, that is, wasps that live part of their lives as parasites inside other insects comprise one of the largest insect groups. There are several hundred species of these wasps found outdoors in the "real world." This very large group is extremely variable in size and color, but most are small to medium-size and brown or black in color.

## Preys on

hornworm

  - `clemson_hgic_tomato_insect_pests.10` (https://hgic.clemson.edu/factsheet/tomato-insect-pests/, wording not committed: run fetch and chunk, then read build/chunks/clemson_hgic_tomato_insect_pests.jsonl)

## Changes after review (2026-10-20)

- Point 23: checked, no change. `preys_on: [hornworm]` (`clemson_hgic_tomato_insect_pests.10`: braconid wasps lay eggs in hornworms and pupate on their backs), `role: beneficial`, and `recognition` uses only `isu_parasitic_wasps.1` ("most are small to medium-size and brown or black in color") and the white pupal cases on hornworms (`clemson_hgic_tomato_insect_pests.10`). Nothing about wasps indoors or in firewood (`isu_parasitic_wasps.3`) is used.
- Not added: `edis_in286_sweetpotato_whitefly.16` says aphelinid wasps (Encarsia, Eretmocerus) parasitise whiteflies, but they are not Braconidae, so `whitefly` is not in `preys_on`. A separate organism would be needed for them.
