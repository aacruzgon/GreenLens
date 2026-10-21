# Leakage check: leakage-2026-10-20-garden

253 evaluation photos (`ml/training/eval/manifest.jsonl`) against 59,821 training images under `ml/training/data/raw`, 0 of them unreadable.
A match is a perceptual-hash distance of 5 bits or less over the photo's 8 rotations and flips, or a BioCLIP 2 cosine of 0.98 or more.

**1 match, 0 not allowed.**

| Evaluation photo | Training image | Hamming | Cosine | Allowed |
|---|---|---|---|---|
| `inat/389981240-714407985.jpg` | `inat_species/tomato/113989783-192529074.jpg` | 24 | 0.9832 | yes |

## Nearest training image per photo

Smallest hash distance per photo, in bits (64 is as far as two hashes can be):

| Bits | 0 | 4 | 8 | 12 | 16 | 20 | 24 | 28 | 32 |
|---|---|---|---|---|---|---|---|---|---|
| Photos at or under | 0 | 0 | 4 | 128 | 253 | 253 | 253 | 253 | 253 |

Highest cosine per photo: median 0.874, 95th percentile 0.930, maximum 0.983.

The 10 closest pairs by cosine:

| Evaluation photo | Training image | Cosine | Hamming |
|---|---|---|---|
| `inat/389981240-714407985.jpg` | `inat_species/tomato/113989783-192529074.jpg` | 0.9832 | 24 |
| `fieldsets/maize-bd-2026/gray-leaf-spot/img-467.jpg` | `maize_makerere/MLB_2/1621844077394.jpg` | 0.9504 | 30 |
| `fieldsets/maize-bd-2026/healthy-leaf/img-918.jpg` | `maize_makerere/Healthy_1/1621338889577.jpg` | 0.9393 | 30 |
| `fieldsets/maize-bd-2026/corn-leaf-blight/img-680.jpg` | `maize_makerere/MLB_2/1622019220141.jpg` | 0.9379 | 26 |
| `inat/388410969-711358321.jpg` | `tom2024/tomato_diseases/Blossom_end_rot_d/1356945245_1660910156937.jpg` | 0.9379 | 28 |
| `inat/388410969-711358332.jpg` | `tom2024/tomato_diseases/tomato_late_blight_d/254027665_1660904287854.jpg` | 0.9362 | 30 |
| `inat/401345730-736623744.jpg` | `plantdoc_cls/PlantDoc-Dataset-5467f6012d78d1c446145d5f582da6096f852ae8/train/Squash Powdery mildew leaf/5HttOvj.jpg` | 0.9352 | 28 |
| `fieldsets/maize-bd-2026/common-rust/img-234.jpg` | `maize_makerere/Healthy_2/1622093596564.jpg` | 0.9342 | 22 |
| `fieldsets/maize-bd-2026/healthy-leaf/img-20.jpg` | `maize_makerere/Healthy_2/1621669168393.jpg` | 0.9341 | 28 |
| `fieldsets/maize-bd-2026/healthy-leaf/img-35.jpg` | `maize_makerere/Healthy_1/1621338752118.jpg` | 0.9337 | 26 |
