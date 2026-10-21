//! Soil Landscapes of Canada v3.2 (Agriculture and Agri-Food Canada): the offline soil profile
//! for Canadian points where `SoilGrids` has no data (architecture 6.4 and open decision 10).
//!
//! `SoilGrids` masks built-up land, so central Toronto, Scarborough, North York, Guelph and
//! Ottawa answered `null` for every property on 2026-10-15 and again on 2026-10-20. SLC maps
//! the soil landscapes of Canada's agricultural regions at 1:1,000,000, cities included: every
//! one of those points lies in a polygon with a described soil. `tools/slc/build_slc.py` turns
//! the published polygons and tables into `data/slc_v3r2.bin` (0.9 MB): each polygon's dominant
//! soil averaged onto the six `SoilGrids` depth bands, and the polygons rasterised on a 0.01°
//! grid. This module reads that file; it makes no network call.
//!
//! Contains information licensed under the Open Government Licence – Canada
//! (<https://open.canada.ca/en/open-government-licence-canada>): Soil Landscapes of Canada
//! v3.2, Agriculture and Agri-Food Canada.

use domain::{
    knowledge::{DepthBand, SoilDepth},
    records::weather::Coordinates,
};

/// The name used in logs.
pub const PROVIDER: &str = "slc";

/// The lookup built by `tools/slc/build_slc.py`, embedded in the binary.
pub const EMBEDDED: &[u8] = include_bytes!("../data/slc_v3r2.bin");

const MAGIC: &[u8; 4] = b"SLC1";
const VERSION: u32 = 1;
const HEADER_LEN: usize = 36;
const BANDS: [DepthBand; 6] = [
    DepthBand::D0To5,
    DepthBand::D5To15,
    DepthBand::D15To30,
    DepthBand::D30To60,
    DepthBand::D60To100,
    DepthBand::D100To200,
];
/// Values per band: pH, clay, sand, silt, organic carbon, nitrogen, CEC, bulk density.
const VALUES: usize = 8;
const EMPTY: u16 = u16::MAX;

/// Why the embedded file didn't load: only a bad build of `data/slc_v3r2.bin` causes it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SlcError {
    /// Not an `SLC1` file of version 1.
    #[error("not a version {VERSION} SLC lookup")]
    Header,
    /// The file is shorter or longer than its header says.
    #[error("the SLC lookup is {got} bytes, its header says {expected}")]
    Length {
        /// Bytes read.
        got: usize,
        /// Bytes the header implies.
        expected: usize,
    },
    /// A run points past the grid or the soils.
    #[error("run {0} is out of range")]
    Run(usize),
}

/// One run of cells in a grid row with the same soil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    first_col: u16,
    len: u16,
    soil: u16,
}

/// The parsed lookup. Built once at start and shared, like the time-zone finder.
pub struct SoilLandscapes {
    lat0_e4: i64,
    lon0_e4: i64,
    step_e4: i64,
    cols: usize,
    /// Per soil, the six bands' values × 100, `EMPTY` where none.
    soils: Vec<[[u16; VALUES]; 6]>,
    /// Index of each row's first run; one more entry than rows.
    row_starts: Vec<u32>,
    runs: Vec<Run>,
}

impl std::fmt::Debug for SoilLandscapes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoilLandscapes")
            .field("soils", &self.soils.len())
            .field("rows", &self.row_starts.len().saturating_sub(1))
            .field("runs", &self.runs.len())
            .finish_non_exhaustive()
    }
}

/// Little-endian reader over the file.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u16(&mut self) -> u16 {
        let v = u16::from_le_bytes([self.bytes[self.at], self.bytes[self.at + 1]]);
        self.at += 2;
        v
    }

    fn u32(&mut self) -> u32 {
        let b = &self.bytes[self.at..self.at + 4];
        self.at += 4;
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }

    fn i32(&mut self) -> i32 {
        let b = &self.bytes[self.at..self.at + 4];
        self.at += 4;
        i32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}

impl SoilLandscapes {
    /// The lookup embedded in the binary.
    ///
    /// # Errors
    ///
    /// [`SlcError`] when the embedded file is malformed, which a test rules out.
    pub fn embedded() -> Result<Self, SlcError> {
        Self::parse(EMBEDDED)
    }

    /// Parses a lookup in the layout `tools/slc/build_slc.py` documents.
    ///
    /// # Errors
    ///
    /// [`SlcError`] for a wrong header, a length that doesn't match it, or a run outside the
    /// grid or the soil table.
    pub fn parse(bytes: &[u8]) -> Result<Self, SlcError> {
        if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
            return Err(SlcError::Header);
        }
        let mut r = Reader { bytes, at: 4 };
        if r.u32() != VERSION {
            return Err(SlcError::Header);
        }
        let lat0_e4 = i64::from(r.i32());
        let lon0_e4 = i64::from(r.i32());
        let step_e4 = i64::from(r.u32());
        let to_usize = |v: u32| usize::try_from(v).map_err(|_| SlcError::Header);
        let rows = to_usize(r.u32())?;
        let cols = to_usize(r.u32())?;
        let n_soils = to_usize(r.u32())?;
        let n_runs = to_usize(r.u32())?;
        if step_e4 == 0 {
            return Err(SlcError::Header);
        }
        let expected = HEADER_LEN + n_soils * 6 * VALUES * 2 + (rows + 1) * 4 + n_runs * 6;
        if bytes.len() != expected {
            return Err(SlcError::Length {
                got: bytes.len(),
                expected,
            });
        }
        let soils = (0..n_soils)
            .map(|_| std::array::from_fn(|_| std::array::from_fn(|_| r.u16())))
            .collect();
        let row_starts: Vec<u32> = (0..=rows).map(|_| r.u32()).collect();
        let runs: Vec<Run> = (0..n_runs)
            .map(|_| Run {
                first_col: r.u16(),
                len: r.u16(),
                soil: r.u16(),
            })
            .collect();
        if row_starts.windows(2).any(|w| w[0] > w[1])
            || row_starts.last().map(|&l| l as usize) != Some(n_runs)
        {
            return Err(SlcError::Run(n_runs));
        }
        for (i, run) in runs.iter().enumerate() {
            if usize::from(run.first_col) + usize::from(run.len) > cols
                || usize::from(run.soil) >= n_soils
            {
                return Err(SlcError::Run(i));
            }
        }
        Ok(Self {
            lat0_e4,
            lon0_e4,
            step_e4,
            cols,
            soils,
            row_starts,
            runs,
        })
    }

    /// The soil at a point, on the six depth bands; `None` outside SLC's agricultural
    /// coverage, on water, and in polygons with no described soil. Nitrogen is always `None`:
    /// SLC doesn't map it.
    pub fn depths(&self, at: Coordinates) -> Option<Vec<SoilDepth>> {
        let cell = |v: f64, origin: i64| {
            // reason: degrees × 10⁴ is at most 1.8 million; the floor is an exact grid index.
            #[expect(clippy::cast_possible_truncation, reason = "|v| ≤ 180° × 10⁴")]
            let e4 = (v * 1e4).floor() as i64;
            (e4 - origin).div_euclid(self.step_e4)
        };
        let row = usize::try_from(cell(at.lat, self.lat0_e4)).ok()?;
        let col = usize::try_from(cell(at.lon, self.lon0_e4)).ok()?;
        if col >= self.cols {
            return None;
        }
        let first = *self.row_starts.get(row)? as usize;
        let end = *self.row_starts.get(row + 1)? as usize;
        let run = self.runs[first..end].iter().find(|r| {
            usize::from(r.first_col) <= col && col < usize::from(r.first_col) + usize::from(r.len)
        })?;
        let soil = &self.soils[usize::from(run.soil)];
        Some(
            BANDS
                .iter()
                .zip(soil)
                .map(|(&depth_cm, v)| {
                    let get = |i: usize| (v[i] != EMPTY).then(|| f64::from(v[i]) / 100.0);
                    SoilDepth {
                        depth_cm,
                        ph_h2o: get(0),
                        clay_pct: get(1),
                        sand_pct: get(2),
                        silt_pct: get(3),
                        organic_carbon_g_kg: get(4),
                        nitrogen_g_kg: get(5),
                        cec_cmol_kg: get(6),
                        bulk_density_kg_dm3: get(7),
                    }
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::*;

    static SLC: LazyLock<SoilLandscapes> = LazyLock::new(|| SoilLandscapes::embedded().unwrap());

    fn at(lat: f64, lon: f64) -> Option<Vec<SoilDepth>> {
        SLC.depths(Coordinates::new(lat, lon).unwrap())
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn the_embedded_file_loads() {
        let s = &*SLC;
        assert_eq!(s.soils.len(), 1265);
        assert_eq!(s.runs.len(), 126_760);
    }

    #[test]
    fn central_toronto_is_chinguacousy_clay_loam() {
        // SLC polygon 563001: 100 % Chinguacousy, cultivated profile: 0–27 cm pH 7.4, 29 %
        // clay, 21 % sand, 50 % silt, 1.9 % organic carbon, CEC 33, 1.28 g/cm³.
        let d = at(43.65, -79.38).unwrap();
        assert_eq!(d.len(), 6);
        let top = d[0];
        assert_eq!(top.depth_cm, DepthBand::D0To5);
        assert!(close(top.ph_h2o, 7.4));
        assert!(close(top.clay_pct, 29.0));
        assert!(close(top.sand_pct, 21.0));
        assert!(close(top.silt_pct, 50.0));
        assert!(close(top.organic_carbon_g_kg, 19.0));
        assert!(close(top.cec_cmol_kg, 33.0));
        assert!(close(top.bulk_density_kg_dm3, 1.28));
        assert!(top.nitrogen_g_kg.is_none(), "SLC has no nitrogen");
        // 15–30 cm straddles the 27 cm boundary: (12 × 7.4 + 3 × 7.6) / 15 = 7.44.
        assert!(close(d[2].ph_h2o, 7.44));
        // The profile ends at 100 cm.
        assert!(d[5].ph_h2o.is_none() && d[5].clay_pct.is_none());
        // Scarborough lies in the same polygon.
        assert_eq!(at(43.77, -79.23), Some(d));
    }

    #[test]
    fn the_null_pixels_of_soilgrids_have_a_soil() {
        for (lat, lon) in [(43.75, -79.55), (43.55, -80.25), (45.42, -75.70)] {
            assert!(at(lat, lon).is_some(), "{lat}, {lon}");
        }
        // Guelph: Burford, a loam with pH 5.0 at the surface.
        assert!(close(at(43.55, -80.25).unwrap()[0].ph_h2o, 5.0));
    }

    #[test]
    fn outside_coverage_is_none() {
        assert!(at(42.0, -93.6).is_none(), "Iowa");
        assert!(at(43.5, -78.5).is_none(), "Lake Ontario");
        assert!(at(70.0, -100.0).is_none(), "north of the grid");
        assert!(at(-33.9, 151.2).is_none(), "south and east of the grid");
    }

    #[test]
    fn malformed_files_are_refused() {
        assert_eq!(
            SoilLandscapes::parse(b"nope").unwrap_err(),
            SlcError::Header
        );
        let short = &EMBEDDED[..EMBEDDED.len() - 1];
        assert!(matches!(
            SoilLandscapes::parse(short),
            Err(SlcError::Length { .. })
        ));
    }
}
