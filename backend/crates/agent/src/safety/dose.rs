//! The dose check ([contracts 7], the dose check): plans and chat replies never give an amount
//! of a product. It runs on every plan text, every assistant sentence before it is streamed and
//! every string the model writes into a tool call.
//!
//! Every pattern is compiled once ([`LazyLock`]) and matched case-insensitively. Units are
//! followed by `\b`, so "2 lowest leaves" and "3 large leaves" never read as litres. The rules
//! that depend on the rest of the sentence (ratios with `in` or one `parts`, percentages) are
//! checked per sentence, against the mixture words and the [`ProductCatalogue`].

use std::{ops::Range, sync::LazyLock};

use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};

use super::{ProductCatalogue, sentences::sentence_spans};

/// A quantity in the measure rules: a number (`2`, `0.5`, `1/2`, `1 1/2`, `½`) or a number word
/// (`one` … `twenty`, `half`, `a`, `an`).
const QUANTITY: &str = r"(?:(?:\d+(?:[.,]\d+)?(?:\s+\d+\s*/\s*\d+)?|\d+\s*/\s*\d+|\d*[½¼¾⅓⅔⅛])|\b(?:one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty|half|a|an)\b)";

/// A quantity in the ratio rules: a number or `one` … `twenty`, never `a`, `an` or `half`.
const RATIO_QUANTITY: &str = r"(?:\d+(?:[.,]\d+)?|\b(?:one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty)\b)";

/// The measures that always count after a quantity.
const MEASURE: &str = r"(?:ml|millilitres?|milliliters?|l|litres?|liters?|fl\.?\s*oz|oz|ounces?|pints?|quarts?|gallons?|gal|g|grams?|kg|mg|lbs?|pounds?|tbsp|tablespoons?|tsp|teaspoons?|ppm)";

/// The everyday words that count only when `of` follows within three words.
const EVERYDAY: &str = r"(?:drops?|cups?|spoonfuls?|capfuls?|pinch(?:es)?)";

/// "…of within three words": `of` is one of the next three words.
const OF_WITHIN_THREE: &str = r"(?:\s+[\p{L}'’-]+){0,2}\s+of\b";

/// Which rule a text broke. The order is the order of contracts 7's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DoseRule {
    /// A quantity followed by a measure: "2 tbsp", "half a litre", "10ml".
    Measure,
    /// A quantity followed by `drop`, `cup`, `spoonful`, `capful` or `pinch`, with `of` within
    /// three words: "a drop of soap", "two cups of water".
    EverydayMeasure,
    /// "a few drops", "a splash" or "a squirt" with `of` within three words.
    VagueMeasure,
    /// A rate: `per litre`, `per liter`, `per gallon`, `/l`, `/gal` (so `ml/l`, `g/l`).
    Rate,
    /// Two numbers around a colon: "1:10" (clock times are a known false positive).
    ColonRatio,
    /// "<quantity> part(s) … to <quantity> part(s)".
    PartsRatio,
    /// "<quantity> part(s) … to <quantity>" or "<quantity> in <quantity>", in a sentence with a
    /// product name or a mixture word.
    MixtureRatio,
    /// A percentage in a sentence with a product name or a mixture word.
    Percentage,
}

impl DoseRule {
    /// What the model is told when a call or a plan text fails ([contracts 7]: "the model gets
    /// the reason").
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::Measure | Self::EverydayMeasure | Self::VagueMeasure => {
                "it gives an amount of something to apply"
            }
            Self::Rate => "it gives a rate of application",
            Self::ColonRatio | Self::PartsRatio | Self::MixtureRatio => "it gives a mixing ratio",
            Self::Percentage => "it gives a percentage of a product or mixture",
        }
    }
}

/// Where a text failed: borrowed from the checked text, so the caller can explain the failure
/// (log the rule, tell the model which words to remove) without copying.
///
/// `'t` is the lifetime of the checked text; the match lives no longer than it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoseMatch<'t> {
    /// The rule.
    pub rule: DoseRule,
    /// The words that matched ("2 tbsp", "1:10").
    pub matched: &'t str,
    /// Their byte range in the whole text.
    pub span: Range<usize>,
    /// The sentence they are in.
    pub sentence: &'t str,
}

impl DoseMatch<'_> {
    /// The reason sent back to the model or the plan writer. Names the words, never repeats the
    /// whole sentence.
    pub fn reason(&self) -> String {
        format!(
            "\"{}\" is not allowed because {}. Never give doses, rates, ratios or percentages of a \
             product; say to follow the product label instead.",
            self.matched,
            self.rule.explanation()
        )
    }

    /// An owned copy, for a value that must outlive the text (a stored rejection, a log field
    /// moved into a task).
    pub fn to_owned_match(&self) -> OwnedDoseMatch {
        OwnedDoseMatch {
            rule: self.rule,
            matched: self.matched.to_owned(),
            span: self.span.clone(),
        }
    }
}

/// [`DoseMatch`] without the borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedDoseMatch {
    /// The rule.
    pub rule: DoseRule,
    /// The words.
    pub matched: String,
    /// Their byte range in the text that was checked.
    pub span: Range<usize>,
}

/// One compiled rule.
struct Rule {
    kind: DoseRule,
    regex: Regex,
    /// Counts only in a sentence with a product name or a mixture word.
    needs_mixture: bool,
}

fn ci(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        // reason: the patterns are the constants of this module, covered by the tests below.
        .expect("dose pattern compiles")
}

fn patterns() -> [(DoseRule, String, bool); 9] {
    [
        (
            DoseRule::Measure,
            format!(r"{QUANTITY}\s*-?\s*{MEASURE}\b"),
            false,
        ),
        (
            DoseRule::EverydayMeasure,
            format!(r"{QUANTITY}\s*-?\s*{EVERYDAY}\b{OF_WITHIN_THREE}"),
            false,
        ),
        (
            DoseRule::VagueMeasure,
            format!(r"\ba\s+(?:few\s+drops|splash|squirt)\b{OF_WITHIN_THREE}"),
            false,
        ),
        (
            DoseRule::Rate,
            r"\bper\s+(?:litre|liter|gallon)s?\b|/\s*(?:l|gal)\b".to_owned(),
            false,
        ),
        (DoseRule::ColonRatio, r"\d+\s*:\s*\d+".to_owned(), false),
        (
            DoseRule::PartsRatio,
            format!(r"{RATIO_QUANTITY}\s+parts?\b.*?\bto\s+{RATIO_QUANTITY}\s+parts?\b"),
            false,
        ),
        (
            DoseRule::MixtureRatio,
            format!(r"{RATIO_QUANTITY}\s+parts?\b.*?\bto\s+{RATIO_QUANTITY}"),
            true,
        ),
        (
            DoseRule::MixtureRatio,
            format!(r"{RATIO_QUANTITY}\s+in\s+{RATIO_QUANTITY}"),
            true,
        ),
        (
            DoseRule::Percentage,
            r"\d+(?:[.,]\d+)?\s*(?:%|percent\b|per\s+cent\b)".to_owned(),
            true,
        ),
    ]
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    patterns()
        .into_iter()
        .map(|(kind, pattern, needs_mixture)| Rule {
            kind,
            regex: ci(&pattern),
            needs_mixture,
        })
        .collect()
});

/// Every rule in one set: a sentence that matches none of them (almost every sentence) is
/// rejected in a single pass before any rule is run on its own.
static ANY: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSetBuilder::new(patterns().map(|(_, p, _)| p))
        .case_insensitive(true)
        .build()
        // reason: the same patterns as RULES.
        .expect("dose pattern set compiles")
});

/// The mixture words of contracts 7 and their inflections.
static MIXTURE: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"\b(?:solutions?|mix(?:es|ed|ing)?|mixtures?|spray(?:s|ed|ing)?|dilut(?:e|es|ed|ing|ion)|concentrations?)\b",
    )
});

/// Whether a sentence holds a mixture word or a product name, the condition of the ratio and
/// percentage rules.
fn mixture_context(sentence: &str, products: &ProductCatalogue) -> bool {
    MIXTURE.is_match(sentence) || products.is_named_in(sentence)
}

/// Checks a text. Returns the first failing match (earliest sentence, then earliest position),
/// or `None` when the text passes.
///
/// `'t` names the text's lifetime because the result borrows from `text` and not from
/// `products`: elision would tie the result to both references (there are two inputs), so the
/// signature says which one the match points into.
pub fn dose_check<'t>(text: &'t str, products: &ProductCatalogue) -> Option<DoseMatch<'t>> {
    sentence_spans(text).find_map(|range| {
        let sentence = &text[range.clone()];
        check_sentence(sentence, products).map(|(rule, m)| DoseMatch {
            rule,
            matched: &sentence[m.clone()],
            span: (range.start + m.start)..(range.start + m.end),
            sentence,
        })
    })
}

/// Checks several texts (a plan's strings, a tool call's string arguments) and returns the first
/// failure. The items may borrow from different owners as long as all outlive `'t`.
pub fn dose_check_all<'t, I>(texts: I, products: &ProductCatalogue) -> Option<DoseMatch<'t>>
where
    I: IntoIterator<Item = &'t str>,
{
    texts.into_iter().find_map(|t| dose_check(t, products))
}

fn check_sentence(sentence: &str, products: &ProductCatalogue) -> Option<(DoseRule, Range<usize>)> {
    let hits = ANY.matches(sentence);
    if !hits.matched_any() {
        return None;
    }
    let mut mixture: Option<bool> = None;
    RULES
        .iter()
        .enumerate()
        .filter(|(i, _)| hits.matched(*i))
        .filter(|(_, r)| {
            !r.needs_mixture || *mixture.get_or_insert_with(|| mixture_context(sentence, products))
        })
        .filter_map(|(_, r)| r.regex.find(sentence).map(|m| (r.kind, m.range())))
        .min_by_key(|(_, m)| m.start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str) -> Option<DoseRule> {
        dose_check(text, ProductCatalogue::builtin()).map(|m| m.rule)
    }

    /// Every example of contracts 7, plus one per measure and rate.
    #[test]
    fn contract_examples_fail() {
        use DoseRule::*;
        let cases: &[(&str, DoseRule)] = &[
            ("Mix 2 tbsp into the watering can.", Measure),
            ("Use 0.5 l of water.", Measure),
            ("Add 1/2 tsp.", Measure),
            ("Add ½ cup of soap.", EverydayMeasure),
            ("Add ¼ teaspoon.", Measure),
            ("Add 1 1/2 tablespoons.", Measure),
            ("Use half a litre.", Measure),
            ("Use an ounce.", Measure),
            ("Dissolve 10ml in water.", Measure),
            ("10 millilitres", Measure),
            ("10 milliliters", Measure),
            ("2 litres", Measure),
            ("2 liters", Measure),
            ("2 fl oz", Measure),
            ("2 oz", Measure),
            ("two ounces", Measure),
            ("a pint", Measure),
            ("three quarts", Measure),
            ("one gallon", Measure),
            ("5 gal", Measure),
            ("5 g", Measure),
            ("five grams", Measure),
            ("1 kg", Measure),
            ("20 mg", Measure),
            ("2 lbs", Measure),
            ("a pound", Measure),
            ("one tablespoon", Measure),
            ("a teaspoon", Measure),
            ("50 ppm", Measure),
            ("a drop of soap", EverydayMeasure),
            ("two cups of water", EverydayMeasure),
            ("a spoonful of sugar", EverydayMeasure),
            ("a capful of bleach", EverydayMeasure),
            ("a pinch of baking soda", EverydayMeasure),
            ("a cup or so of milk", EverydayMeasure),
            ("Add a few drops of dish soap.", VagueMeasure),
            ("Add a splash of vinegar.", VagueMeasure),
            ("a squirt of soap", VagueMeasure),
            ("Use it at 5 ml per litre.", Measure),
            ("Use it per litre of water.", Rate),
            ("per liter", Rate),
            ("per gallon", Rate),
            ("Apply 2/l.", Rate),
            ("Use 5 g/l.", Measure),
            ("dilute to /gal", Rate),
            ("Mix at 1:10.", ColonRatio),
            ("Use one part soap to ten parts water.", PartsRatio),
            ("Use 1 part milk to 9 parts water.", PartsRatio),
            ("Mix 1 in 10 with water.", MixtureRatio),
            ("One part neem to ten of water.", MixtureRatio),
            ("Use a 2% soap spray.", Percentage),
            ("A 0.5 percent copper solution.", Percentage),
        ];
        for (text, rule) in cases {
            assert_eq!(check(text), Some(*rule), "{text:?} should fail as {rule:?}");
        }
    }

    #[test]
    fn contract_examples_pass() {
        let cases = [
            "Remove the 2 lowest leaves.",
            "Pick off 3 large leaves.",
            "Expect a drop in temperature tonight.",
            "The leaves curl into a cup.",
            "Cut a part of the stem back to a healthy bud.",
            "Plant one in a pot.",
            "Remove one in three canes.",
            "More than 50% of the leaves are spotted.",
            "Above 90% humidity, the fungus spreads.",
            "Check again in 2 weeks.",
            "Look for new spots every 3 days.",
            "Space them 10 cm apart.",
            "Mulch 30 cm deep around the stem.",
            "The spots are about 2 mm across.",
            "Water early in the morning.",
            "Spray the plant with water to knock off aphids.",
            "Use a product registered for this use and follow its label exactly.",
            "",
        ];
        for text in cases {
            assert_eq!(check(text), None, "{text:?} should pass");
        }
    }

    /// The three known false positives of contracts 7: they fail, and the prompts avoid them.
    #[test]
    fn known_false_positives_fail() {
        assert_eq!(check("Use a 1 gallon sprayer."), Some(DoseRule::Measure));
        assert_eq!(
            check("Water at 10:30 in the morning."),
            Some(DoseRule::ColonRatio)
        );
        assert_eq!(
            check("Spots cover 12% of the leaf, so spray the healthy leaves too."),
            Some(DoseRule::Percentage)
        );
    }

    /// The percentage and `in` rules look at the sentence, not the whole text: a measured share
    /// in a sentence of its own passes, as the prompts tell the model to write it.
    #[test]
    fn conditions_are_per_sentence() {
        assert_eq!(
            check("Spots cover 12% of the leaf. Spray the healthy leaves too."),
            None
        );
        assert_eq!(
            check("Remove one in three canes. Then mix compost into the soil."),
            None
        );
    }

    #[test]
    fn match_borrows_the_words_and_their_span() {
        let text = "Remove spotted leaves. Then mix 2 tbsp of soap into water.";
        let m = dose_check(text, ProductCatalogue::builtin()).unwrap();
        assert_eq!(m.matched, "2 tbsp");
        assert_eq!(&text[m.span.clone()], "2 tbsp");
        assert_eq!(m.sentence, "Then mix 2 tbsp of soap into water.");
        assert!(m.reason().contains("\"2 tbsp\""));
    }

    #[test]
    fn earliest_match_wins_within_a_sentence() {
        let m = dose_check("Mix 1:10, about 2 tbsp.", ProductCatalogue::builtin()).unwrap();
        assert_eq!(m.rule, DoseRule::ColonRatio);
    }

    #[test]
    fn check_all_finds_the_first_failing_text() {
        let texts = ["Remove leaves.", "Spray 2 ml.", "Add 3 g."];
        let m = dose_check_all(texts, ProductCatalogue::builtin()).unwrap();
        assert_eq!(m.matched, "2 ml");
    }
}
