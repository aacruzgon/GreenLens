"""`training.heads.train_disease`: which grid point the head keeps."""

from ml.training.heads.train_disease import select_c


def grid(*rows):
    return [{"C": c, "val_top1_per_photo": v, "val_top1_per_crop": v, "dev_top3": d, "dev_n": 94} for c, v, d in rows]


def test_dev_top3_keeps_the_best_val_score_among_cs_tied_on_dev():
    # v3's grid: C=0.1 leads on dev by one photo of 94, which is noise; C=5 is the best on val within one SE.
    g = grid((0.1, 0.792, 80 / 94), (0.3, 0.829, 79 / 94), (1, 0.851, 79 / 94), (5, 0.863, 78 / 94), (20, 0.867, 73 / 94))
    assert g[select_c(g, "dev-top3")]["C"] == 5
    assert g[select_c(g, "crop")]["C"] == 20 and g[select_c(g, "photo")]["C"] == 20


def test_a_clear_lead_on_dev_wins_over_val():
    g = grid((1, 0.80, 85 / 94), (20, 0.90, 70 / 94))
    assert g[select_c(g, "dev-top3")]["C"] == 1
