"""Train the leaf detector (RF-DETR-S, one class `leaf`) on ml/training/data/cache/leaf-coco."""
import argparse
import datetime
from pathlib import Path

from rfdetr import RFDETRSmall

ROOT = Path(__file__).resolve().parents[1]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--epochs", type=int, default=50)
    ap.add_argument("--batch-size", default="auto", help="int or 'auto'")
    ap.add_argument("--grad-accum-steps", type=int, default=4)
    ap.add_argument("--lr", type=float, default=1e-4)
    ap.add_argument("--resolution", type=int, default=512)
    ap.add_argument("--dataset-dir", default=str(ROOT / "data/cache/leaf-coco"))
    ap.add_argument("--output-dir", default=str(ROOT / f"runs/leaf-rfdetr-s-{datetime.date.today():%Y%m%d}"))
    a = ap.parse_args()
    bs = a.batch_size if a.batch_size == "auto" else int(a.batch_size)

    model = RFDETRSmall()
    model.train(
        dataset_dir=a.dataset_dir,
        epochs=a.epochs,
        batch_size=bs,
        grad_accum_steps=a.grad_accum_steps,
        lr=a.lr,
        resolution=a.resolution,
        output_dir=a.output_dir,
        class_names=["leaf"],
    )


if __name__ == "__main__":
    main()
