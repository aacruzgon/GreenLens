"""Single members of a remote zip, read over HTTP Range requests (fetch.py's `zip_members` kind).

A big archive often holds far more than the classes we train on (PlantCity's 6.1 GB train.zip
has 53 classes; we need 10). This reads the zip's central directory from its tail, then only the
byte ranges of the chosen members, and checks each member's CRC-32 as it is inflated. It handles
zip64 archives and the stored and deflated methods; Mendeley, Zenodo and Figshare serve ranges.
"""

from __future__ import annotations

import struct
import urllib.request
import zlib
from dataclasses import dataclass

UA = "curl/8.9"   # Mendeley's API refuses Python's default User-Agent
TIMEOUT = 120
TAIL = 1 << 17    # the end of central directory record, its zip64 locator and any comment


@dataclass(frozen=True)
class Member:
    name: str
    method: int        # 0 stored, 8 deflated
    crc: int
    csize: int
    usize: int
    offset: int        # of the local header
    end: int           # where the next member (or the central directory) starts


def read_range(url: str, start: int, end: int) -> tuple[bytes, int]:
    """Bytes `start`..`end` inclusive, and the file's total size from Content-Range."""
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Range": f"bytes={start}-{end}"})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        if r.status != 206:
            raise OSError(f"{url}: no range support (HTTP {r.status})")
        data = r.read()
        total = int(r.headers["Content-Range"].rsplit("/", 1)[1])
    if len(data) != min(end, total - 1) - start + 1:
        raise OSError(f"{url}: short read at {start}")
    return data, total


def members(url: str) -> list[Member]:
    """Every file entry of the zip at `url` (directories left out), in central-directory order."""
    _, size = read_range(url, 0, 0)
    tail_start = max(0, size - TAIL)
    tail, _ = read_range(url, tail_start, size - 1)
    i = tail.rfind(b"PK\x05\x06")
    if i < 0:
        raise ValueError(f"{url}: no end of central directory record")
    total, cd_size, cd_off = struct.unpack("<HII", tail[i + 10 : i + 20])
    if 0xFFFFFFFF in (cd_size, cd_off) or total == 0xFFFF:
        loc = tail.rfind(b"PK\x06\x07", 0, i)
        (z64_off,) = struct.unpack("<Q", tail[loc + 8 : loc + 16])
        rec = tail[z64_off - tail_start :] if z64_off >= tail_start else read_range(url, z64_off, z64_off + 55)[0]
        if rec[:4] != b"PK\x06\x06":
            raise ValueError(f"{url}: bad zip64 end of central directory record")
        cd_size, cd_off = struct.unpack("<QQ", rec[40:56])
    cd = tail[cd_off - tail_start : cd_off - tail_start + cd_size] if cd_off >= tail_start else read_range(url, cd_off, cd_off + cd_size - 1)[0]
    out, p = [], 0
    while p + 46 <= len(cd) and cd[p : p + 4] == b"PK\x01\x02":
        flags, method = struct.unpack("<HH", cd[p + 8 : p + 12])
        crc, csize, usize = struct.unpack("<III", cd[p + 16 : p + 28])
        fl, el, cl = struct.unpack("<HHH", cd[p + 28 : p + 34])
        (off,) = struct.unpack("<I", cd[p + 42 : p + 46])
        raw_name = cd[p + 46 : p + 46 + fl]
        name = raw_name.decode("utf-8" if flags & 0x800 else "cp437")
        extra = cd[p + 46 + fl : p + 46 + fl + el]
        q = 0
        while q + 4 <= len(extra):
            hid, hl = struct.unpack("<HH", extra[q : q + 4])
            if hid == 0x0001:   # zip64: the fields that overflowed, in this order
                vals = struct.unpack(f"<{hl // 8}Q", extra[q + 4 : q + 4 + hl // 8 * 8])
                k = 0
                if usize == 0xFFFFFFFF:
                    usize, k = vals[k], k + 1
                if csize == 0xFFFFFFFF:
                    csize, k = vals[k], k + 1
                if off == 0xFFFFFFFF:
                    off = vals[k]
            q += 4 + hl
        out.append((name, method, crc, csize, usize, off))
        p += 46 + fl + el + cl
    starts = sorted({o for *_, o in out} | {cd_off})
    nxt = {a: b for a, b in zip(starts, starts[1:])}
    return [Member(n, m, c, cs, us, o, nxt[o]) for n, m, c, cs, us, o in out if not n.endswith("/")]


def member_bytes(m: Member, buf: bytes, buf_start: int) -> bytes:
    """The member's content from a buffer holding its local header and data; checks the CRC-32."""
    p = m.offset - buf_start
    if buf[p : p + 4] != b"PK\x03\x04":
        raise ValueError(f"{m.name}: no local header at {m.offset}")
    fl, el = struct.unpack("<HH", buf[p + 26 : p + 30])
    data = buf[p + 30 + fl + el : p + 30 + fl + el + m.csize]
    if m.method == 0:
        out = data
    elif m.method == 8:
        out = zlib.decompress(data, -15)
    else:
        raise ValueError(f"{m.name}: compression method {m.method} not supported")
    if len(out) != m.usize or zlib.crc32(out) != m.crc:
        raise ValueError(f"{m.name}: CRC-32 or size mismatch")
    return out


def chunks(chosen: list[Member], max_bytes: int = 64 << 20, max_gap: int = 1 << 20) -> list[list[Member]]:
    """The chosen members in file order, in runs that one range request can read: neighbours at
    most `max_gap` bytes apart, at most `max_bytes` per run."""
    runs: list[list[Member]] = []
    for m in sorted(chosen, key=lambda m: m.offset):
        if runs and m.offset - runs[-1][-1].end <= max_gap and m.end - runs[-1][0].offset <= max_bytes:
            runs[-1].append(m)
        else:
            runs.append([m])
    return runs


def read_run(url: str, run: list[Member]) -> list[tuple[Member, bytes]]:
    start, end = run[0].offset, run[-1].end - 1
    buf, _ = read_range(url, start, end)
    return [(m, member_bytes(m, buf, start)) for m in run]
