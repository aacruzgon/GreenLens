"""`training.heads.train_disease`: which grid point the head keeps."""

from ml.training.heads.train_disease import c_without_each_dataset, select_c


def grid(*rows):
    return [{"C": c, "val_top1_per_photo": v, "val_top1_per_crop": v, "dev_top3_sent": d, "dev_with_problem": 94}
            for c, v, d in rows]


def test_dev_top3_keeps_the_best_val_score_among_cs_tied_on_dev():
    # v3's grid: C=0.1 leads on dev by one photo of 94, which is noise; C=5 is the best on val within one SE.
    g = grid((0.1, 0.792, 80 / 94), (0.3, 0.829, 79 / 94), (1, 0.851, 79 / 94), (5, 0.863, 78 / 94), (20, 0.867, 73 / 94))
    assert g[select_c(g, "dev-top3")]["C"] == 5
    assert g[select_c(g, "crop")]["C"] == 20 and g[select_c(g, "photo")]["C"] == 20


def test_a_clear_lead_on_dev_wins_over_val():
    g = grid((1, 0.80, 85 / 94), (20, 0.90, 70 / 94))
    assert g[select_c(g, "dev-top3")]["C"] == 1


def test_leaving_a_dataset_out_of_val_can_move_c():
    # Dataset "a" alone prefers C=20; without it, "b" decides and C=1 wins. Both are tied on dev.
    g = [{"C": c, "val_top1_per_crop": (a * 100 + b * 100) / 200, "dev_top3_sent": 0.8, "dev_with_problem": 83,
          "val_by_dataset": {"a": {"top1": a, "n": 100}, "b": {"top1": b, "n": 100}}}
         for c, a, b in ((1, 0.70, 0.90), (20, 0.99, 0.85))]
    assert g[select_c(g, "dev-top3")]["C"] == 20
    assert c_without_each_dataset(g, "dev-top3") == {"a": 1, "b": 20}
    assert c_without_each_dataset(g, "photo") == {}
