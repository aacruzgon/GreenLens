"""The species reference photos (contracts §10; storage 2): each seed image as the WebP that
RustFS holds, and its content-hashed key. build_migration.py writes the keys into the migration;
upload.py builds the same bytes in the seed-upload image and puts them in RustFS.

Only Pillow and PyYAML (through seed.py) are needed, so the image doesn't carry numpy or the
embedder. The same Pillow gives the same bytes; ml/knowledge/Dockerfile pins the version that
ml/requirements.lock pins.
"""

from __future__ import annotations

import hashlib
import io
import re

from ml.knowledge.seed import Seed

# A storage_key in the migration's species rows (build_migration.py writes them with value()).
STORAGE_KEY = re.compile(r"storage_key: '(knowledge/species/[^']*)'")


def reference_image(seed: Seed, species_id: str, img: dict) -> tuple[str, bytes]:
    """The photo as WebP (upright, long side 1024, no metadata) and its key,
    knowledge/species/{species_id}/{first 16 hex of the WebP's SHA-256}.webp. The EXIF orientation
    is applied before the metadata goes, as for uploads (contracts §1)."""
    from PIL import Image, ImageOps

    with Image.open(seed.root / "images" / img["file"]) as im:
        im = ImageOps.exif_transpose(im).convert("RGB")
        if max(im.size) > 1024:
            im.thumbnail((1024, 1024), Image.Resampling.LANCZOS)
        buf = io.BytesIO()
        im.save(buf, "WEBP", quality=85, method=6)
    data = buf.getvalue()
    return f"knowledge/species/{species_id}/{hashlib.sha256(data).hexdigest()[:16]}.webp", data


def reference_images(seed: Seed) -> dict[str, bytes]:
    """Every species' photos, {storage key: WebP bytes}."""
    photos: dict[str, bytes] = {}
    for s in seed.species:
        for img in s.get("reference_images") or []:
            key, data = reference_image(seed, s["id"], img)
            photos[key] = data
    return photos


def keys_in_migration(text: str) -> set[str]:
    """The photo keys a knowledge.surql refers to."""
    return set(STORAGE_KEY.findall(text))
