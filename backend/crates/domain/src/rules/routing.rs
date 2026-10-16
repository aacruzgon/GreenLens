//! Photo routing to the VLM ([contracts 5], photo routing): which images each call gets.
//! Works on borrowed slices: it only picks and orders.

use crate::{jobs::Indicator, rules::limits::MAX_VLM_IMAGES, scan::ImageKind};

/// One candidate image for routing: its kind and its position among the scan's images of
/// that kind (0 for the first `leaf_closeup`, 1 for the second).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutableImage<T> {
    /// Its kind.
    pub kind: ImageKind,
    /// Its rank within its kind, in upload order.
    pub nth_of_kind: u32,
    /// Whatever the caller routes (an id, a `JobImage`).
    pub item: T,
}

/// The findings order: `whole_plant`, first `leaf_closeup`, `leaf_underside`, `soil_surface`,
/// second `leaf_closeup`, `flower_fruit`.
pub const FINDINGS_ORDER: [(ImageKind, u32); 6] = [
    (ImageKind::WholePlant, 0),
    (ImageKind::LeafCloseup, 0),
    (ImageKind::LeafUnderside, 0),
    (ImageKind::SoilSurface, 0),
    (ImageKind::LeafCloseup, 1),
    (ImageKind::FlowerFruit, 0),
];

fn findings_rank(kind: ImageKind, nth: u32) -> usize {
    FINDINGS_ORDER
        .iter()
        .position(|&(k, n)| k == kind && n == nth)
        .unwrap_or(FINDINGS_ORDER.len())
}

/// The findings call: the first 4 present, in the findings order.
pub fn findings_images<T: Clone>(images: &[RoutableImage<T>]) -> Vec<T> {
    let mut ranked: Vec<(usize, &RoutableImage<T>)> = images
        .iter()
        .filter(|i| i.kind.is_plant_or_soil())
        .map(|i| (findings_rank(i.kind, i.nth_of_kind), i))
        .filter(|(rank, _)| *rank < FINDINGS_ORDER.len())
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .take(MAX_VLM_IMAGES)
        .map(|(_, i)| i.item.clone())
        .collect()
}

/// The indicators call: each image scores the number of indicators that list its kind; the 4
/// highest are sent, ties broken by the findings order, and `whole_plant` is included whenever
/// any indicator lists it. `first` is a photo that must come first (a `verify` job's new
/// photo); it takes one of the 4 places.
pub fn indicator_images<T: Clone>(
    images: &[RoutableImage<T>],
    indicators: &[&Indicator],
    first: Option<&RoutableImage<T>>,
) -> Vec<T> {
    let score = |kind: ImageKind| {
        indicators
            .iter()
            .filter(|ind| ind.image_kinds.contains(&kind))
            .count()
    };
    let mut ranked: Vec<(usize, usize, &RoutableImage<T>)> = images
        .iter()
        .filter(|i| i.kind.is_plant_or_soil())
        .map(|i| (score(i.kind), findings_rank(i.kind, i.nth_of_kind), i))
        .filter(|(s, _, _)| *s > 0)
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let room = MAX_VLM_IMAGES - usize::from(first.is_some());
    let mut chosen: Vec<&RoutableImage<T>> = ranked.iter().take(room).map(|(_, _, i)| *i).collect();
    let wants_whole = score(ImageKind::WholePlant) > 0;
    let has_whole = chosen.iter().any(|i| i.kind == ImageKind::WholePlant);
    if wants_whole
        && !has_whole
        && let Some((_, _, whole)) = ranked
            .iter()
            .find(|(_, _, i)| i.kind == ImageKind::WholePlant)
    {
        if chosen.len() == room {
            chosen.pop();
        }
        chosen.push(whole);
    }
    first
        .into_iter()
        .chain(chosen)
        .map(|i| i.item.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::IndicatorWhere;

    fn img(kind: ImageKind, nth: u32, item: &'static str) -> RoutableImage<&'static str> {
        RoutableImage {
            kind,
            nth_of_kind: nth,
            item,
        }
    }

    fn ind(kinds: &[ImageKind]) -> Indicator {
        Indicator {
            indicator_id: "p.x".parse().unwrap(),
            question: String::new(),
            r#where: IndicatorWhere::LowerLeaves,
            image_kinds: kinds.to_vec(),
        }
    }

    #[test]
    fn findings_follow_the_fixed_order() {
        use ImageKind as K;
        let images = [
            img(K::FlowerFruit, 0, "flower"),
            img(K::LeafCloseup, 1, "leaf2"),
            img(K::SoilSurface, 0, "soil"),
            img(K::LeafCloseup, 0, "leaf1"),
            img(K::WholePlant, 0, "whole"),
            img(K::JarTest, 0, "jar"),
        ];
        assert_eq!(
            findings_images(&images),
            vec!["whole", "leaf1", "soil", "leaf2"]
        );
    }

    #[test]
    fn indicators_pick_by_score_and_keep_whole_plant() {
        use ImageKind as K;
        let images = [
            img(K::WholePlant, 0, "whole"),
            img(K::LeafCloseup, 0, "leaf1"),
            img(K::LeafCloseup, 1, "leaf2"),
            img(K::LeafUnderside, 0, "under"),
            img(K::SoilSurface, 0, "soil"),
        ];
        let a = ind(&[K::LeafCloseup]);
        let b = ind(&[K::LeafCloseup, K::LeafUnderside]);
        let c = ind(&[K::SoilSurface]);
        let d = ind(&[K::WholePlant]);
        let chosen = indicator_images(&images, &[&a, &b, &c, &d], None);
        assert_eq!(chosen.len(), 4);
        assert_eq!(&chosen[..2], &["leaf1", "leaf2"]);
        assert!(chosen.contains(&"whole"));

        let first = img(K::LeafUnderside, 1, "new");
        let chosen = indicator_images(&images, &[&a], Some(&first));
        assert_eq!(chosen, vec!["new", "leaf1", "leaf2"]);
    }
}
