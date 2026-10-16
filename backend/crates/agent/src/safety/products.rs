//! Pest-control product names ([contracts 7], plan validation: "products make a step
//! chemical"): the fixed list of the contract plus every name in `products.yaml`, compiled into
//! one case-insensitive regex. The dose check uses it for its "in a sentence with a product
//! name" conditions, and plan validation for re-categorising steps and the broad-spectrum rule.

use std::{collections::HashMap, sync::LazyLock};

use domain::knowledge::ProductsSeed;
use regex::{Regex, RegexBuilder};

/// The product words contracts 7 names, always recognised whatever `products.yaml` holds.
/// Plurals are matched by the regex (`s?`); `sulphur` is the British spelling of `sulfur`.
pub const BUILTIN_PRODUCTS: [&str; 13] = [
    "pesticide",
    "fungicide",
    "insecticide",
    "herbicide",
    "copper",
    "sulfur",
    "sulphur",
    "neem",
    "insecticidal soap",
    "horticultural oil",
    "Bt",
    "spinosad",
    "pyrethrin",
];

/// A product named in a text, borrowed from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductMention<'t> {
    /// The words as they appear in the text ("Neem oil").
    pub name: &'t str,
    /// `products.yaml` marks it `broad_spectrum`; built-in words are not.
    pub broad_spectrum: bool,
}

/// The compiled product list. Built once at start-up from `KnowledgeStore::products()` (the
/// knowledge base changes only with a migration) and shared as `Arc<ProductCatalogue>`; the
/// dose check and plan validation borrow it.
#[derive(Debug, Clone)]
pub struct ProductCatalogue {
    regex: Regex,
    /// Lower-cased name (singular) → `broad_spectrum`.
    broad: HashMap<String, bool>,
}

static BUILTIN: LazyLock<ProductCatalogue> =
    LazyLock::new(|| ProductCatalogue::new(&ProductsSeed::default()));

impl ProductCatalogue {
    /// The built-in words plus every product of `products.yaml`.
    pub fn new(seed: &ProductsSeed) -> Self {
        let mut broad: HashMap<String, bool> = BUILTIN_PRODUCTS
            .iter()
            .map(|p| (p.to_lowercase(), false))
            .collect();
        for p in &seed.products {
            broad.insert(p.name.to_lowercase(), p.broad_spectrum);
        }
        // Longest names first, so "insecticidal soap" wins over a shorter prefix.
        let mut names: Vec<&String> = broad.keys().collect();
        names.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        let alternation = names
            .iter()
            .map(|n| regex::escape(n).replace(' ', r"\s+"))
            .collect::<Vec<_>>()
            .join("|");
        let pattern = format!(r"\b(?:{alternation})(?:s|es)?\b");
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .size_limit(1 << 24)
            .build()
            // reason: every name is escaped, so the pattern is an alternation of literals and
            // `\s+`, which always compiles within the size limit for a list of product names.
            .expect("product names compile");
        Self { regex, broad }
    }

    /// The built-in list only, for tests and for callers without a knowledge base.
    pub fn builtin() -> &'static Self {
        &BUILTIN
    }

    /// The first product named in `text`.
    pub fn mention<'t>(&self, text: &'t str) -> Option<ProductMention<'t>> {
        self.mentions(text).next()
    }

    /// Every product named in `text`, in order.
    ///
    /// `use<'c, 't>`: the iterator borrows both the catalogue (`'c`, the compiled regex and the
    /// lookup table) and the text (`'t`, which each [`ProductMention`] points into). Edition 2024
    /// would capture both anyway; the bound says so explicitly.
    pub fn mentions<'c, 't>(
        &'c self,
        text: &'t str,
    ) -> impl Iterator<Item = ProductMention<'t>> + use<'c, 't> {
        self.regex.find_iter(text).map(move |m| ProductMention {
            name: m.as_str(),
            broad_spectrum: self.is_broad(m.as_str()),
        })
    }

    /// Whether the text names a product at all.
    pub fn is_named_in(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }

    fn is_broad(&self, found: &str) -> bool {
        let words = found.split_whitespace().collect::<Vec<_>>().join(" ");
        let lower = words.to_lowercase();
        let singular = [
            lower.as_str(),
            lower.strip_suffix("es").unwrap_or(&lower),
            lower.strip_suffix('s').unwrap_or(&lower),
        ];
        singular
            .iter()
            .find_map(|n| self.broad.get(*n).copied())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use domain::knowledge::ProductSeed;

    use super::*;

    #[test]
    fn builtin_words_are_found_with_plurals_and_case() {
        let c = ProductCatalogue::builtin();
        assert_eq!(c.mention("Spray Neem oil at dusk").unwrap().name, "Neem");
        assert_eq!(c.mention("apply fungicides").unwrap().name, "fungicides");
        assert_eq!(
            c.mention("an insecticidal  soap spray").unwrap().name,
            "insecticidal  soap"
        );
        assert!(c.mention("I doubt it").is_none(), "Bt is a whole word");
        assert!(c.mention("Remove the lowest leaves").is_none());
    }

    #[test]
    fn knowledge_base_products_carry_broad_spectrum() {
        let seed = ProductsSeed {
            products: vec![
                ProductSeed {
                    name: "malathion".to_owned(),
                    broad_spectrum: true,
                },
                ProductSeed {
                    name: "pyrethrin".to_owned(),
                    broad_spectrum: true,
                },
            ],
        };
        let c = ProductCatalogue::new(&seed);
        let found: Vec<_> = c
            .mentions("Malathion or pyrethrins, never copper")
            .collect();
        assert_eq!(found.len(), 3);
        assert!(found[0].broad_spectrum);
        assert!(
            found[1].broad_spectrum,
            "plural of a broad-spectrum product"
        );
        assert!(!found[2].broad_spectrum);
    }
}
