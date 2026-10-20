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

/// A quantity in the measure rules: a number (`2`, `0.5`, `1/2`, `1 1/2`, `½`, `⅜`, the `2` of
/// `2x`) or a number word (`one` … `twenty`, `thirty` … `ninety`, `hundred`, `thousand`, `half`,
/// `a`, `an`).
const QUANTITY: &str = r"(?:(?:\d+(?:[.,]\d+)?(?:\s+\d+\s*/\s*\d+)?|\d+\s*/\s*\d+|\d*[½¼¾⅓⅔⅛⅕⅖⅗⅘⅙⅚⅜⅝⅞⅐⅑⅒])|\b(?:one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety|hundred|thousand|half|a|an)\b)";

/// What else counts as an amount beside a rate's unit phrase: vague, fractional and multiplying
/// words, plural measure nouns ("capfuls per litre"), and a rate taken from a record or a habit
/// ("the same rate", "the logged rate", "last week's dose"). Not `many`, which refusals use
/// ("how many grams go in each litre"), and not the label's rate, which is what the advice says
/// to follow.
const RATE_AMOUNT: &str = r"\b(?:several|some|dozens?|double|twice|triple|thrice|quarters?|thirds?|tenths?|few|couple|pair|handful|hundreds|thousands|lots|plenty|capfuls|caps|scoops|drops|cups|spoons|teaspoons|tablespoons|tsps|tbsps|handfuls|sachets|tablets|pumps|squirts)\b|\b(?:same|usual|normal|previous|logged|recorded|log['’]s|last\s+[\p{L}]+['’]s)\s+(?:rate|dose|dosage|amount)s?\b";

/// A quantity in the ratio rules: a number or `one` … `twenty`, never `a`, `an` or `half`.
const RATIO_QUANTITY: &str = r"(?:\d+(?:[.,]\d+)?|\b(?:one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|twenty)\b)";

/// The measures that always count after a quantity.
const MEASURE: &str = r"(?:ml|millilitres?|milliliters?|l|litres?|liters?|fl\.?\s*oz|oz|ounces?|pints?|quarts?|gallons?|gal|g|grams?|kg|mg|lbs?|pounds?|tbsp|tablespoons?|tsp|teaspoons?|ppm)";

/// The everyday words that count only when `of` follows within three words.
const EVERYDAY: &str = r"(?:drops?|cups?|spoonfuls?|capfuls?|pinch(?:es)?)";

/// "…of within three words": `of` is one of the next three words.
const OF_WITHIN_THREE: &str = r"(?:\s+[\p{L}'’-]+){0,2}\s+of\b";

/// The volumes a rate is given per, with the abbreviations `ℓ`, `ltr`, `lt`, `qt` and `pt` (EON
/// gap G19) and an optional `US`, `UK` or `imperial` before them.
const RATE_UNIT: &str = r"(?:(?:us|u\.s\.|uk|imperial)\s+)?(?:litres?|liters?|l|ℓ|ltrs?|lt|gallons?|gal|quarts?|qts?|pints?|pts?)";

/// A rate's unit phrase: `per`, `each` or `every` and a volume, or `/` and a volume. It fails a
/// sentence that also holds a [`QUANTITY`] or a [`RATE_AMOUNT`] anywhere, or where at most three
/// words come before it (EON gap G13).
const RATE_PHRASE: &str = r"(?:\b(?:per|each|every)\s+|/\s*)";

/// What a rate is given per beside a volume (EON gap G19). After `per`: a field measure
/// (`hectare(s)`, `ha`, `acre(s)`, optionally after a number, a fraction, `half` or `quarter`), a
/// container (`tank(s)`, `spray tank(s)`, `tankful(s)`, `tankload(s)`, `backpack(s)`,
/// `knapsack(s)`, `sprayer(s)`, `jerry can(s)`, `bucket(s)`, `watering can(s)`), or an area with a
/// number ("100 m²", "100 m2", "10 square metres", "1,000 sq ft"). After `/`: only a field measure
/// with no number before it or an area with a number, so "1/2 acre" and "rainwater/tank water"
/// pass. Not `each` or `every`, which garden advice uses ("water each plant", "after every
/// tank"), and not an area without a number, which is spacing ("two plants per square metre",
/// "four lettuces per square foot").
const RATE_BASE: &str = r"\bper\s+(?:(?:\d+\s*/\s*\d+\s*-?\s*|\d[\d.,]*\s*-?\s*|[½¼¾]\s*|(?:half|quarter)(?:\s+an?)?\s*-?\s*)?(?:hectares?|ha|acres?)\b|(?:spray\s+)?(?:tanks?|tankfuls?|tankloads?|backpacks?|knapsacks?|sprayers?|jerry[\s-]*cans?|buckets?|watering[\s-]+cans?)\b|(?:\d[\d.,]*|(?:a\s+)?(?:ten|hundred|thousand))\s*(?:m²|m2\b|ft²|ft2\b|sqm\b|sq\b\.?(?:\s*(?:m|ft|feet|foot|yds?|yards?|metres?|meters?)\b)?|square\s+(?:metres?|meters?|feet|foot|yards?)\b))|/\s*(?:hectares?|ha|acres?)\b|/\s*\d[\d.,]*\s*(?:m²|m2\b|ft²|ft2\b|sqm\b|sq\b\.?(?:\s*(?:m|ft|feet|foot)\b)?)";

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
    /// `per`, `each` or `every` and a volume, or `/` and a volume, in a sentence that holds a
    /// quantity anywhere, or where at most three words come before that phrase: "a capful per
    /// litre", "Per litre, add two scoops.", "2/l", "Use capfuls/L.", "Do this per litre of
    /// water." after "Add two capfuls.". A sentence with the unit, no quantity and four or more
    /// words before it ("I can't give you the amount per litre.") passes. `per` or `/` before a
    /// field measure, a container or an area with a number counts as a unit phrase too: "a bag
    /// per acre", "Use a capful per tank.", "a scoop per 100 m²" (EON gap G19).
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

/// A [`QUANTITY`] or a [`RATE_AMOUNT`] anywhere: the condition of the rate rule.
static ANY_QUANTITY: LazyLock<Regex> = LazyLock::new(|| ci(&format!("{QUANTITY}|{RATE_AMOUNT}")));

/// What may come before a rate's unit phrase for it to count without a quantity: list markers
/// and at most three words, as when a sentence break splits a rate from its amount ("Add two
/// capfuls. Do this per litre.", "a\ncapful per litre").
static LINE_START: LazyLock<Regex> = LazyLock::new(|| ci(r"^[\s*•·>\-–—]*(?:[^\s/]+\s*){0,3}$"));

impl DoseMatch<'_> {
    /// Whether the sentence holds a [`QUANTITY`] or a [`RATE_AMOUNT`] ("2", "two", "a", "some",
    /// "capfuls"). A `Rate` match without one is a unit phrase with at most three words before
    /// it, as after a sentence break; logged without the text (EON gap G13).
    pub fn has_quantity(&self) -> bool {
        ANY_QUANTITY.is_match(self.sentence)
    }

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

/// What else a rule's sentence must hold for its match to count.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Condition {
    /// Nothing: the match alone fails the sentence.
    None,
    /// A product name or a mixture word.
    Mixture,
    /// An amount anywhere, or at most three words before the match (the rate rule).
    QuantityOrStart,
}

/// One compiled rule.
struct Rule {
    kind: DoseRule,
    regex: Regex,
    condition: Condition,
}

fn ci(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        // reason: the patterns are the constants of this module, covered by the tests below.
        .expect("dose pattern compiles")
}

fn patterns() -> [(DoseRule, String, Condition); 9] {
    [
        (
            DoseRule::Measure,
            format!(r"{QUANTITY}\s*-?\s*{MEASURE}\b"),
            Condition::None,
        ),
        (
            DoseRule::EverydayMeasure,
            format!(r"{QUANTITY}\s*-?\s*{EVERYDAY}\b{OF_WITHIN_THREE}"),
            Condition::None,
        ),
        (
            DoseRule::VagueMeasure,
            format!(r"\ba\s+(?:few\s+drops|splash|squirt)\b{OF_WITHIN_THREE}"),
            Condition::None,
        ),
        (
            DoseRule::Rate,
            format!(r"{RATE_PHRASE}{RATE_UNIT}\b|{RATE_BASE}"),
            Condition::QuantityOrStart,
        ),
        (
            DoseRule::ColonRatio,
            r"\d+\s*:\s*\d+".to_owned(),
            Condition::None,
        ),
        (
            DoseRule::PartsRatio,
            format!(r"{RATIO_QUANTITY}\s+parts?\b.*?\bto\s+{RATIO_QUANTITY}\s+parts?\b"),
            Condition::None,
        ),
        (
            DoseRule::MixtureRatio,
            format!(r"{RATIO_QUANTITY}\s+parts?\b.*?\bto\s+{RATIO_QUANTITY}"),
            Condition::Mixture,
        ),
        (
            DoseRule::MixtureRatio,
            format!(r"{RATIO_QUANTITY}\s+in\s+{RATIO_QUANTITY}"),
            Condition::Mixture,
        ),
        (
            DoseRule::Percentage,
            r"\d+(?:[.,]\d+)?\s*(?:%|percent\b|per\s+cent\b)".to_owned(),
            Condition::Mixture,
        ),
    ]
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    patterns()
        .into_iter()
        .map(|(kind, pattern, condition)| Rule {
            kind,
            regex: ci(&pattern),
            condition,
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
            r.condition != Condition::Mixture
                || *mixture.get_or_insert_with(|| mixture_context(sentence, products))
        })
        .filter_map(|(_, r)| r.regex.find(sentence).map(|m| (r, m.range())))
        .filter(|(r, m)| {
            r.condition != Condition::QuantityOrStart
                || ANY_QUANTITY.is_match(sentence)
                || LINE_START.is_match(&sentence[..m.start])
        })
        .map(|(r, m)| (r.kind, m))
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
            ("Use 5 g/l.", Measure),
            ("Use 5 ml/l.", Measure),
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

    /// The rate rule (EON gap G13): a unit phrase fails beside an amount anywhere in the
    /// sentence, or with at most three words before it. Each case is a rate the earlier rules
    /// or an audit found; some fail first as another rule.
    #[test]
    fn rates_fail() {
        use DoseRule::*;
        let cases: &[(&str, DoseRule)] = &[
            ("Use 25 g per litre.", Measure),
            ("Add a capful per litre.", Rate),
            ("Add half a cup per gallon.", Rate),
            ("Use two tablespoons per gallon.", Measure),
            ("Two scoops per liter.", Rate),
            ("two scoops of it per liter", Rate),
            // More than three words between the quantity and the unit.
            ("Add a scoop of the product per litre of water.", Rate),
            ("add two scoops of the powder to the water per litre", Rate),
            ("two scoops of the copper product per gallon", Rate),
            // The unit first, or after punctuation.
            ("Per litre, add a capful.", Rate),
            ("per litre: a capful", Rate),
            ("a capful (per litre)", Rate),
            ("a capful—per litre", Rate),
            // Quantity words beyond twenty, vague amounts and multiples.
            ("thirty capfuls per litre", Rate),
            ("fifty capfuls per gallon", Rate),
            ("hundred per litre", Rate),
            ("several capfuls per litre", Rate),
            ("some powder per litre", Rate),
            ("double the amount per litre", Rate),
            ("twice the label rate per litre", Rate),
            ("2x per litre", Rate),
            // A line break splits the sentence: the unit starts the second one.
            ("a capful\nper litre", Rate),
            ("Kocide: a capful\nper litre of water.", Rate),
            ("per liter", Rate),
            ("per gallon", Rate),
            ("a\ncapful per litre", Rate),
            ("fifty grams", Measure),
            ("Add thirty ml of Kocide.", Measure),
            // Fraction glyphs, fraction and vague words, plural measure nouns, multiples.
            ("Use ⅜ cup per litre.", Rate),
            ("Use quarter cup per litre.", Rate),
            ("Add few caps per litre.", Rate),
            ("Add dozens of drops per litre.", Rate),
            ("Triple the label rate per litre.", Rate),
            ("Use lots of soap per gallon.", Rate),
            ("Use capfuls/L.", Rate),
            ("ADD CAPFULS PER GALLON.", Rate),
            // A rate taken from a record or a habit.
            ("Use the same rate per litre as last time.", Rate),
            ("Use the log's dose per litre.", Rate),
            ("Repeat last week's rate per litre.", Rate),
            // The amount in one sentence, the unit in the next.
            ("Add two capfuls. Do this per litre of water.", Rate),
            ("Add a capful. That is per litre.", Rate),
            ("Add two capfuls.\nThat is per litre.", Rate),
            ("Two capfuls. Mix that per litre.", Rate),
            ("Use the product at 2. Rate is per litre of water.", Rate),
            ("Use it per litre of water.", Rate),
            ("dilute to /gal", Rate),
            ("Add a capful to every litre.", Rate),
            ("One scoop for each gallon.", Rate),
            ("Add a scoop of product per litre of water.", Rate),
            ("a capful per quart", Rate),
            ("½ capful per pint", Rate),
            ("Apply 2/l.", Rate),
            ("Apply 3 capfuls/gal.", Rate),
        ];
        for (text, rule) in cases {
            assert_eq!(check(text), Some(*rule), "{text:?} should fail as {rule:?}");
        }
    }

    /// Rates per area or container, and the volume abbreviations (EON gap G19): `per` or `/`
    /// before a field measure, a container or an area with a number is a unit phrase.
    #[test]
    fn area_and_container_rates_fail() {
        let cases = [
            "a bag per acre",
            "a scoop per 2 acres",
            "a capful per hectare",
            "two scoops per ha",
            "a capful/ha",
            "a scoop per 100 m²",
            "a scoop per 100 m2",
            "a capful per 10 m² of bed",
            "two scoops per 10 square metres",
            "a scoop per 100 square feet",
            "a bag per 1,000 sq ft",
            "Use a bag per 100 sq. ft.",
            "Use a bag per 1000 sq. m of lawn.",
            "a bag per half an acre",
            "a bag per ½ acre",
            "a scoop per quarter hectare",
            "Use a capful per tank.",
            "a capful per spray tank",
            "a capful per tankload",
            "a sachet per knapsack",
            "a capful per sprayer",
            "a capful per jerry can",
            "a capful per watering-can",
            "a capful per tankful",
            "Per tank, add a capful.",
            "three capfuls per backpack",
            "Add three scoops of Kocide per backpack.",
            "a capful per bucket",
            "a capful per watering can",
            "a capful per ℓ",
            "a capful/ℓ",
            "a capful per ltr",
            "a capful per lt",
            "a capful per qt",
            "a capful per pt",
            "a capful per US gallon",
        ];
        for text in cases {
            assert_eq!(check(text), Some(DoseRule::Rate), "{text:?} should fail");
        }
    }

    /// What G19's widening leaves alone: spacing, a field's or a bed's size, `each` and `every`
    /// before a container, and a refusal with no amount.
    #[test]
    fn spacing_and_sizes_pass() {
        let cases = [
            "Space the plants 30 cm apart, two per square metre.",
            "Plant four lettuces per square foot.",
            "Plant one seedling per pot.",
            "Plant one tomato per pot.",
            "Water each bed per week",
            "Mulch a 5 cm layer per bed",
            "Water each plant at the base.",
            "Hang one sticky trap per square metre of greenhouse.",
            "Thin to one plant per 30 cm of row.",
            "Rinse the sprayer after each tank.",
            "Mix a fresh tank each time you spray.",
            "Keep a log for each tank you mix.",
            "Fill the watering can with rainwater.",
            "The farm covers 12 hectares.",
            "The beds cover about 100 m² in all.",
            "Walk the field once per acre block.",
            // `/` takes no number before a field measure and no container (audit of G19).
            "Your plot is 1/2 acre.",
            "A 1/4 acre lot is plenty for a family.",
            "The field is 3/4 hectare.",
            "Use rainwater/tank water if you can.",
            "Water from a can/bucket at the base.",
            "Two buckets of compost per bed are enough.",
            "Two plants per m² is enough.",
            "I can't give you the rate per hectare.",
            "I can't give the amount per tank; follow the label.",
        ];
        for text in cases {
            assert_eq!(check(text), None, "{text:?} should pass");
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
            "Two plants per square metre is enough.",
            "Mulch 30 cm deep around the stem.",
            "The spots are about 2 mm across.",
            "Water early in the morning.",
            "Spray the plant with water to knock off aphids.",
            "Use a product registered for this use and follow its label exactly.",
            // A unit in a sentence with no amount and four or more words before it (EON gap
            // G13: the first two are refusals E8.3 replaced).
            "I can't tell you how much Kocide went into the backpack per litre.",
            "I can't give the amount per litre.",
            "I can't give you the amount per litre.",
            "I can't say how many grams go in each litre.",
            "Follow the label's rate per litre.",
            "Use the label rate per litre.",
            "Maria logged the rate in g/L for each spray.",
            "The label gives the rate in g/l or ml/l.",
            // Quantity words that count only beside a rate's unit.
            "Collect some litres of rainwater.",
            "Water with several gallons each week.",
            "",
        ];
        for text in cases {
            assert_eq!(check(text), None, "{text:?} should pass");
        }
    }

    /// The known false positives of contracts 7: they fail. The prompts avoid the first three.
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
        assert_eq!(
            check("A drop in temperature of 5 °C."),
            Some(DoseRule::EverydayMeasure)
        );
        // A rate's unit in a sentence with any quantity: an article, a number in a product
        // name, `some`.
        for text in [
            "I can't tell you how much Kocide went into a backpack per litre.",
            "I can't give you an amount per litre.",
            "How much Kocide 3000 to use per litre is on the label.",
            "Some growers ask about the rate per litre.",
            // At most three words before the unit, no amount.
            "Rates per litre are on the label.",
            "DiPel's rate per litre is on its label.",
            // A count per field measure or per area with a number: spacing and sampling or trap
            // density (EON gap G19).
            "Scout a few rows per hectare.",
            "Set one trap per acre.",
            "One tree per 20 m².",
            "Run one sprinkler per 100 square feet.",
            "Take one soil sample per hectare.",
            "Keep one hive per acre.",
            // A volume abbreviation as a URL's path segment, in a sentence with an amount.
            "See https://example.org/pt/garden for a guide.",
        ] {
            assert_eq!(check(text), Some(DoseRule::Rate), "{text:?}");
        }
    }

    /// The known misses of contracts 7 (EON gap G19): a rate per plant, per length of row or per
    /// area without a number reads like spacing ("two plants per square metre"), so it passes.
    #[test]
    fn known_misses_pass() {
        for text in [
            "a capful per plant",
            "a capful per square foot",
            "use a capful per 10 m of row",
        ] {
            assert_eq!(check(text), None, "{text:?}");
        }
    }

    /// `has_quantity` (EON gap G13's log) is whether the sentence holds an amount, so a rate
    /// caught only because at most three words come before its unit logs `false`.
    #[test]
    fn has_quantity_reads_the_sentence() {
        let products = ProductCatalogue::builtin();
        let quantity = |text: &str| dose_check(text, products).map(|m| m.has_quantity());
        assert_eq!(quantity("Add a capful per litre."), Some(true));
        assert_eq!(quantity("Add two capfuls per litre."), Some(true));
        assert_eq!(quantity("Apply 2/l."), Some(true));
        assert_eq!(quantity("Add ½ capful per litre."), Some(true));
        assert_eq!(quantity("Then\nper litre of water."), Some(false));
        assert_eq!(quantity("- per gallon"), Some(false));
        assert_eq!(quantity("a\ncapful per litre"), Some(false));
        assert_eq!(
            quantity("Add two capfuls.\nThat is per litre."),
            Some(false)
        );
        assert_eq!(quantity("Use capfuls/L."), Some(true));
        assert_eq!(quantity("I can't give you the amount per litre."), None);
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
