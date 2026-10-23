# Face check, landmarks: faces-landmarks-2026-10-22

A prototype check, not shipped: could YuNet's five landmarks (right eye, left eye, nose tip, right and left mouth corners), which `api` ignores, tell the face boxes on leaves from real faces? Same photos and scoring as [faces-2026-10-22](faces-2026-10-22.md): 93 Open Images photos as uploaded, and the 152 dev plant photos.

**Method.**

- `vision`'s `face_check` example with `--landmarks` runs `FaceDetector::detect`, then the same passes again with a decode that keeps the landmarks, and merges them as `union_merge` does, keeping each merged box's members (the pass boxes merged into it). The merged boxes are the shipped detector's: equal to `detect`'s in the same run on every photo, and to the main report's runs (table below). The decode matches OpenCV 4.12's `FaceDetectorYN` to 0.0002 px on 80 boxes of 6 images at 1,280 px.
- A merged box takes the landmarks of its **highest-scoring member** (`on (top member)`). `on (any member)` keeps a box when any member passes.
- **Bounds** on the landmarks, each normalised by the box or by the face's own axis: "up" runs from the mouth corners' midpoint to the eyes' midpoint, and L is that distance. Each bound is set so it alone keeps 99.5% of the true-face boxes (boxes matching an annotated face) at threshold 0.15 on half A of the photos (47 photos, 175 true-face boxes; half B is the other 46, alternately by sorted photo ID). The same bounds apply at every threshold.
- Recall is measured on half B (held out), and on all 93 photos, with growth 25% as shipped and 35% as the main report proposes. The dev photos never set anything. `Fit on B` swaps the halves.
- **Learned:** logistic regression on the same features, the true-face boxes against the boxes on no annotated face in half A's photos, cut to keep 99.5% of half A's true-face boxes.

## What it shows

Written after looking at every true-face box the bounds drop at 0.15, the two faces missed under the closest setting, and 4 of the 31 pages of dev plant boxes with their points drawn, in `ml/training/runs/faces/landmarks/crops/`.

**The landmarks don't tell leaves from faces.** YuNet places two eyes, a nose and two mouth corners in a face's layout inside every box it outputs. On cedar-apple rust galls, grape and squash leaf spots, maize streaks, spider-mite damage, and even the plain sheet behind the maize leaves, the points sit where a face's would. At 0.15, the median nose lies 0.48 of the way from the mouth line to the eye line on the dev plant boxes, against 0.45 on true faces. The eyes-to-mouth distance is 0.46 of the box against 0.43, and the roll is 3.0° against 3.6°. The bounds keep 91.9% of the dev plant boxes (1,411 of 1,535) and 93.8% of the true-face boxes (439 of 468).

**So the filter barely moves the false blur.** At the shipped 0.3, it removes every box from 2 of the 89 blurred dev photos (87 left, 57.2%; 10.7% of an average photo blurred instead of 11.5%). At 0.15, 116 of 119 are left. The learned rule does no better: 118 of 119 at 0.15. Nothing comes near 2%.

**It costs recall.** At 0.3 with the top member's points, recall falls from 94.7% to 92.5% on all 93 photos, and from 97.1% to 96.0% on the held-out half. It loses 6 more faces: two tilted heads past the 37.7° roll bound (`b863b3c0ad584678#0`, a smiling man at score 0.90, and `272d61fa512fd9e2#3`), a profile (`c6f1a6d26c665d0f#0`), a face in shadow (`e055b57760e3470c#3`), a face against a rock (`cc31ef58bdf2194b#0`), and one now covered 87.5% (`06171c6d90f7c087#0`). Keeping a box when any member passes costs less (94.3%), and removes even less blur (88 photos). Near-profile boxes suffer most: at 0.15 the bounds keep 81.8% of them (63 of 77). Bounds set on half B instead are tighter (roll at most 25.0° against 37.7°) and drop more faces: 87.8% on half A at 0.15 with the top member's points, against 95.3% unfiltered.

**The true-face boxes the bounds drop** are mostly partial boxes on a face (a cheek, an ear, half a face at a tile's edge), boxes on an arm or a hand next to a face, the tilted heads, and the profiles. Leaves give no such tell.

**Do the passes agree on the points?** No more on faces than on leaves: on boxes with two members or more, the largest disagreement is 0.44 of L at the median on true faces, 0.50 on other boxes, and 0.45 on dev plant boxes ("The features at 0.15").

**98% recall is reachable, but not with this filter's false blur.** The filter can only lower recall, so the ceiling at each threshold is the unfiltered row. On all 93 photos only 0.15 with growth 35% passes 98%: 99.3% (280 of 282), and 99.4% on the held-out half. With any member's points it keeps 99.3%, but 117 of the 152 dev photos (77%: 86 iNaturalist and 31 field-set photos) still get a box, and 38.7% of an average photo is blurred. Its two misses are faces the main report lists: `5762f6ddd53543d6#1`, the out-of-focus profile behind flowers, and `f6888e526a4c11d0#0`, the 20 px face.

**Proposal: don't change `faces.rs` for the landmarks.** They come out of the same network as the false box, so they can't veto it. A check that can veto it needs its own view of the pixels, which this check doesn't measure. Any change to `faces.rs` or `FaceConfig` waits on the team, with Sara as the backend owner.

## Bounds, set on half A

| Bound | Value | What it says |
|---|---|---|
| `outside max` | 0.170 | how far the farthest point may lie outside the box, as a share of the box's side |
| `roll max` | 37.679 | degrees between the face's "up" and the image's up |
| `nose_height min` | 0.218 | the nose along "up": 0 is the mouth line, 1 the eye line |
| `nose_height max` | 0.645 |  |
| `eye_mouth min` | 0.287 | L as a share of the box's size, √(w·h) |
| `eye_mouth max` | 0.698 |  |
| `eye_side min` | -0.104 | the right-to-left eye vector across "up", over L (below 0: mirrored) |
| `mouth_side min` | -0.126 | the same for the mouth corners |
| `eye_width max` | 0.639 | eye distance as a share of the box's size (no minimum, so profiles pass) |
| `mouth_eye min` | 0.372 | mouth width over eye distance |
| `mouth_eye max` | 1.387 |  |

## Recall and false blur, original uploads

Recall: faces 20 px+ across in the stored image whose box the grown boxes cover 90% or more. Dev: the dev plant photos with any box left (target 2%), and the share of an average dev photo blurred. Bounds set on half A unless marked.

| Threshold | Growth | Filter | Recall, half B (held out) | Recall, all 93 | Recall, half A (fit on B) | Dev photos with a box | Dev area blurred, mean (max) | Dev photos, fit on B |
|---|---|---|---|---|---|---|---|---|
| 0.3 | 25% | off | 97.1% (170/175) | 94.7% (267/282) | 90.6% (97/107) | 89/152 (58.6%) | 11.5% (85%) | 89/152 |
| 0.3 | 25% | on (top member) | 96.0% (168/175) | 92.5% (261/282) | 83.2% (89/107) | 87/152 (57.2%) | 10.7% (85%) | 87/152 |
| 0.3 | 25% | on (any member) | 97.1% (170/175) | 94.3% (266/282) | 89.7% (96/107) | 88/152 (57.9%) | 10.7% (85%) | 88/152 |
| 0.3 | 25% | learned | 97.1% (170/175) | 94.7% (267/282) | 90.6% (97/107) | 88/152 (57.9%) | 10.9% (85%) | 89/152 |
| 0.3 | 35% | off | 98.3% (172/175) | 96.1% (271/282) | 92.5% (99/107) | 89/152 (58.6%) | 13.3% (92%) | 89/152 |
| 0.3 | 35% | on (top member) | 97.1% (170/175) | 94.3% (266/282) | 85.0% (91/107) | 87/152 (57.2%) | 12.4% (92%) | 87/152 |
| 0.3 | 35% | on (any member) | 98.3% (172/175) | 96.1% (271/282) | 91.6% (98/107) | 88/152 (57.9%) | 12.5% (92%) | 88/152 |
| 0.3 | 35% | learned | 98.3% (172/175) | 96.1% (271/282) | 92.5% (99/107) | 88/152 (57.9%) | 12.7% (92%) | 89/152 |
| 0.2 | 25% | off | 98.3% (172/175) | 96.5% (272/282) | 93.5% (100/107) | 110/152 (72.4%) | 25.4% (100%) | 110/152 |
| 0.2 | 25% | on (top member) | 97.1% (170/175) | 94.0% (265/282) | 84.1% (90/107) | 107/152 (70.4%) | 23.3% (100%) | 107/152 |
| 0.2 | 25% | on (any member) | 98.3% (172/175) | 95.7% (270/282) | 91.6% (98/107) | 108/152 (71.0%) | 24.2% (100%) | 108/152 |
| 0.2 | 25% | learned | 98.3% (172/175) | 96.5% (272/282) | 93.5% (100/107) | 109/152 (71.7%) | 24.2% (100%) | 110/152 |
| 0.2 | 35% | off | 99.4% (174/175) | 97.5% (275/282) | 94.4% (101/107) | 110/152 (72.4%) | 28.5% (100%) | 110/152 |
| 0.2 | 35% | on (top member) | 98.3% (172/175) | 95.4% (269/282) | 85.0% (91/107) | 107/152 (70.4%) | 26.3% (100%) | 107/152 |
| 0.2 | 35% | on (any member) | 99.4% (174/175) | 97.2% (274/282) | 92.5% (99/107) | 108/152 (71.0%) | 27.3% (100%) | 108/152 |
| 0.2 | 35% | learned | 99.4% (174/175) | 97.5% (275/282) | 94.4% (101/107) | 109/152 (71.7%) | 27.3% (100%) | 110/152 |
| 0.15 | 25% | off | 98.3% (172/175) | 97.2% (274/282) | 95.3% (102/107) | 119/152 (78.3%) | 37.3% (100%) | 119/152 |
| 0.15 | 25% | on (top member) | 97.1% (170/175) | 95.4% (269/282) | 87.8% (94/107) | 116/152 (76.3%) | 34.3% (100%) | 117/152 |
| 0.15 | 25% | on (any member) | 98.3% (172/175) | 97.2% (274/282) | 94.4% (101/107) | 117/152 (77.0%) | 35.1% (100%) | 119/152 |
| 0.15 | 25% | learned | 98.3% (172/175) | 97.2% (274/282) | 95.3% (102/107) | 118/152 (77.6%) | 35.5% (100%) | 119/152 |
| 0.15 | 35% | off | 99.4% (174/175) | 99.3% (280/282) | 99.1% (106/107) | 119/152 (78.3%) | 41.1% (100%) | 119/152 |
| 0.15 | 35% | on (top member) | 98.3% (172/175) | 97.5% (275/282) | 92.5% (99/107) | 116/152 (76.3%) | 38.0% (100%) | 117/152 |
| 0.15 | 35% | on (any member) | 99.4% (174/175) | 99.3% (280/282) | 98.1% (105/107) | 117/152 (77.0%) | 38.7% (100%) | 119/152 |
| 0.15 | 35% | learned | 99.4% (174/175) | 99.3% (280/282) | 99.1% (106/107) | 118/152 (77.6%) | 39.2% (100%) | 119/152 |

## Boxes the bounds keep

Merged boxes, by the landmarks of their top member, with the bounds set on half A. True: on an annotated face (all 93 photos). Other: on no annotated face in the Open Images photos (dogs, wheels, fabric, a few unannotated faces). Near profile: true-face boxes whose eye distance is under 50% of L.

| Threshold | True-face boxes | Near profile | Other boxes | Dev plant boxes |
|---|---|---|---|---|
| 0.3 | 338/354 (95.5%) | 48/56 (85.7%) | 534/571 (93.5%) | 477/510 (93.5%) |
| 0.2 | 391/412 (94.9%) | 51/60 (85.0%) | 1107/1206 (91.8%) | 978/1057 (92.5%) |
| 0.15 | 439/468 (93.8%) | 63/77 (81.8%) | 1690/1870 (90.4%) | 1411/1535 (91.9%) |

On half A at 0.15, the bounds together keep 167 of its 175 true-face boxes.

Boxes each bound breaks at 0.15 (a box can break several):

| Bound | True-face boxes | Other boxes | Dev plant boxes |
|---|---|---|---|
| `outside max` | 2 | 12 | 10 |
| `roll max` | 2 | 5 | 9 |
| `nose_height min` | 3 | 24 | 10 |
| `nose_height max` | 3 | 23 | 32 |
| `eye_mouth min` | 2 | 26 | 16 |
| `eye_mouth max` | 8 | 68 | 45 |
| `eye_side min` | 1 | 3 | 1 |
| `mouth_side min` | 2 | 4 | 5 |
| `eye_width max` | 1 | 47 | 17 |
| `mouth_eye min` | 6 | 7 | 7 |
| `mouth_eye max` | 3 | 7 | 12 |

## The features at 0.15

Quantiles 0.5%, 5%, 50%, 95% and 99.5% of the top member's features, all 93 photos and every dev photo. Not bounds: `yaw` is eye distance over L; `spread`, on boxes with two members or more, is the largest distance between a point of the top member and the same point of another member, over L.

| Feature | True-face boxes | Other boxes | Dev plant boxes |
|---|---|---|---|
| `outside` | 0 / 0 / 0 / 0.015 / 0.155 | 0 / 0 / 0 / 0.011 / 0.227 | 0 / 0 / 0 / 0.003 / 0.257 |
| `roll` | 0.053 / 0.33 / 3.585 / 13.941 / 34.225 | 0.016 / 0.208 / 2.939 / 12.826 / 27.338 | 0.03 / 0.234 / 2.956 / 12.56 / 38.425 |
| `nose_height` | 0.139 / 0.339 / 0.452 / 0.581 / 0.656 | 0.131 / 0.314 / 0.461 / 0.595 / 0.727 | 0.198 / 0.35 / 0.477 / 0.607 / 0.753 |
| `eye_mouth` | 0.289 / 0.355 / 0.425 / 0.628 / 0.741 | 0.242 / 0.33 / 0.468 / 0.684 / 0.763 | 0.276 / 0.333 / 0.455 / 0.668 / 0.768 |
| `eye_side` | -0.031 / 0.212 / 0.825 / 1.109 / 1.4 | 0.057 / 0.335 / 0.767 / 1.301 / 2.112 | 0.061 / 0.306 / 0.747 / 1.168 / 1.686 |
| `mouth_side` | -0.1 / 0.133 / 0.681 / 0.972 / 1.182 | -0.009 / 0.194 / 0.604 / 1.051 / 1.87 | -0.042 / 0.195 / 0.58 / 0.937 / 1.553 |
| `eye_width` | 0.031 / 0.101 / 0.374 / 0.495 / 0.595 | 0.054 / 0.151 / 0.372 / 0.584 / 0.747 | 0.037 / 0.134 / 0.347 / 0.547 / 0.672 |
| `mouth_eye` | 0.253 / 0.584 / 0.833 / 0.947 / 1.489 | 0.408 / 0.571 / 0.808 / 0.976 / 1.353 | 0.412 / 0.575 / 0.787 / 0.95 / 1.514 |
| `yaw` | 0.066 / 0.228 / 0.825 / 1.109 / 1.401 | 0.128 / 0.343 / 0.769 / 1.32 / 2.119 | 0.1 / 0.324 / 0.749 / 1.179 / 1.702 |
| `spread` | 0 / 0.029 / 0.435 / 3.527 / 25.996 | 0 / 0 / 0.5 / 4.321 / 23.423 | 0 / 0 / 0.452 / 5.609 / 47.142 |

## Checks

| Run | Photos | Boxes equal to `detect`'s | Equal to the main report's run | Merged boxes | With one member |
|---|---|---|---|---|---|
| openimages@0.3 | 93 | 93/93 | 93/93 | 925 | 333 |
| openimages@0.2 | 93 | 93/93 | 93/93 | 1618 | 668 |
| openimages@0.15 | 93 | 93/93 | 93/93 | 2338 | 971 |
| dev@0.3 | 152 | 152/152 | 152/152 | 510 | 181 |
| dev@0.2 | 152 | 152/152 | 152/152 | 1057 | 361 |
| dev@0.15 | 152 | 152/152 | 152/152 | 1535 | 505 |
