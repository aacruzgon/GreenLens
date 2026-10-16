//! COCO run-length masks ([contracts 5], `RleMask`): a port of `rleToString`, `rleFrString`
//! and the encode and decode loops of the COCO API's `maskApi.c`. No maintained Rust crate does
//! this, and the algorithm is short.
//!
//! The mask is flattened **column-major** (index `i` is row `i % h`, column `i / h`); `counts`
//! alternates runs of 0 and 1, starting with 0. The string stores each count as a little-endian
//! base-32 varint in characters offset by 48: 5 data bits per character, `0x20` = more follow,
//! `0x10` on the last = negative. From the third count on, each is stored as the difference to
//! the count two places earlier. `size` is `[height, width]`, as in COCO.

use domain::{jobs::RleMask, ports::MaskView};

use crate::mask::MaskBuf;

/// The largest mask decoded: 64 megapixels.
pub const MAX_PIXELS: u64 = 64_000_000;

/// Why a mask string is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RleError {
    /// A character outside `0`…`o` (48–111).
    #[error("invalid character {0:#04x}")]
    BadChar(u8),
    /// A count's last character is missing, or a count is longer than any valid one.
    #[error("truncated count")]
    Truncated,
    /// A count decodes to a negative run.
    #[error("negative run")]
    Negative,
    /// The runs don't add up to `height × width`.
    #[error("runs sum to {sum}, expected {expected}")]
    SizeMismatch {
        /// Their sum.
        sum: u64,
        /// `height × width`.
        expected: u64,
    },
    /// Over [`MAX_PIXELS`].
    #[error("mask too large")]
    TooLarge,
}

/// `rleFrString`: the counts of a compressed string.
#[allow(
    clippy::many_single_char_names,
    reason = "the names of maskApi.c, so the port reads against it"
)]
pub fn counts_from_string(s: &str) -> Result<Vec<u32>, RleError> {
    let b = s.as_bytes();
    let mut counts: Vec<i64> = Vec::with_capacity(b.len());
    let mut p = 0;
    while p < b.len() {
        let (mut x, mut k, mut more) = (0i64, 0u32, true);
        while more {
            let ch = *b.get(p).ok_or(RleError::Truncated)?;
            if !(48..48 + 64).contains(&ch) {
                return Err(RleError::BadChar(ch));
            }
            let c = i64::from(ch - 48);
            x |= (c & 0x1f) << (5 * k);
            more = c & 0x20 != 0;
            p += 1;
            k += 1;
            if !more && (c & 0x10) != 0 {
                x |= -1i64 << (5 * k);
            }
            if k > 12 {
                return Err(RleError::Truncated);
            }
        }
        if counts.len() > 2 {
            x += counts[counts.len() - 2];
        }
        counts.push(x);
    }
    counts
        .into_iter()
        .map(|c| u32::try_from(c).map_err(|_| RleError::Negative))
        .collect()
}

/// `rleToString`: the compressed string of counts.
pub fn string_from_counts(counts: &[u32]) -> String {
    let mut s = String::new();
    for (i, &c) in counts.iter().enumerate() {
        let mut x = i64::from(c);
        if i > 2 {
            x -= i64::from(counts[i - 2]);
        }
        loop {
            let mut ch = (x & 0x1f) as u8;
            x >>= 5;
            let more = if ch & 0x10 != 0 { x != -1 } else { x != 0 };
            if more {
                ch |= 0x20;
            }
            s.push(char::from(ch + 48));
            if !more {
                break;
            }
        }
    }
    s
}

/// Encodes a mask (non-zero = inside) the way `maskApi.c` does, byte for byte.
pub fn encode(mask: MaskView<'_>) -> RleMask {
    let (w, h) = (mask.width(), mask.height());
    let mut counts = Vec::new();
    let (mut run, mut current) = (0u32, false);
    for col in 0..w {
        for row in 0..h {
            let v = mask.data()[row as usize * w as usize + col as usize] != 0;
            if v != current {
                counts.push(run);
                run = 0;
                current = v;
            }
            run += 1;
        }
    }
    counts.push(run);
    RleMask {
        size: [h, w],
        counts: string_from_counts(&counts),
    }
}

/// Decodes a mask, checking every count and that the runs cover exactly `height × width`.
pub fn decode(rle: &RleMask) -> Result<MaskBuf, RleError> {
    let [h, w] = rle.size;
    let total = u64::from(h) * u64::from(w);
    if total > MAX_PIXELS {
        return Err(RleError::TooLarge);
    }
    let counts = counts_from_string(&rle.counts)?;
    let sum: u64 = counts.iter().map(|&c| u64::from(c)).sum();
    if sum != total {
        return Err(RleError::SizeMismatch {
            sum,
            expected: total,
        });
    }
    let mut mask = MaskBuf::empty(w, h);
    let (mut i, mut value) = (0u64, false);
    for c in counts {
        if value {
            for j in i..i + u64::from(c) {
                mask.set((j / u64::from(h)) as u32, (j % u64::from(h)) as u32, true);
            }
        }
        i += u64::from(c);
        value = !value;
    }
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixtures written by pycocotools (a wrapper over the C `maskApi`), from the vision probe.
    #[test]
    fn matches_pycocotools_byte_for_byte() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/rle_cases.json")).unwrap();
        for c in cases.as_array().unwrap() {
            let h = u32::try_from(c["h"].as_u64().unwrap()).unwrap();
            let w = u32::try_from(c["w"].as_u64().unwrap()).unwrap();
            let rle = RleMask {
                size: [h, w],
                counts: c["counts"].as_str().unwrap().to_owned(),
            };
            let mask = decode(&rle).unwrap();
            for (i, v) in c["flat_f"].as_array().unwrap().iter().enumerate() {
                let i = u32::try_from(i).unwrap();
                assert_eq!(mask.get(i / h, i % h), v.as_u64() == Some(1));
            }
            assert_eq!(encode(mask.view()), rle);
        }
    }

    #[test]
    fn rejects_garbage() {
        let rle = |counts: &str, h, w| RleMask {
            size: [h, w],
            counts: counts.to_owned(),
        };
        assert_eq!(decode(&rle("!!", 2, 2)), Err(RleError::BadChar(b'!')));
        assert_eq!(decode(&rle("0", 100_000, 100_000)), Err(RleError::TooLarge));
        assert_eq!(counts_from_string("a"), Err(RleError::Truncated));
        assert!(matches!(
            decode(&rle("5", 2, 2)),
            Err(RleError::SizeMismatch { .. })
        ));
    }
}
