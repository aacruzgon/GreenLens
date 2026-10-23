"""The text embedder's golden vectors (docs/vision/models.md §10, dev-roadmap M8).

    python -m ml.tests.make_text_golden

Writes ml/tests/fixtures/text_embedder_golden.json. For each case it records the model's input
(a query carries QUERY_PREFIX), its token count after truncation at 512, and the first 8 values
of its vector from the pinned files ($MODEL_MANIFEST, default ~/.cache/pdm-models/manifest.toml).
test_knowledge.py checks them in Python; the Rust embedder (vision::embed) checks the same values.

The first 5 cases were recorded on 2026-10-18. The script stops unless it reproduces them, so a
rerun can't silently move the reference. The other 15 cover what the Rust tokenizer and
pipeline must do the same way: case folding, accents, symbols, emoji, whitespace, a real seed
chunk and a text over 512 tokens that both sides truncate.
"""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np

from ml.knowledge import embed

OUT = Path(__file__).resolve().parent / "fixtures/text_embedder_golden.json"
MAX_TOKENS = 512

# A 60-day plant journal of 1,022 tokens: over MAX_TOKENS, so both sides must truncate it the same way.
LONG = " ".join(f"Day {d}: no new spots on the lower leaves, soil still damp after watering."
                for d in range(1, 61))

CASES: list[tuple[str, str]] = [
    # The 5 of 2026-10-18, unchanged.
    ("query", "Why are my tomato leaves turning yellow?"),
    ("query", "brown spots with rings on lower leaves"),
    ("document", "Early blight starts as small brown spots on the lower leaves. The spots grow and form "
                 "rings like a target."),
    ("document", "Water at the base of the plant, early in the day, so the leaves stay dry."),
    ("document", "Ladybug larvae are long, dark grey, spiky, with orange spots, and eat aphids."),
    # Queries, as chat and search send them.
    ("query", "What's eating holes in my kale?"),                         # an apostrophe
    ("query", "WHY IS MY BASIL WILTING"),                                 # capitals: the tokenizer lowercases
    ("query", "powdery mildew squash"),                                   # keywords only
    ("query", "Is it safe to spray neem oil at 30 °C?"),                  # a degree sign and a number
    ("query", "Pourquoi les feuilles de mes tomates sont-elles tachées ?"),  # French: accents are stripped
    ("query", "tiny white bugs 🐛 under pepper leaves"),                   # an emoji: [UNK]
    ("query", "Can I pick grapes 14 days after a copper spray?"),         # digits
    # Documents: knowledge chunks and memories, with no prefix.
    ("document", "Two-spotted spider mites (Tetranychus urticae) feed on the undersides of leaves and "
                 "leave fine webbing."),                                  # a Latin name in parentheses
    ("document", "The user's garden is in Ottawa, zone 5a; the last frost is around May 10."),  # a memory
    ("document", "Spread 2–3 cm of mulch and keep the soil pH between 6.0 and 6.8."),  # an en dash, decimals
    ("document", "Symptoms:\n- yellow halos around the spots\n- leaves drop\tafter heavy rain"),  # newline, tab
    ("document", "Ladybug"),                                               # one word
    ("document", "  water   deeply,  then   wait  two days  "),             # runs of spaces
    # A real chunk of the knowledge seed, verbatim (isu_septoria_leaf_spot.3): inch marks, and a
    # sentence with no space after its full stop.
    ("document", "Symptoms are evident in the foliage, particularly older (lower) leaves. Septoria blight "
                 "causes numerous small (about 1/8\" to 1/4\" in diameter) brown spots that develop light "
                 "tan to white center as they age. Rapid defoliation occurs soon after if left untreated.The "
                 "overall effect of both blights is similar. Leaves turn yellow, brown, then wither and die."),
    ("document", LONG),
]

ABOUT = ("The text embedder's golden test (docs/vision/models.md §10): the model's input text (queries "
         "already carry the query prefix), its token count after truncation at 512, and the first 8 "
         "values of its vector from snowflake-arctic-embed-xs@d8c86521, recorded by "
         "ml/tests/make_text_golden.py on 2026-10-22 (the first 5 cases on 2026-10-18).")


def model_input(mode: str, text: str) -> str:
    return embed.QUERY_PREFIX + text if mode == "query" else text


def main() -> None:
    e = embed.Embedder()
    inputs = [model_input(m, t) for m, t in CASES]
    vectors = e.embed_documents(inputs)
    cases = []
    for (mode, text), inp, v in zip(CASES, inputs, vectors):
        enc = e.tokenizer.encode(inp)
        tokens = int(sum(enc.attention_mask))
        truncated = tokens == MAX_TOKENS and len(enc.overflowing) > 0
        assert truncated == (text is LONG), (text[:40], tokens)   # only LONG reaches the limit
        cases.append({"mode": mode, "text": text, "input": inp, "tokens": tokens,
                      "first_8": [round(float(x), 6) for x in v[:8]]})

    if OUT.exists():
        old = json.loads(OUT.read_text())["cases"][:5]
        for o, n in zip(old, cases):
            assert o["input"] == n["input"], o["input"]
            assert np.allclose(o["first_8"], n["first_8"], atol=1e-6), (o["input"], o["first_8"], n["first_8"])

    doc = {"about": ABOUT, "model": e.name, "cases": cases}
    OUT.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{OUT}: {len(cases)} cases, {sum(c['mode'] == 'query' for c in cases)} queries")


if __name__ == "__main__":
    main()
