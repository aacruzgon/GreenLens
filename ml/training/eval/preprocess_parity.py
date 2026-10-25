"""Pre-processing parity between the worker and `ml/` (J1, vision dev-roadmap gap G8).

    python -m ml.training.eval.preprocess_parity run              # dump, compare, write the report
    python -m ml.training.eval.preprocess_parity run --no-embed   # without BioCLIP on the CPU
    python -m ml.training.eval.preprocess_parity report REPORT.json   # rewrite the .md from the JSON

The heads and thresholds were fitted on tensors made by `ml/pdm_vision/preprocess.py` (Pillow);
the perception worker makes its own with `vision`'s `preprocess::to_tensor` (`fast_image_resize`).
This runs the `vision` crate's `preprocess_dump` example on 10 dev photos and 5 leaf crops, which
writes every intermediate image and tensor as `.npy`, and compares them here in layers:

1. **Pre-processing (J1's acceptance).** From the same pixels, the stored photo the worker
   decodes, `pp.to_tensor` against the worker's tensor for BioCLIP, the leaf detector and SAM 2.1,
   and for each leaf crop `pp.crop_box` against the worker's crop (the same pixels, so the same
   rectangle) and their BioCLIP tensors. Target: max absolute difference under 0.02 per tensor
   after normalisation, about 1 level of 255 at the recipes' standard deviations.
2. **The upload's resize to 1,600 px,** `pp.resize` (as `heads/crops.py` imitates the stored photo)
   against the API's, from the same decoded pixels.
3. **Decoding,** Pillow's (`pp.load_rgb`) against the API's (`imaging::decode_upload`).
4. **The `WebP` round trip** the API stores and the worker decodes, which `ml/` doesn't apply.
5. **End to end:** what the heads were trained on (Pillow decode, `pp.resize` to 1,600 px, no
   `WebP`, `pp.to_tensor`) against what the worker sends, per recipe and per crop.
6. **BioCLIP embeddings** (`--embed`, the fp32 vision tower on the CPU) of layers 1 and 5: the
   cosine similarity between the two sides, which is what the species and disease heads read.

Photos: 10 dev photos of the evaluation set (never test), picked by `select_photos` to cover each
source, photos at or under the stored size and over it, and both orientations. Crops: 5 boxes in
`CROPS`, chosen to exercise the crop's rounding and clamping, not the detector.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import sys
import tomllib
from datetime import date
from pathlib import Path

import numpy as np
from PIL import Image

from ml.pdm_vision import preprocess as pp

ROOT = Path(__file__).resolve().parents[3]
EVAL = ROOT / "ml/training/eval"
EVAL_MANIFEST = EVAL / "manifest.jsonl"
PHOTOS = EVAL / "photos"
REPORTS = EVAL / "reports"
RUNS = ROOT / "ml/training/runs/preprocess-parity"
MODELS = ROOT / "ml/models/manifest.toml"
CPU_MODELS = Path.home() / ".cache/pdm-models-cpu"
TARGET = 0.02
STORED_LONG_SIDE = 1600

# The worker's recipe per manifest entry, and `pp`'s constant that `ml/` trains and evaluates with.
RECIPES = {"bioclip": pp.BIOCLIP, "leaf_detector": pp.RFDETR, "sam_encoder": pp.SAM2}

# Leaf boxes (normalised x, y, w, h) on the first five photos. They test the crop, not the
# detector: growth past the top-left corner, past the bottom-right, fractional edges, a box
# under 40 px in the stored photo, and a box covering most of it.
CROPS = [
    ("past the top-left corner", (0.02, 0.03, 0.30, 0.40)),
    ("past the bottom-right corner", (0.55, 0.50, 0.44, 0.49)),
    ("fractional edges", (0.3137, 0.2719, 0.1234, 0.0987)),
    ("under 40 px", (0.40, 0.35, 0.02, 0.03)),
    ("most of the photo", (0.10, 0.10, 0.80, 0.80)),
]


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def manifest_recipe(entry: dict) -> pp.Recipe:
    """A manifest entry's `[model.input]` as `pp.Recipe`."""
    i = entry["input"]
    return pp.Recipe(int(i["shape"][3]), pp.Resize(i["resize"]), tuple(i["mean"]), tuple(i["std"]),
                     i.get("colour", "rgb") == "bgr")


def manifest_entries(path: Path = MODELS) -> dict[str, dict]:
    return {e["name"]: e for e in tomllib.loads(path.read_text())["model"]}


def select_photos(rows: list[dict], n: int = 10) -> list[dict]:
    """`n` dev photos: one per (source, at or under the stored size, portrait) group first, then
    the rest, each group in order of the photo's SHA-256, so the pick doesn't depend on file order."""
    def group(r: dict) -> tuple:
        w, h = r["size"]
        return (r["file"].split("/")[0], max(w, h) <= STORED_LONG_SIDE, h > w)

    ordered = sorted(rows, key=lambda r: r["sha256"])
    picked, seen = [], set()
    for r in ordered:
        if group(r) not in seen:
            seen.add(group(r))
            picked.append(r)
    picked += [r for r in ordered if r not in picked]
    return sorted(picked[:n], key=lambda r: r["sha256"])


def dev_photos() -> list[dict]:
    rows = []
    for line in EVAL_MANIFEST.read_text().splitlines():
        case = json.loads(line)
        if case["split"] != "dev":
            continue
        for i, im in enumerate(case["images"]):
            with Image.open(PHOTOS / im["file"]) as x:
                size = x.size
            rows.append({"key": f"{case['case_id']}#{i}", "file": im["file"], "sha256": im["sha256"], "size": size})
    return rows


def diff(a: np.ndarray, b: np.ndarray, unit: float | None = None) -> dict:
    """Absolute differences of two arrays of one shape, in their own units; with `unit`, also the
    share of values more than one unit apart."""
    if a.shape != b.shape:
        return {"shape": [list(a.shape), list(b.shape)]}
    d = np.abs(a.astype(np.float64) - b.astype(np.float64))
    out = {"max": float(d.max()), "mean": float(d.mean()), "p99": float(np.quantile(d, 0.99)),
           "share_nonzero": float((d > 0).mean())}
    if unit is not None:
        out["share_over_1_level"] = float((d > unit * 1.001).mean())
    return out


def levels(a: np.ndarray, b: np.ndarray) -> dict:
    """Differences of two `u8` images in levels of 255, with the share that differ by more than 1."""
    out = diff(a, b)
    if "shape" not in out:
        out["share_over_1"] = float((np.abs(a.astype(np.int16) - b.astype(np.int16)) > 1).mean())
    return out


def one_level(recipe: pp.Recipe) -> float:
    """One level of 255 after normalisation, at the recipe's smallest standard deviation: the
    largest difference a 1-level disagreement makes."""
    return 1.0 / 255.0 / min(recipe.std)


def cargo_dump(job: dict) -> Path:
    """Runs `preprocess_dump` on the job and returns its `index.json`."""
    from ml.training.eval.spot_fit import cargo_example

    p = cargo_example("preprocess_dump", json.dumps(job) + "\n")
    assert p.stdout is not None
    lines = [json.loads(line) for line in p.stdout if line.strip()]
    if p.wait() != 0 or not lines or "index" not in lines[0]:
        raise SystemExit(f"preprocess_dump failed: {lines}")
    return Path(lines[0]["index"])


class Bioclip:
    """The fp32 vision tower on the CPU (`manifest.cpu.toml`'s file, hash checked)."""

    def __init__(self, root: Path = CPU_MODELS):
        import onnxruntime as ort

        entry = manifest_entries(ROOT / "ml/models/manifest.cpu.toml")["bioclip"]
        path = root / entry["file"]
        if sha256(path) != entry["sha256"]:
            raise SystemExit(f"{path}: hash differs from manifest.cpu.toml")
        self.file = entry["file"]
        self.sess = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
        self.inp = entry["input"]["name"]
        self.out = entry["outputs"]["embeddings"]

    def __call__(self, tensors: list[np.ndarray]) -> np.ndarray:
        e = self.sess.run([self.out], {self.inp: np.stack(tensors).astype(np.float32)})[0]
        return e / np.linalg.norm(e, axis=1, keepdims=True)


# The embedding comparisons, each a pair of BioCLIP tensors: the worker's code against `pp`'s from
# the same stored pixels; the stored photo against the one before `WebP` (both through `pp`); the
# API's decode and resize against Pillow's (both before `WebP`, through `pp`); and the whole chain.
EMBED_LAYERS = ("same_pixels", "webp", "decode_resize", "end_to_end")


def compare(index: dict, out: Path, embed: Bioclip | None) -> dict:
    """Every layer of the module docstring, per photo and per crop."""
    entries = manifest_entries()
    recipes = {name: manifest_recipe(entries[name]) for name in RECIPES}
    bio = recipes["bioclip"]
    photos, crops = [], []
    images: dict[str, dict[str, Image.Image]] = {}
    pairs: dict[str, list[tuple[np.ndarray, np.ndarray]]] = {k: [] for k in EMBED_LAYERS}

    def add_pairs(worker: np.ndarray, stored: Image.Image, resized: Image.Image, own: Image.Image) -> None:
        on_stored, on_resized, on_own = (pp.to_tensor(im, bio) for im in (stored, resized, own))
        for k, pair in zip(EMBED_LAYERS, ((worker, on_stored), (on_stored, on_resized),
                                          (on_resized, on_own), (worker, on_own))):
            pairs[k].append(pair)

    for ph in index["photos"]:
        load = lambda what: np.load(out / f"{ph['id']}.{what}.npy")  # noqa: E731
        decoded, resized, stored = load("decoded"), load("resized"), load("stored")
        own = pp.load_rgb(ph["path"])
        own_stored = pp.resize(own, *pp.fit_long_side(own.size, STORED_LONG_SIDE))
        im = Image.fromarray(decoded)
        py_resized = np.asarray(pp.resize(im, *pp.fit_long_side(im.size, STORED_LONG_SIDE)))
        images[ph["id"]] = {"stored": Image.fromarray(stored), "resized": Image.fromarray(resized), "own": own_stored}
        row = {
            "id": ph["id"], "photo": ph["key"], "decoded": ph["decoded"], "stored": ph["stored"],
            "decode": levels(decoded, np.asarray(own)),
            "resize": levels(resized, py_resized),
            "resize_hfirst": levels(load("resized.hfirst"), py_resized),
            "webp": levels(stored, resized),
            "recipes": {},
        }
        for name, recipe in recipes.items():
            t = load(name)
            same = pp.to_tensor(images[ph["id"]]["stored"], recipe)
            unit = one_level(recipe)
            row["recipes"][name] = {"same_pixels": diff(t, same, unit),
                                    "end_to_end": diff(t, pp.to_tensor(own_stored, recipe), unit),
                                    "same_pixels_hfirst": diff(load(f"{name}.hfirst"), same, unit)}
        add_pairs(load("bioclip"), *(images[ph["id"]][k] for k in ("stored", "resized", "own")))
        photos.append(row)
    grow = float(index["crop_grow"])
    unit = one_level(bio)
    for c in index["crops"]:
        rust_crop = np.load(out / f"{c['id']}.crop.npy")
        t = np.load(out / f"{c['id']}.bioclip.npy")
        cut = {k: pp.crop_box(v, tuple(c["bbox"]), grow) for k, v in images[c["photo"]].items()}
        py_crop = np.asarray(cut["stored"])
        same = pp.to_tensor(cut["stored"], bio)
        crops.append({
            "id": c["id"], "photo": c["photo"], "label": c["label"], "bbox": c["bbox"], "rect": c["rect"],
            "same_rect": py_crop.shape == rust_crop.shape and bool(np.array_equal(py_crop, rust_crop)),
            "py_size": [py_crop.shape[1], py_crop.shape[0]],
            "same_pixels": diff(t, same, unit),
            "end_to_end": diff(t, pp.to_tensor(cut["own"], bio), unit),
            "same_pixels_hfirst": diff(np.load(out / f"{c['id']}.bioclip.hfirst.npy"), same, unit),
        })
        add_pairs(t, cut["stored"], cut["resized"], cut["own"])
    result = {"photos": photos, "crops": crops}
    if embed is not None:
        n = len(photos)
        cos = {}
        for k, ps in pairs.items():
            a, b = embed([x for x, _ in ps]), embed([y for _, y in ps])
            cos[k] = [float(v) for v in (a * b).sum(axis=1)]
        result["embeddings"] = {"model": embed.file,
                                "photos": {k: v[:n] for k, v in cos.items()},
                                "crops": {k: v[n:] for k, v in cos.items()}}
    return result


def summary(result: dict, recipes: dict[str, pp.Recipe]) -> dict:
    """The worst case per recipe and layer, and whether J1's target holds."""
    out = {}
    for name in RECIPES:
        same = [p["recipes"][name]["same_pixels"]["max"] for p in result["photos"]]
        e2e = [p["recipes"][name]["end_to_end"]["max"] for p in result["photos"]]
        out[name] = {"same_pixels_max": max(same), "end_to_end_max": max(e2e),
                     "same_pixels_share_over_1_level": max(p["recipes"][name]["same_pixels"]["share_over_1_level"] for p in result["photos"]),
                     "hfirst_max": max(p["recipes"][name]["same_pixels_hfirst"]["max"] for p in result["photos"]),
                     "end_to_end_mean": max(p["recipes"][name]["end_to_end"]["mean"] for p in result["photos"]),
                     "one_level": one_level(recipes[name])}
    out["crops"] = {"same_pixels_max": max(c["same_pixels"]["max"] for c in result["crops"]),
                    "end_to_end_max": max(c["end_to_end"]["max"] for c in result["crops"]),
                    "hfirst_max": max(c["same_pixels_hfirst"]["max"] for c in result["crops"]),
                    "same_rect": all(c["same_rect"] for c in result["crops"])}
    out["passes"] = (all(out[n]["same_pixels_max"] < TARGET for n in RECIPES)
                     and out["crops"]["same_pixels_max"] < TARGET and out["crops"]["same_rect"])
    for layer in ("decode", "resize", "resize_hfirst", "webp"):
        out[layer] = {"max": max(p[layer]["max"] for p in result["photos"]),
                      "mean": max(p[layer]["mean"] for p in result["photos"]),
                      "share_over_1": max(p[layer]["share_over_1"] for p in result["photos"])}
    if "embeddings" in result:
        e = result["embeddings"]
        out["embeddings"] = {k: {"photos": min(e["photos"][k]), "crops": min(e["crops"][k])} for k in EMBED_LAYERS}
    return out


def run(args) -> int:
    today = date.today().isoformat()
    rows = dev_photos()
    picked = select_photos(rows, args.photos)
    out = RUNS / today
    if out.exists():
        shutil.rmtree(out)
    photos = [{"id": f"p{i}", "path": str(PHOTOS / r["file"])} for i, r in enumerate(picked)]
    crops = [{"id": f"c{i}", "photo": f"p{i}", "bbox": list(b)} for i, (_, b) in enumerate(CROPS)]
    index_path = cargo_dump({"manifest": str(MODELS), "out": str(out), "photos": photos, "crops": crops})
    index = json.loads(index_path.read_text())
    for ph, r in zip(index["photos"], picked):
        ph["key"] = r["key"]
    for c, (label, _) in zip(index["crops"], CROPS):
        c["label"] = label
    embed = None if args.no_embed else Bioclip()
    result = compare(index, out, embed)
    entries = manifest_entries()
    recipes = {name: manifest_recipe(entries[name]) for name in RECIPES}
    rust = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
    git = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    report = {
        "date": today,
        "command": "python -m ml.training.eval.preprocess_parity run" + (" --no-embed" if args.no_embed else ""),
        "inputs": {
            "commit": git,
            "eval_manifest": {"path": str(EVAL_MANIFEST.relative_to(ROOT)), "sha256": sha256(EVAL_MANIFEST)},
            "model_manifest": {"path": str(MODELS.relative_to(ROOT)), "sha256": sha256(MODELS)},
            "photos": [{"id": p["id"], "key": r["key"], "file": r["file"], "sha256": r["sha256"], "size": list(r["size"])}
                       for p, r in zip(photos, picked)],
            "crops": [{"id": c["id"], "photo": c["photo"], "label": label, "bbox": list(b)}
                      for c, (label, b) in zip(crops, CROPS)],
            "worker": {"example": "backend/crates/vision/examples/preprocess_dump.rs", "rustc": rust,
                       "crop_grow": index["crop_grow"], "stored_long_side": index["stored_long_side"],
                       "webp_quality": index["webp_quality"]},
            "python": {"version": platform.python_version(), "pillow": Image.__version__, "numpy": np.__version__,
                       "resize_filter": pp.RESIZE_FILTER},
        },
        "target": TARGET,
        "recipes": {name: {"side": r.side, "resize": r.resize.value, "mean": list(r.mean), "std": list(r.std),
                           "matches_pp": r == RECIPES[name]} for name, r in recipes.items()},
        "summary": summary(result, recipes),
        **result,
    }
    path = REPORTS / f"preprocess-parity-{today}.json"
    path.write_text(json.dumps(report, indent=1) + "\n")
    path.with_suffix(".md").write_text(report_md(report))
    print(json.dumps(report["summary"], indent=1))
    print(f"wrote {path.relative_to(ROOT)} and .md", file=sys.stderr)
    return 0


def f(x: float, digits: int = 4) -> str:
    return f"{x:.{digits}f}"


def report_md(r: dict) -> str:
    s, inp = r["summary"], r["inputs"]
    w = inp["worker"]
    names = list(RECIPES)
    lvl = {n: s[n]["one_level"] for n in names}
    worst_share = max(s[n]["same_pixels_share_over_1_level"] for n in names)
    best_share = min(s[n]["same_pixels_share_over_1_level"] for n in names)
    hfirst_bio = max(s["bioclip"]["hfirst_max"], s["crops"]["hfirst_max"])
    hfirst_rest = max(s[n]["hfirst_max"] for n in names)
    hfirst_share = max([p["recipes"][n]["same_pixels_hfirst"]["share_over_1_level"] for p in r["photos"] for n in names]
                       + [c["same_pixels_hfirst"]["share_over_1_level"] for c in r["crops"]])
    e = s.get("embeddings")
    lines = [
        f"# Pre-processing parity: preprocess-parity-{r['date']}",
        "",
        f"J1 ([dev-roadmap](../../../../docs/vision/dev-roadmap.md), gap G8), {r['date']}: `{r['command']}` on commit "
        f"`{inp['commit'][:8]}`. The `vision` crate's `preprocess_dump` example runs the upload's and the perception "
        f"worker's steps on {len(r['photos'])} dev photos and {len(r['crops'])} leaf crops and writes every image and "
        "tensor; `ml/pdm_vision/preprocess.py` makes its own from the same pixels and from the photo files. Inputs: "
        f"`{inp['eval_manifest']['path']}` (SHA-256 `{inp['eval_manifest']['sha256'][:16]}…`), the recipes and "
        f"`crop_grow` ({w['crop_grow']:g}) of `{inp['model_manifest']['path']}` (`{inp['model_manifest']['sha256'][:16]}…`), "
        f"the upload's {w['stored_long_side']} px and `WebP` quality {w['webp_quality']:g}; {w['rustc']}, Python "
        f"{inp['python']['version']}, Pillow {inp['python']['pillow']}, NumPy {inp['python']['numpy']}.",
        "",
        f"## Result: {'passes' if s['passes'] else 'fails'} the {r['target']} target, for one reason, on the worker's side",
        "",
        "- **From the same stored pixels,** the largest difference in a tensor is "
        + ", ".join(f"{f(s[n]['same_pixels_max'], 3)} for `{n}`" for n in names)
        + f" and {f(s['crops']['same_pixels_max'], 3)} for the leaf crops, against {r['target']}. One level of 255 is "
        + f"{f(lvl['bioclip'])} at BioCLIP's smallest standard deviation and {f(lvl['leaf_detector'])} at ImageNet's, so the "
        + f"target allows one level. On the worst photo, {best_share:.1%} to {worst_share:.1%} of the values are more than one level apart.",
        "- **The cause is the order of the resize's two passes.** `fast_image_resize` 6.1.0 resizes an 8-bit image "
        "vertically first (`Resizer::do_convolution`: \"For u8-based images, it is faster to do the vertical pass "
        "first\"); Pillow resizes horizontally first. Both round and clip to 8 bits between the passes, so Lanczos3's "
        "overshoot at a strong edge is clipped at different points. The large differences sit on high-contrast edges "
        "with values near 0 or 255.",
        f"- **The worker is the side to change.** The heads, the plant probe and the thresholds were fitted on Pillow's "
        f"pixels. The example's diagnostic `resize_h_first`, the same `fast_image_resize` call as two one-axis passes, "
        f"horizontal first, brings BioCLIP's tensors and the crops within one level (max {f(hfirst_bio)}) and the "
        f"detector's and SAM's within two ({f(hfirst_rest)}, at most {hfirst_share:.0e} of a tensor's values, most likely "
        f"the two libraries' fixed-point rounding). It also brings the upload's {w['stored_long_side']} px resize from "
        f"{s['resize']['max']:.0f} levels to {s['resize_hfirst']['max']:.0f}. The change belongs in `imaging::resize`.",
    ]
    if e:
        lines.append(f"- **Its effect is small:** from the same pixels, BioCLIP's embeddings agree to a cosine similarity of "
                     f"{e['same_pixels']['photos']:.4f} or more on the photos and {e['same_pixels']['crops']:.4f} or more on the crops.")
    lines += [
        "- **The crop's rounding is confirmed:** every crop has the same rectangle and pixels as `pp.crop_box`'s, "
        "with the growth clamped at both corners." if s["crops"]["same_rect"] else
        "- **The crop's rectangles differ** from `pp.crop_box`'s.",
        "",
    ]
    if e:
        crop_webp = r["embeddings"]["crops"]["webp"]
        worst = r["crops"][crop_webp.index(min(crop_webp))]
        lines += [
            "## Larger, and not pre-processing: the `WebP` the API stores",
            "",
            f"The API stores each photo as lossy `WebP` at quality {w['webp_quality']:g}, and the worker decodes that; "
            f"`ml/` trains and calibrates on the photo before it (`heads/crops.py` stores at {w['stored_long_side']} px "
            f"with no `WebP`). The round trip moves pixels by {s['webp']['mean']:.1f} levels on average on the worst "
            f"photo (max {s['webp']['max']:.0f}), and BioCLIP's embedding to a cosine similarity of "
            f"{e['webp']['photos']:.3f} on whole photos and {e['webp']['crops']:.3f} on a crop of "
            f"{worst['rect'][2]} × {worst['rect'][3]} px ({worst['id']}), which BioCLIP's recipe scales up to 224 px. It is almost the whole gap between what the heads were trained on and what the worker "
            "sends, and it needs its own decision (train on round-tripped photos, or store at a higher quality); J1 "
            "doesn't change it.",
            "",
            "| BioCLIP embeddings, lowest cosine similarity | Photos | Crops |",
            "|---|---|---|",
            f"| Same stored pixels: the worker's code against `pp`'s | {e['same_pixels']['photos']:.4f} | {e['same_pixels']['crops']:.4f} |",
            f"| `WebP`: the stored photo against the photo before it, both through `pp` | {e['webp']['photos']:.4f} | {e['webp']['crops']:.4f} |",
            f"| Decode and resize: the API's against Pillow's, before `WebP`, both through `pp` | {e['decode_resize']['photos']:.4f} | {e['decode_resize']['crops']:.4f} |",
            f"| End to end: the worker against `ml/`'s training chain | {e['end_to_end']['photos']:.4f} | {e['end_to_end']['crops']:.4f} |",
            "",
            f"`{r['embeddings']['model']}` on the CPU, hash checked against `manifest.cpu.toml`.",
            "",
        ]
    lines += [
        "## Layers",
        "",
        "| Layer | Compared | Worst photo, max | Worst photo, mean | Values more than 1 level apart |",
        "|---|---|---|---|---|",
        f"| Decode | Pillow (`pp.load_rgb`) against `imaging::decode_upload` | {s['decode']['max']:.0f} levels | {f(s['decode']['mean'], 3)} | {s['decode']['share_over_1']:.2%} |",
        f"| Resize to {w['stored_long_side']} px | `pp.resize` against `imaging::resize_long_side`, same decoded pixels | {s['resize']['max']:.0f} levels | {f(s['resize']['mean'], 3)} | {s['resize']['share_over_1']:.2%} |",
        f"| The same, horizontal first | `pp.resize` against the diagnostic | {s['resize_hfirst']['max']:.0f} levels | {f(s['resize_hfirst']['mean'], 3)} | {s['resize_hfirst']['share_over_1']:.4%} |",
        f"| `WebP` | the stored photo against the resized one | {s['webp']['max']:.0f} levels | {f(s['webp']['mean'], 2)} | {s['webp']['share_over_1']:.1%} |",
        "",
        "| Tensor | Same pixels, max | Horizontal first, max | End to end, max | End to end, worst photo's mean | One level |",
        "|---|---|---|---|---|---|",
    ]
    for n in names:
        lines.append(f"| `{n}` ({r['recipes'][n]['side']} px, {r['recipes'][n]['resize']}) | {f(s[n]['same_pixels_max'])} | "
                     f"{f(s[n]['hfirst_max'])} | {f(s[n]['end_to_end_max'])} | {f(s[n]['end_to_end_mean'])} | {f(lvl[n])} |")
    lines.append(f"| Leaf crops, `bioclip` | {f(s['crops']['same_pixels_max'])} | {f(s['crops']['hfirst_max'])} | "
                 f"{f(s['crops']['end_to_end_max'])} | | {f(lvl['bioclip'])} |")
    lines += ["", "`pp`'s recipes equal the manifest's: " + ", ".join(
        f"`{n}` {'yes' if r['recipes'][n]['matches_pp'] else '**no**'}" for n in names) + ".", "",
        "## Photos", "",
        "| Id | Photo | Decoded | Stored | Decode max | Resize max | `WebP` mean | "
        + " | ".join(f"`{n}` same / horizontal first" for n in names) + " |",
        "|---|---|---|---|---|---|---|" + "---|" * len(names)]
    for p in r["photos"]:
        lines.append(f"| {p['id']} | `{p['photo']}` | {p['decoded'][0]}×{p['decoded'][1]} | {p['stored'][0]}×{p['stored'][1]} | "
                     f"{p['decode']['max']:.0f} | {p['resize']['max']:.0f} | {f(p['webp']['mean'], 2)} | "
                     + " | ".join(f"{f(p['recipes'][n]['same_pixels']['max'])} / {f(p['recipes'][n]['same_pixels_hfirst']['max'])}" for n in names)
                     + " |")
    lines += ["", "## Leaf crops", "",
              f"Boxes on the stored photo, grown by `crop_grow` {w['crop_grow']:g} on each side. They test the crop, not the detector.",
              "",
              "| Id | Photo | Box (x, y, w, h) | Worker's rectangle (x, y, w, h) | Same as `pp.crop_box` | Same pixels, max | Horizontal first, max | End to end, max |",
              "|---|---|---|---|---|---|---|---|"]
    for c in r["crops"]:
        lines.append(f"| {c['id']} | {c['photo']} | {c['label']}: {', '.join(f'{v:g}' for v in c['bbox'])} | "
                     f"{', '.join(str(v) for v in c['rect'])} | {'yes' if c['same_rect'] else 'no'} | "
                     f"{f(c['same_pixels']['max'])} | {f(c['same_pixels_hfirst']['max'])} | {f(c['end_to_end']['max'])} |")
    lines += ["", "## Notes", "",
              "- Every photo is a dev photo; the test split isn't used.",
              "- The photos carry no EXIF orientation (the evaluation set strips it on import), so the decoders' rotation isn't exercised.",
              "- The upload's face blur isn't applied. It changes the same pixels on both sides of the comparison.",
              "- A box of zero area, which the detector never keeps, is cut as an empty image by the worker and as 1 px by `pp.crop_box`.",
              f"- The {r['target']} target allows one level. Even horizontal first, a few values ({hfirst_share:.0e} of a tensor at most) differ by two.",
              ""]
    return "\n".join(lines)


def rebuild(args) -> int:
    path = Path(args.report)
    path.with_suffix(".md").write_text(report_md(json.loads(path.read_text())))
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--photos", type=int, default=10)
    r.add_argument("--no-embed", action="store_true")
    r.set_defaults(fn=run)
    b = sub.add_parser("report")
    b.add_argument("report")
    b.set_defaults(fn=rebuild)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
