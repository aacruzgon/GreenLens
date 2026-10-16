# Model notices

This file lists every model file in `manifest.toml` with its licence, its authors, the citation they ask for and the caveats about its training data. It follows the design in [docs/vision/README.md §9](../../docs/vision/README.md#9-licences-and-notices). Every file in `/models` ships with this file and with `licences/`, and the app's About screen links to it.

The CPU development twins (the fp32 ONNX files and the `*.cpu.json` heads in `manifest.cpu.toml`) are the same entries and carry the same notices.

Full licence texts are in [`licences/`](licences/). The Apache-2.0 and CC-BY-4.0 texts come from the SPDX licence list, release tag `v3.29.0` of `spdx/license-list-data` (`text/<id>.txt`). Anything not confirmed in a primary source is marked *unverified*. Nothing here is legal advice.

## text_embedder

- **Model:** Snowflake/snowflake-arctic-embed-xs, revision `d8c86521100d3556476a063fc2342036d45c106f`.
- **Licence:** Apache-2.0, as the model card's License section states. The repository has no LICENSE file and the Hub's licence tag is empty. Text: `licences/Apache-2.0.txt`.
- **Authors:** Snowflake (Luke Merrick, Danmei Xu, Gaurav Nuti, Daniel Campos).
- **Citation:** Merrick, Xu, Nuti, Campos, "Arctic-Embed: Scalable, Efficient, and Accurate Text Embedding Models", arXiv:2405.05374, 2024.
- **Training data:** about 400 million query-document pairs from public datasets and proprietary web search data, then about 1 million triplets (model card). The mix isn't itemised, so its terms are *unverified*.

## text_embedder_tokenizer

- **File:** the tokenizer of the same model and revision as `text_embedder`.
- **Licence, authors, citation:** as `text_embedder` (Apache-2.0, `licences/Apache-2.0.txt`).

## yunet

- **Model:** `face_detection_yunet_2026may.onnx` from opencv/opencv_zoo, commit `26cc381e4d2594bb9f47a26eb8fd96c94a13660d`.
- **Licence:** MIT, "Copyright (c) 2020 Shiqi Yu". Text: `licences/MIT-yunet.txt`, copied verbatim from `models/face_detection_yunet/LICENSE` at that commit.
- **Authors:** Wei Wu, Hanyang Peng, Shiqi Yu.
- **Citation:** Wu, Peng, Yu, "YuNet: A Tiny Millisecond-level Face Detector", Machine Intelligence Research 20(5):656-665, 2023.
- **Training-data caveat:** YuNet was trained on WIDER FACE, which is licensed CC BY-NC-ND 4.0 (non-commercial, no derivatives). The authors publish the weights under MIT. Whether a dataset's terms reach a model trained on it is unsettled, and every face detector shares this question. It is an open decision for the team (§9 item 2). We use YuNet only to blur faces in uploaded photos.

## bioclip

- **Model:** the vision tower of imageomics/bioclip-2, revision `2957b322090f9cb17ae72c71981c7218a28d81e0`, exported to ONNX (fp16) by `ml/training/export/bioclip.py`. The weights are otherwise unchanged.
- **Licence:** MIT. Text: `licences/MIT-bioclip-2.txt`. The Hugging Face repository has no LICENSE file at that revision, only the model card's `license: MIT`. The text is the LICENSE file of the GitHub repository Imageomics/bioclip-2 (default branch, read 2026-10-16), which lists several copyright holders, including OpenCLIP's authors. It isn't pinned to that revision.
- **Authors:** Jianyang Gu, Samuel Stevens, Elizabeth G. Campolongo, Matthew J. Thompson, Net Zhang, Jiaman Wu, Andrei Kopanev, Zheda Mai, Alexander E. White, James Balhoff, Wasila M. Dahdul, Daniel Rubenstein, Hilmar Lapp, Tanya Berger-Wolf, Wei-Lun Chao, Yu Su (Imageomics Institute).
- **Citation (the model card asks for both):**
  - Gu et al., "BioCLIP 2: Emergent Properties from Scaling Hierarchical Contrastive Learning", NeurIPS 2025, vol. 38, pp. 102778-102811, arXiv:2505.23883.
  - Gu et al., "BioCLIP 2", software, Hugging Face, v1.0.0, 2025, doi:10.57967/hf/5765.
  - The card also suggests citing OpenCLIP (Ilharco et al.) and BioCLIP (Stevens et al., CVPR 2024).
- **Training-data caveat:** trained on TreeOfLife-200M, starting from a CLIP ViT-L/14 pre-trained on LAION-2B. TreeOfLife-200M is tagged CC0 as a compilation. Its predecessor TreeOfLife-10M states that its images keep their own terms, some of them CC BY-NC-SA. The same likely holds for 200M, which is *unverified*. See §9 item 2.

## leaf_detector

- **Model:** RF-DETR-S (rfdetr 1.11.2) COCO weights, fine-tuned by us on PlantDoc's detection boxes collapsed to one class, `leaf`, and exported to ONNX (fp16).
- **Licence:** Apache-2.0 for the RF-DETR-S weights and code (Roboflow); our fine-tune is a derivative of them. Text: `licences/Apache-2.0.txt`. Only the N, S, M and L sizes are Apache-2.0; the larger sizes are not used.
- **Authors:** Roboflow (Isaac Robinson, Peter Robicheaux, Matvei Popov, Deva Ramanan, Neehar Peri).
- **Citation:** Robinson et al., "RF-DETR: Neural Architecture Search for Real-Time Detection Transformers", arXiv:2511.09554.
- **Training data:** PlantDoc detection split. See [Datasets](#datasets) for its licence and the caveat on the scraped photos. The base weights were trained on COCO, whose terms are *unverified* here.

## sam_encoder

- **Model:** facebook/sam2.1-hiera-tiny, revision `de431c4043854a71d8101e17995dfe596bf101a5`, with facebookresearch/sam2 at `2b90b9f5`, exported to ONNX (fp16) by `ml/training/export/sam2.py`.
- **Licence:** Apache-2.0 (Meta). Text: `licences/Apache-2.0.txt`.
- **Authors:** Nikhila Ravi, Valentin Gabeur and co-authors at Meta.
- **Citation:** Ravi et al., "SAM 2: Segment Anything in Images and Videos", arXiv:2408.00714, 2024.
- **Training data:** SA-1B and SA-V (from memory of the paper, *unverified*), with terms also *unverified*.

## sam_decoder

- **Model:** the box-prompted mask decoder of the same checkpoint, exported separately.
- **Licence, authors, citation, training data:** as `sam_encoder` (Apache-2.0, `licences/Apache-2.0.txt`).

## bioclip_labels_233482ce598f591b

- **What it is:** text embeddings made by BioCLIP 2's text tower for a stand-in list of 20 species names (`ml/training/labels/standin_labels.yaml`), using the openai-imagenet-80 prompt templates. It is a stand-in until the knowledge seed has real species files.
- **Licence:** MIT, as BioCLIP 2. Text: `licences/MIT-bioclip-2.txt`.
- **Authors and citation:** as `bioclip`.
- **Caveats:** as `bioclip` (TreeOfLife-200M). The species names and templates are ours and carry no third-party terms.

## disease_head_disease-head-v1d-2026-10-16

- **Model:** our logistic-regression disease head, trained on BioCLIP 2 image features (`bioclip`). The weights are ours.
- **Licence:** the project's licence isn't chosen yet (§9 item 1). The head inherits the BioCLIP 2 MIT text (`licences/MIT-bioclip-2.txt`) for the features it was trained on.
- **Training data:** PlantDoc (CC BY 4.0), TOM2024 (CC BY 4.0), Tomato-Pakistan 2026 (CC BY 4.0 as published) and iBean (MIT). Licence texts: `licences/CC-BY-4.0.txt`, `licences/MIT-ibean.txt`. Citations and caveats are in [Datasets](#datasets).
- **Caveats:** the Tomato-Pakistan caveat below applies. The head was trained on BioCLIP 2 features, so the `bioclip` training-data caveat also applies.

## Datasets

CC BY 4.0 requires attribution, a link to the licence, and a note of changes. Our weights are trained on these datasets and the images are never redistributed. Licence texts: `licences/CC-BY-4.0.txt` and `licences/MIT-ibean.txt`.

| Dataset | Licence | Citation | Link |
|---|---|---|---|
| PlantDoc (classification and detection splits) | CC BY 4.0 for the annotations; the scraped photos keep their owners' copyright (*unverified*) | Singh et al., "PlantDoc: A Dataset for Visual Plant Disease Detection", CoDS-COMAD 2020 | [classification](https://github.com/pratikkayal/PlantDoc-Dataset), [detection](https://github.com/pratikkayal/PlantDoc-Object-Detection-Dataset) |
| TOM2024 | CC BY 4.0 | Appiah O. et al. (2024), Mendeley Data V1, doi:10.17632/3d4yg89rtr.1; Data in Brief 59, 111357, doi:10.1016/j.dib.2025.111357 | <https://data.mendeley.com/datasets/3d4yg89rtr/1> |
| Tomato-Pakistan 2026 | CC BY 4.0 as published; it can't cover photos the authors didn't take | Malik M.N., Ali S.H., Ali H. (2026), Mendeley Data V1, doi:10.17632/3mbnb82mxd.1 | <https://data.mendeley.com/datasets/3mbnb82mxd/1> |
| iBean | MIT, "Copyright (c) 2020 AIR Lab Makerere University" | Makerere AI Lab (2020), Bean disease dataset | <https://github.com/AI-Lab-Makerere/ibean/> |
| WIDER FACE (YuNet's training data) | CC BY-NC-ND 4.0, see `yunet` | Yang et al., "WIDER FACE: A Face Detection Benchmark", CVPR 2016 (*unverified*) | <http://shuoyang1213.me/WIDERFACE/> |
| TreeOfLife-200M (BioCLIP 2's training data) | CC0 as a compilation; images may keep their own terms, see `bioclip` | see the BioCLIP 2 paper | <https://huggingface.co/datasets/imageomics/TreeOfLife-200M> |

**PlantDoc.** The photos were scraped from the web, so their owners keep their copyright (§9 item 3). This is fine for training; we never redistribute the images.

**Tomato-Pakistan 2026.** Every file is an augmented copy, about 934 originals. 302 of the originals are PlantDoc's scraped photos, and our mapping drops them. Any other scraped photos keep their owners' copyright (*unverified*). Shipping weights trained on this set is a team decision (§9). No Data in Brief paper was found.

**iBean.** Smartphone field photos of bean leaves from Uganda, annotated by NaCRRI. The head uses it only if beans become supported.
