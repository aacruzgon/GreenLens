"""The iNaturalist species index (review of #85)."""

from ml.training.data import inat_species


def test_the_wild_chiltepin_is_left_out_of_pepper_only():
    row = lambda slug, taxon: {"slug": slug, "taxon": taxon}
    assert inat_species.excluded(row("pepper", "Capsicum annuum glabriusculum"))
    assert not inat_species.excluded(row("pepper", "Capsicum annuum"))
    assert not inat_species.excluded(row("pepper", "Capsicum annuum annuum"))
    assert not inat_species.excluded(row("tomato", "Capsicum annuum glabriusculum"))


def test_the_species_index_has_no_excluded_row():
    lines = inat_species.INDEX.read_text().splitlines()
    rows = [dict(zip(inat_species.COLUMNS, line.split("\t"))) for line in lines[1:]]
    assert rows and not any(inat_species.excluded(r) for r in rows)
