"""The perception card's checks: ONNX Runtime on CUDA (evaluation 7) and the batch caps that fit
(evaluation 8, "Perception batch size"; dev-roadmap J3).

    python -m ml.training.eval.perception_gpu check  [--models DIR]
    python -m ml.training.eval.perception_gpu budget [--models DIR] [--caps det=8,enc=2,dec=16,clip=16 ...]
    python -m ml.training.eval.perception_gpu all    [--models DIR]

It needs only numpy, onnx and onnxruntime-gpu 1.31 with the CUDA 13 wheels the training
environment uses (the `[cuda,cudnn]` extra would pull newer CUDA 13.4 ones), so it runs on a host
without that environment. `uv` needs no sudo; `python3 -m venv` needs Debian's `python3-venv`:

    uv venv -p 3.13 ~/pdm-gpu && uv pip install -p ~/pdm-gpu/bin/python onnxruntime-gpu==1.31.0 \\
        nvidia-cuda-runtime==13.0.96 nvidia-cublas==13.1.1.3 nvidia-cudnn-cu13==9.24.0.43 \\
        nvidia-cufft==12.0.0.61 nvidia-curand==10.4.0.35 nvidia-cuda-nvrtc==13.0.88 \\
        nvidia-nvjitlink==13.3.33 numpy onnx
    HF_TOKEN=... python3 -m ml.models.fetch --for perception --dest ~/pdm-models
    ~/pdm-gpu/bin/python -m ml.training.eval.perception_gpu all --models ~/pdm-models

`check` (evaluation 7, day 1):
- ONNX Runtime's CUDA provider loads on this card, and a session that asks for it gets it (the
  worker registers it with `error_on_failure`, so a CPU fallback would be a failed start).
- A small fp32 model with a dynamic batch, built here (convolution, pooling, matrix product), runs
  at batches 1, 4 and 16 and matches the CPU within 1e-4, with TF32 off so the comparison is exact
  arithmetic, not TF32's 10-bit mantissa.
- Each perception model in the manifest opens on CUDA and runs a batch of 1.

`budget` (evaluation 8): for each cap set, a fresh process loads the four perception sessions with
the worker's settings (vision `runtime.rs`: arena `kSameAsRequested`, cuDNN `HEURISTIC` search, the
manifest's `gpu_mem_limit_mb` per session unless `--no-limits`), warms each at its cap, then runs a
scan of 6 shots and 40 leaf crops: the detector on the 6 shots, the SAM encoder on the 6, the SAM
decoder with all 40 boxes on one image (the worst case), and BioCLIP 2 on the 40 crops and the 6
shots, each in batches of its cap. It reports the process's peak VRAM, the card's free memory at
that peak and the median time per scan. A cap set passes when at least 1 GB of the card stays free.
The worker's own run (P2) confirms the caps chosen here.

Each check runs in its own process, so the parent never holds a CUDA context that a later
measurement would count as another process's memory. The exit code is 0 when the check passed
(if it ran) and every cap set named by `--must-pass` passed with the manifest's limits (default:
the manifest's caps; when none of the named sets ran, every set that did).

Writes `perception-gpu-<host>-<date>.json` in the current directory and prints a summary.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import socket
import statistics
import subprocess
import sys
import threading
import time
import tomllib
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "ml/models/manifest.toml"
MODELS = ("leaf_detector", "sam_encoder", "sam_decoder", "bioclip")
KEY = {"det": "leaf_detector", "enc": "sam_encoder", "dec": "sam_decoder", "clip": "bioclip"}
SCAN_SHOTS, SCAN_CROPS = 6, 40
FREE_AT_PEAK_MIB = 1024          # evaluation 8: at least 1 GB of the card free at peak
DEFAULT_CAPS = ["det=4,enc=1,dec=8,clip=8", "manifest", "det=16,enc=4,dec=32,clip=32"]


def ort():
    import onnxruntime

    onnxruntime.preload_dlls()   # the CUDA and cuDNN libraries from the NVIDIA pip wheels
    return onnxruntime


def entries(models: Path) -> dict[str, dict]:
    manifest = models / "manifest.toml" if (models / "manifest.toml").exists() else MANIFEST
    by_name = {e["name"]: e for e in tomllib.loads(manifest.read_text())["model"]}
    return {n: by_name[n] for n in MODELS}


def gpu_query() -> dict:
    out = subprocess.run(["nvidia-smi", "--query-gpu=name,driver_version,uuid,memory.used,memory.total,compute_cap",
                          "--format=csv,noheader,nounits"], capture_output=True, text=True, check=True).stdout
    name, driver, uuid, used, total, cc = [x.strip() for x in out.splitlines()[0].split(",")]
    return {"name": name, "driver": driver, "uuid": uuid, "used_mib": int(used), "total_mib": int(total), "compute_cap": cc}


def process_mib(pid: int) -> int:
    out = subprocess.run(["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True).stdout
    return sum(int(m) for p, m in (l.split(",") for l in out.splitlines() if l.strip()) if int(p) == pid)


def cuda_options(limit_mb: int | None, tf32: bool = True) -> dict:
    """The worker's CUDA provider settings (backend vision `runtime.rs`)."""
    opts = {"device_id": 0, "arena_extend_strategy": "kSameAsRequested", "cudnn_conv_algo_search": "HEURISTIC",
            "use_tf32": "1" if tf32 else "0"}
    if limit_mb:
        opts["gpu_mem_limit"] = str(limit_mb * 1024 * 1024)
    return opts


def session(o, path: Path, opts: dict):
    so = o.SessionOptions()
    so.log_severity_level = 3
    s = o.InferenceSession(str(path), so, providers=[("CUDAExecutionProvider", opts)])
    if s.get_providers()[0] != "CUDAExecutionProvider":
        raise RuntimeError(f"{path.name}: the CUDA execution provider didn't load")
    return s


# --- check -------------------------------------------------------------------------------------

def small_model(path: Path) -> None:
    """A dynamic-batch fp32 model: conv 3→16, ReLU, global average pool, flatten, matrix product → 10."""
    import onnx
    from onnx import TensorProto, helper, numpy_helper

    rng = np.random.default_rng(0)
    w = numpy_helper.from_array(rng.normal(0, 0.1, (16, 3, 3, 3)).astype(np.float32), "w")
    b = numpy_helper.from_array(rng.normal(0, 0.1, 16).astype(np.float32), "b")
    m = numpy_helper.from_array(rng.normal(0, 0.1, (16, 10)).astype(np.float32), "m")
    nodes = [helper.make_node("Conv", ["x", "w", "b"], ["c"], pads=[1, 1, 1, 1]),
             helper.make_node("Relu", ["c"], ["r"]),
             helper.make_node("GlobalAveragePool", ["r"], ["p"]),
             helper.make_node("Flatten", ["p"], ["f"]),
             helper.make_node("MatMul", ["f", "m"], ["y"])]
    graph = helper.make_graph(nodes, "small", [helper.make_tensor_value_info("x", TensorProto.FLOAT, ["batch", 3, 64, 64])],
                              [helper.make_tensor_value_info("y", TensorProto.FLOAT, ["batch", 10])], [w, b, m])
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 9
    onnx.save(model, str(path))


def feeds(sess, batch: int, rng) -> dict:
    out = {}
    for i in sess.get_inputs():
        shape = [batch if not isinstance(d, int) else d for d in i.shape]
        if i.name == "boxes":
            xy = rng.uniform(0, 900, (shape[0], 2))
            out[i.name] = np.concatenate([xy, xy + rng.uniform(20, 120, (shape[0], 2))], 1).astype(np.float32)
        else:
            out[i.name] = rng.standard_normal(shape).astype(np.float32)
    return out


def check(models: Path, tmp: Path) -> dict:
    o = ort()
    result = {"onnxruntime": o.__version__, "providers": o.get_available_providers(), "gpu": gpu_query()}
    tiny = tmp / "small-dynamic-batch.onnx"
    small_model(tiny)
    cpu = o.InferenceSession(str(tiny), providers=["CPUExecutionProvider"])
    gpu = session(o, tiny, cuda_options(None, tf32=False))
    rng = np.random.default_rng(1)
    diffs = {}
    for batch in (1, 4, 16):
        x = rng.standard_normal((batch, 3, 64, 64)).astype(np.float32)
        diffs[batch] = float(np.abs(gpu.run(None, {"x": x})[0] - cpu.run(None, {"x": x})[0]).max())
    result["small_model_max_abs_diff"] = diffs
    result["small_model_pass"] = all(d <= 1e-4 for d in diffs.values())
    opened = {}
    for name, e in entries(models).items():
        t0 = time.perf_counter()
        try:
            s = session(o, models / e["file"], cuda_options(e.get("config", {}).get("gpu_mem_limit_mb")))
            s.run(None, feeds(s, 1, rng))
            opened[name] = {"ok": True, "seconds": round(time.perf_counter() - t0, 2)}
        except Exception as err:   # report every model, not only the first failure
            opened[name] = {"ok": False, "error": str(err)[:400]}
    result["models_open_on_cuda"] = opened
    result["pass"] = result["small_model_pass"] and all(v["ok"] for v in opened.values())
    return result


# --- budget ------------------------------------------------------------------------------------

def parse_caps(spec: str, es: dict) -> dict[str, int]:
    caps = {"leaf_detector": es["leaf_detector"]["config"]["max_batch"],
            "sam_encoder": es["sam_encoder"]["config"]["max_batch"],
            "sam_decoder": es["sam_decoder"]["config"]["max_boxes"],
            "bioclip": es["bioclip"]["config"]["max_batch"]}
    if spec != "manifest":
        for part in spec.split(","):
            k, v = part.split("=")
            caps[KEY[k.strip()]] = int(v)
    return caps


def chunks(n: int, cap: int) -> list[int]:
    return [min(cap, n - i) for i in range(0, n, cap)]


def budget_child(models: Path, caps: dict[str, int], limits: bool) -> dict:
    """Runs in its own process, so every cap set starts with empty arenas."""
    o = ort()
    es = entries(models)
    pid = os.getpid()
    peak = {"mib": 0}
    stop = threading.Event()

    def poll():
        while not stop.is_set():
            peak["mib"] = max(peak["mib"], process_mib(pid))
            stop.wait(0.05)

    card_before = gpu_query()
    threading.Thread(target=poll, daemon=True).start()
    rng = np.random.default_rng(2)
    t0 = time.perf_counter()
    sess = {n: session(o, models / e["file"], cuda_options(e.get("config", {}).get("gpu_mem_limit_mb") if limits else None))
            for n, e in es.items()}
    loaded_s = time.perf_counter() - t0
    after_load = process_mib(pid)
    enc_out = None
    for n, s in sess.items():   # the worker's warm-up: one run at each session's cap
        if n == "sam_decoder":
            f = {k: v[:1] for k, v in zip(("high_res_feats_0", "high_res_feats_1", "image_embed"), enc_out)}
            f["boxes"] = feeds(s, caps[n], rng)["boxes"]
            s.run(None, f)
        else:
            out = s.run(None, feeds(s, caps[n], rng))
            if n == "sam_encoder":
                enc_out = out
    after_warmup = process_mib(pid)

    def scan():
        for b in chunks(SCAN_SHOTS, caps["leaf_detector"]):
            sess["leaf_detector"].run(None, feeds(sess["leaf_detector"], b, rng))
        feats = None
        for b in chunks(SCAN_SHOTS, caps["sam_encoder"]):
            feats = sess["sam_encoder"].run(None, feeds(sess["sam_encoder"], b, rng))
        one = {k: v[:1] for k, v in zip(("high_res_feats_0", "high_res_feats_1", "image_embed"), feats)}
        for b in chunks(SCAN_CROPS, caps["sam_decoder"]):
            sess["sam_decoder"].run(None, {**one, "boxes": feeds(sess["sam_decoder"], b, rng)["boxes"]})
        for b in chunks(SCAN_CROPS + SCAN_SHOTS, caps["bioclip"]):
            sess["bioclip"].run(None, feeds(sess["bioclip"], b, rng))

    times = []
    for _ in range(4):
        t = time.perf_counter()
        scan()
        times.append(time.perf_counter() - t)
    time.sleep(0.2)
    stop.set()
    card = gpu_query()
    others = card_before["used_mib"]   # what other processes held before this one started
    free_at_peak = card["total_mib"] - others - peak["mib"]
    return {"caps": caps, "limits": limits, "load_seconds": round(loaded_s, 1), "mib_after_load": after_load,
            "mib_after_warmup": after_warmup, "peak_mib": peak["mib"], "others_mib": others,
            "free_at_peak_mib": free_at_peak, "pass": free_at_peak >= FREE_AT_PEAK_MIB,
            "scan_seconds_median": round(statistics.median(times[1:]), 3)}


def budget(models: Path, specs: list[str], limits: bool) -> list[dict]:
    es = entries(models)
    out = []
    for spec in specs:
        caps = parse_caps(spec, es)
        r = run_child(["_child", "--models", str(models), "--caps-json", json.dumps(caps)]
                      + (["--no-limits"] if not limits else []), {"caps": caps, "limits": limits})
        r["spec"] = spec
        out.append(r)
        print(summary_line(r), flush=True)
    return out


def summary_line(r: dict) -> str:
    c = r["caps"]
    head = f"det {c['leaf_detector']:>2} enc {c['sam_encoder']} dec {c['sam_decoder']:>2} clip {c['bioclip']:>2} " \
           f"{'limits' if r['limits'] else 'no limits'}: "
    if "error" in r:
        return head + "FAILED " + r["error"].strip().splitlines()[-1][:200]
    return head + (f"peak {r['peak_mib']} MiB (loaded {r['mib_after_load']}, warmed {r['mib_after_warmup']}), "
                   f"free at peak {r['free_at_peak_mib']} MiB, scan {r['scan_seconds_median']} s "
                   f"{'PASS' if r['pass'] else 'FAIL'}")


def run_child(args: list[str], fallback: dict) -> dict:
    """Runs this module in a fresh process and returns the JSON line it prints last."""
    p = subprocess.run([sys.executable, "-m", "ml.training.eval.perception_gpu", *args],
                       capture_output=True, text=True, cwd=ROOT)
    line = next((l for l in reversed(p.stdout.splitlines()) if l.startswith("{")), None)
    if p.returncode == 0 and line:
        return json.loads(line)
    return {**fallback, "pass": False, "error": (p.stderr or p.stdout)[-600:]}


def verdict(report: dict, must_pass: list[str]) -> bool:
    """The check, if it ran, and the cap sets that must pass, if a budget ran."""
    ok = report["check"].get("pass", False) if "check" in report else True
    if "budget" in report:
        judged = [r for r in report["budget"] if r["spec"] in must_pass] or report["budget"]
        ok = ok and all(r.get("pass") for r in judged)
    return ok


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("what", choices=("check", "budget", "all", "_child", "_check"))
    ap.add_argument("--models", type=Path, default=ROOT / "ml/models/files",
                    help="the directory `ml.models.fetch --for perception` wrote")
    ap.add_argument("--caps", nargs="*", default=DEFAULT_CAPS,
                    help="cap sets, e.g. det=8,enc=2,dec=16,clip=16, or 'manifest'")
    ap.add_argument("--no-limits", action="store_true", help="no gpu_mem_limit per session: what each set needs")
    ap.add_argument("--must-pass", nargs="*", default=["manifest"],
                    help="cap sets that must pass for exit code 0 (default: manifest)")
    ap.add_argument("--caps-json", help=argparse.SUPPRESS)
    ap.add_argument("--out", type=Path)
    args = ap.parse_args(argv)
    models = args.models.expanduser().resolve()

    if args.what == "_child":
        print(json.dumps(budget_child(models, json.loads(args.caps_json), not args.no_limits)))
        return 0
    if args.what == "_check":
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            print(json.dumps(check(models, Path(tmp))))
        return 0

    report = {"host": socket.gethostname(), "date": dt.date.today().isoformat()}
    if args.what in ("check", "all"):
        report["check"] = c = run_child(["_check", "--models", str(models)], {})
        if "error" in c:
            print("check FAILED: " + c["error"].strip().splitlines()[-1][:300])
    if "gpu" in report.get("check", {}):
        print(f"{c['gpu']['name']} (compute {c['gpu']['compute_cap']}), driver {c['gpu']['driver']}, "
              f"ONNX Runtime {c['onnxruntime']}, uuid {c['gpu']['uuid']}")
        print(f"small fp32 model, CUDA vs CPU max abs diff: {c['small_model_max_abs_diff']} "
              f"{'PASS' if c['small_model_pass'] else 'FAIL'} (1e-4)")
        for n, v in c["models_open_on_cuda"].items():
            print(f"  {n:14s} {'opens and runs on CUDA' if v['ok'] else 'FAILED: ' + v['error']}")
    if args.what in ("budget", "all"):
        print("budget: every session loaded, warmed at its cap, then a scan of "
              f"{SCAN_SHOTS} shots and {SCAN_CROPS} crops (needs {FREE_AT_PEAK_MIB} MiB free at peak)")
        report["budget"] = budget(models, args.caps, not args.no_limits)
        judged = [r["spec"] for r in report["budget"] if r["spec"] in args.must_pass] or args.caps
        print(f"must pass: {', '.join(judged)}")
        if not args.no_limits:   # what each set needs without the manifest's limits
            report["budget_no_limits"] = budget(models, args.caps, False)
    out = args.out or Path(f"perception-gpu-{report['host']}-{report['date']}.json")
    out.write_text(json.dumps(report, indent=2) + "\n")
    print(f"wrote {out}")
    ok = verdict(report, args.must_pass)
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
