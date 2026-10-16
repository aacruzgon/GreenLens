//! Planes, RANSAC and percentiles, all deterministic: a seeded `ChaCha8Rng` (its stream is
//! specified, unlike `rand`'s `StdRng`), indices drawn by a fixed multiply-shift rule, and a
//! nearest-rank percentile, so measurements stay comparable between versions ([contracts 7]).

use chacha20::ChaCha8Rng;
use nalgebra::{Matrix3, SVD, Vector3};
use rand::{Rng, SeedableRng};

/// A plane `n · p + d = 0` with a unit normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// Unit normal.
    pub n: Vector3<f64>,
    /// Offset.
    pub d: f64,
}

impl Plane {
    /// The distance of a point to the plane.
    pub fn distance(&self, p: &Vector3<f64>) -> f64 {
        (self.n.dot(p) + self.d).abs()
    }

    /// Where the ray from the camera along `dir` meets the plane, as the scale `t` of `dir`;
    /// `None` when parallel or behind the camera.
    pub fn intersect(&self, dir: &Vector3<f64>) -> Option<f64> {
        let denom = self.n.dot(dir);
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = -self.d / denom;
        (t > 0.0).then_some(t)
    }
}

/// A fitted plane with its inliers and their RMS distance (the contract's residual).
#[derive(Debug, Clone, PartialEq)]
pub struct PlaneFit {
    /// The plane, normal facing the camera.
    pub plane: Plane,
    /// Indices of the inliers into the input.
    pub inliers: Vec<usize>,
    /// RMS distance of the inliers to the plane, metres.
    pub rms: f64,
}

/// The least-squares plane through ≥ 3 points: the right singular vector of the centred 3×3
/// covariance with the smallest singular value (looked up, not assumed last).
pub fn fit_plane(points: &[Vector3<f64>]) -> Option<(Plane, f64)> {
    if points.len() < 3 {
        return None;
    }
    let n = points.len() as f64;
    let c = points.iter().sum::<Vector3<f64>>() / n;
    let cov = points
        .iter()
        .map(|p| (p - c) * (p - c).transpose())
        .sum::<Matrix3<f64>>();
    let svd = SVD::new(cov, false, true);
    let v_t = svd.v_t?;
    let (imin, _) = svd
        .singular_values
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))?;
    let mut normal = v_t.row(imin).transpose().normalize();
    if normal.dot(&c) > 0.0 {
        normal = -normal; // face the camera
    }
    let plane = Plane {
        n: normal,
        d: -normal.dot(&c),
    };
    let rms = (points
        .iter()
        .map(|p| plane.distance(p).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    Some((plane, rms))
}

fn plane_from_3(a: &Vector3<f64>, b: &Vector3<f64>, c: &Vector3<f64>) -> Option<Plane> {
    let n = (b - a).cross(&(c - a));
    let len = n.norm();
    (len > 1e-12).then(|| {
        let n = n / len;
        Plane { n, d: -n.dot(a) }
    })
}

/// The measurement RNG: `ChaCha8` seeded from a `u64`.
pub fn seeded(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// An index below `n` from the RNG's next `u64`: Lemire's multiply-shift on the high 32 bits,
/// a fixed rule that doesn't depend on `rand`'s `random_range` staying the same.
pub fn below<R: Rng>(rng: &mut R, n: usize) -> usize {
    (((rng.next_u64() >> 32) * n as u64) >> 32) as usize
}

/// FNV-1a 64 over bytes: the seed of a measurement, from the image id and the region number,
/// so a rerun of the same photo draws the same samples.
pub fn seed_for(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in *part {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// RANSAC for one plane: `iterations` minimal samples, keep the plane with the most inliers
/// within `threshold`, refit on its inliers by SVD.
pub fn ransac(
    points: &[Vector3<f64>],
    threshold: f64,
    iterations: usize,
    seed: u64,
) -> Option<PlaneFit> {
    if points.len() < 3 {
        return None;
    }
    let mut rng = seeded(seed);
    let mut best: Option<(Plane, usize)> = None;
    for _ in 0..iterations {
        let n = points.len();
        let (i, j, k) = (below(&mut rng, n), below(&mut rng, n), below(&mut rng, n));
        if i == j || j == k || i == k {
            continue;
        }
        let Some(candidate) = plane_from_3(&points[i], &points[j], &points[k]) else {
            continue;
        };
        let count = points
            .iter()
            .filter(|p| candidate.distance(p) <= threshold)
            .count();
        if best.as_ref().is_none_or(|b| count > b.1) {
            best = Some((candidate, count));
        }
    }
    let (candidate, _) = best?;
    let inliers: Vec<usize> = (0..points.len())
        .filter(|&i| candidate.distance(&points[i]) <= threshold)
        .collect();
    let chosen: Vec<_> = inliers.iter().map(|&i| points[i]).collect();
    let (plane, rms) = fit_plane(&chosen)?;
    Some(PlaneFit {
        plane,
        inliers,
        rms,
    })
}

/// Nearest-rank percentile (`p` in 0–100) on a copy: the value at rank ⌈p/100 · n⌉.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * v.len() as f64).ceil().max(1.0) as usize;
    Some(v[rank.min(v.len()) - 1])
}

/// The median as the contract's `value` uses it: the nearest-rank 50th percentile.
pub fn median(values: &[f64]) -> Option<f64> {
    percentile(values, 50.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane 0.3 m away tilted 15°, sampled on a grid, plus 80 background outliers.
    fn scene() -> (Vec<Vector3<f64>>, Plane) {
        let truth = Plane {
            n: Vector3::new(0.0, -0.2588, -0.9659).normalize(),
            d: 0.30,
        };
        let mut rng = seeded(1);
        let mut pts = Vec::new();
        for _ in 0..400 {
            let dir = Vector3::new(
                below(&mut rng, 400) as f64 / 2000.0 - 0.1,
                below(&mut rng, 400) as f64 / 2000.0 - 0.1,
                1.0,
            );
            let t = truth.intersect(&dir).unwrap();
            let noise = (below(&mut rng, 2001) as f64 - 1000.0) * 1e-6; // ±1 mm
            pts.push(dir * t + Vector3::new(0.0, 0.0, noise));
        }
        for i in 0..80 {
            pts.push(Vector3::new(0.0, 0.0, 0.6 + f64::from(i) * 0.001));
        }
        (pts, truth)
    }

    #[test]
    fn ransac_finds_the_plane_and_is_deterministic() {
        let (pts, truth) = scene();
        let a = ransac(&pts, 0.003, 200, 42).unwrap();
        let b = ransac(&pts, 0.003, 200, 42).unwrap();
        assert_eq!(a.inliers, b.inliers);
        assert!(a.inliers.len() >= 380 && a.inliers.iter().all(|&i| i < 400));
        assert!(a.plane.n.dot(&truth.n).abs() > 0.999);
        assert!(a.rms < 0.002);
    }

    #[test]
    fn the_seeded_stream_is_pinned() {
        // If a dependency bump changes these, every measurement changes: fail loudly.
        let mut r = seeded(7);
        let v: Vec<usize> = (0..5).map(|_| below(&mut r, 1000)).collect();
        assert_eq!(v, [157, 167, 704, 726, 601]);
        assert_eq!(seed_for(&[b"a"]), seed_for(&[b"a"]));
        assert_ne!(seed_for(&[b"a", b"b"]), seed_for(&[b"ab"]));
    }

    #[test]
    fn percentiles_are_nearest_rank() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 98.0), Some(98.0));
        assert_eq!(percentile(&v, 2.0), Some(2.0));
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[]), None);
    }
}
