#!/usr/bin/env python3
"""Builds `crates/integrations/data/slc_v3r2.bin`, the Soil Landscapes of Canada lookup.

Source: Agriculture and Agri-Food Canada, Soil Landscapes of Canada v3.2, under the Open
Government Licence - Canada (https://open.canada.ca/en/open-government-licence-canada).
Dataset: https://open.canada.ca/data/en/dataset/5ad5e20c-f2bb-497d-a2a2-440eec6e10cd

Inputs (download into one directory):
  soil_landscapes_of_canada_v3r2.gpkg   the polygons (EPSG:4269, NAD83)
  ca_all_slc_v3r2_cmp.csv               the components of each polygon
  ca_all_slc_v3r2_slt.csv               the soil layers of each soil

    python3 -I backend/tools/slc/build_slc.py <input dir> backend/crates/integrations/data/slc_v3r2.bin

Standard library only. What it does:
  1. Each polygon's dominant component (largest share; the lowest component number on a tie).
  2. That soil's layers averaged onto the six SoilGrids depth bands, weighted by overlap; a band
     less than half covered by layers with a value stays empty. Surface organic layers (negative
     depths) are left out. pH in water (PH2), sand, silt and clay (%), organic carbon (% x 10 =
     g/kg), CEC (cmol/kg) and bulk density (g/cm3 = kg/dm3). SLC has no nitrogen.
  3. The polygons rasterised on a 0.01 degree grid (about 1.1 km by 0.8 km in southern Ontario,
     inside the map's 1:1,000,000 accuracy), each cell holding its centre's soil, stored as runs.

NAD83 and WGS84 differ by about a metre here, far below the grid, so no datum shift is applied.

File layout, little-endian:
  b"SLC1"
  u32 version (1), i32 lat0 and lon0 in 1e-4 degrees, u32 step in 1e-4 degrees, u32 rows,
  u32 cols, u32 soils, u32 runs
  soils x 6 bands x 8 values, u16 each: value x 100, 0xFFFF where empty. The values in order:
    ph_h2o, clay_pct, sand_pct, silt_pct, organic_carbon_g_kg, nitrogen_g_kg (always empty),
    cec_cmol_kg, bulk_density_kg_dm3
  rows + 1 x u32: the index of each row's first run (row 0 is the southernmost)
  runs x (u16 first column, u16 length, u16 soil)
"""

import csv
import math
import sqlite3
import struct
import sys
from collections import defaultdict

STEP_E4 = 100  # 0.01 degree
BANDS = [(0, 5), (5, 15), (15, 30), (30, 60), (60, 100), (100, 200)]
FIELDS = ["PH2", "TCLAY", "TSAND", "TSILT", "ORGCARB", None, "CEC", "BD"]
SCALE = {"ORGCARB": 10.0}  # % -> g/kg
EMPTY = 0xFFFF
ENVELOPE = {0: 0, 1: 32, 2: 48, 3: 48, 4: 64}


def wkb_polygons(blob):
    """GeoPackage geometry -> list of polygons, each a list of rings of (lon, lat)."""
    flags = blob[3]
    wkb = blob[8 + ENVELOPE[(flags >> 1) & 7]:]
    off = 0

    def u32(bo):
        nonlocal off
        v = struct.unpack(bo + "I", wkb[off:off + 4])[0]
        off += 4
        return v

    def geom():
        nonlocal off
        bo = "<" if wkb[off] == 1 else ">"
        off += 1
        kind = u32(bo) % 1000
        if kind == 3:
            poly = []
            for _ in range(u32(bo)):
                n = u32(bo)
                pts = struct.unpack(bo + f"{2 * n}d", wkb[off:off + 16 * n])
                off += 16 * n
                poly.append(list(zip(pts[0::2], pts[1::2])))
            return [poly]
        if kind == 6:
            out = []
            for _ in range(u32(bo)):
                out += geom()
            return out
        raise ValueError(f"geometry type {kind}")

    return geom()


def number(raw):
    try:
        v = float(raw)
    except ValueError:
        return None
    return None if v < 0 else v


def band_values(layers):
    """A soil's layers -> 6 bands x 8 values (None where empty)."""
    out = []
    for top, bottom in BANDS:
        row = []
        for field in FIELDS:
            if field is None:
                row.append(None)
                continue
            covered = total = 0.0
            for layer in layers:
                u, lo = number(layer["UDEPTH"]), number(layer["LDEPTH"])
                v = number(layer[field])
                if u is None or lo is None or v is None or lo <= u:
                    continue
                overlap = min(bottom, lo) - max(top, u)
                if overlap > 0:
                    covered += overlap
                    total += overlap * v * SCALE.get(field, 1.0)
            row.append(total / covered if covered >= (bottom - top) / 2 else None)
        out.append(row)
    return out


def main(src, dst):
    components = defaultdict(list)
    with open(f"{src}/ca_all_slc_v3r2_cmp.csv", encoding="latin-1") as f:
        for r in csv.DictReader(f):
            components[int(r["POLY_ID"])].append(r)
    layers = defaultdict(list)
    with open(f"{src}/ca_all_slc_v3r2_slt.csv", encoding="latin-1") as f:
        for r in csv.DictReader(f):
            layers[r["SOIL_ID"]].append(r)

    soils, soil_index, poly_soil = [], {}, {}
    for poly_id, cmps in components.items():
        best = min(cmps, key=lambda c: (-float(c["PERCENT_"]), int(c["CMP"])))
        soil_id = best["SOIL_ID"]
        if soil_id not in soil_index:
            bands = band_values(layers.get(soil_id, []))
            if all(v is None for row in bands for v in row):
                soil_index[soil_id] = None
            else:
                soil_index[soil_id] = len(soils)
                soils.append(bands)
        if soil_index[soil_id] is not None:
            poly_soil[poly_id] = soil_index[soil_id]

    db = sqlite3.connect(f"{src}/soil_landscapes_of_canada_v3r2.gpkg")
    shapes = [
        (poly_soil[pid], wkb_polygons(blob))
        for pid, blob in db.execute("select POLY_ID, Shape from ca_all_slc_v3r2")
        if pid in poly_soil
    ]
    step = STEP_E4 / 1e4
    lats = [y for _, ps in shapes for p in ps for x, y in p[0]]
    lons = [x for _, ps in shapes for p in ps for x, y in p[0]]
    lat0 = math.floor(min(lats) / step) * step
    lon0 = math.floor(min(lons) / step) * step
    rows = math.ceil((max(lats) - lat0) / step) + 1
    cols = math.ceil((max(lons) - lon0) / step) + 1
    assert cols < 0xFFFF and len(soils) < 0xFFFF

    # Scanline fill at each row's centre: the crossings of every edge, per (row, polygon).
    spans = defaultdict(list)  # row -> [(first col, last col, soil)]
    for soil, polys in shapes:
        for poly in polys:
            crossings = defaultdict(list)
            for ring in poly:
                for (x1, y1), (x2, y2) in zip(ring, ring[1:] + ring[:1]):
                    if y1 == y2:
                        continue
                    lo, hi = min(y1, y2), max(y1, y2)
                    r0 = math.ceil((lo - lat0) / step - 0.5)
                    r1 = math.ceil((hi - lat0) / step - 0.5) - 1
                    for r in range(max(r0, 0), min(r1, rows - 1) + 1):
                        yc = lat0 + (r + 0.5) * step
                        crossings[r].append(x1 + (yc - y1) * (x2 - x1) / (y2 - y1))
            for r, xs in crossings.items():
                xs.sort()
                for a, b in zip(xs[0::2], xs[1::2]):
                    c0 = math.ceil((a - lon0) / step - 0.5)
                    c1 = math.ceil((b - lon0) / step - 0.5) - 1
                    if c1 >= c0:
                        spans[r].append((max(c0, 0), min(c1, cols - 1), soil))

    runs, starts = [], []
    for r in range(rows):
        starts.append(len(runs))
        row = sorted(spans.get(r, []))
        merged = []
        for c0, c1, soil in row:
            if merged and merged[-1][2] == soil and merged[-1][1] >= c0 - 1:
                merged[-1][1] = max(merged[-1][1], c1)
            elif merged and merged[-1][1] >= c0:
                c0 = merged[-1][1] + 1  # overlapping slivers: the first polygon keeps the cell
                if c0 <= c1:
                    merged.append([c0, c1, soil])
            else:
                merged.append([c0, c1, soil])
        runs += [(c0, c1 - c0 + 1, soil) for c0, c1, soil in merged]
    starts.append(len(runs))

    with open(dst, "wb") as f:
        f.write(b"SLC1")
        f.write(struct.pack("<IiiIIIII", 1, round(lat0 * 1e4), round(lon0 * 1e4), STEP_E4,
                            rows, cols, len(soils), len(runs)))
        for bands in soils:
            for row in bands:
                f.write(struct.pack("<8H", *(EMPTY if v is None else round(v * 100) for v in row)))
        f.write(struct.pack(f"<{len(starts)}I", *starts))
        for run in runs:
            f.write(struct.pack("<3H", *run))
    print(f"{len(poly_soil)} polygons, {len(soils)} soils, {rows} x {cols} cells, "
          f"{len(runs)} runs -> {dst}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
