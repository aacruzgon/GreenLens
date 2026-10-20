//! The dose check ([contracts 7], the dose check): plans and chat replies never give an amount
//! of a product. It runs on every plan text, every assistant sentence before it is streamed and
//! every string the model writes into a tool call.
//!
//! Every pattern is compiled once ([`LazyLock`]) and matched case-insensitively. Units are
//! followed by `\b`, so "2 lowest leaves" and "3 large leaves" never read as litres. The rules
//! that depend on the rest of the sentence (ratios with `in` or one `parts`, percentages) are
//! checked per sentence, against the mixture words and the [`ProductCatalogue`].

use std::{ops::Range, sync::LazyLock};

use regex::{Captures, Regex, RegexBuilder, RegexSet, RegexSetBuilder};

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
    /// Two numbers around a colon: "1:10", "1 : 100", "Mix 1: 10". Not a clock time ("at
    /// 18:40", "06:30", "7:00", "6:30 am", "10:30 in the morning") or a label or a date before a
    /// colon ("Bed 3: 2 July", "2026-07-15: 420 bulbs") (EON gap G22).
    ColonRatio,
    /// "<quantity> part(s) … to <quantity> part(s)".
    PartsRatio,
    /// "<quantity> part(s) … to <quantity>" or "<quantity> in <quantity>", in a sentence with a
    /// product name or a mixture word. Not "<quantity> in <year>" ("bed 1 in 2026") or a period
    /// after `in` ("1 in 10 days") (EON gap G22).
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

/// Which matches of a rule are not what the rule is about, checked match by match (EON gap G22).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Exempt {
    /// Every match counts.
    Nothing,
    /// The colon rule: a clock time, or a label or a date before a colon.
    TimeOrLabel,
    /// The `in` rule: a year as the second term, or a period after it.
    YearOrPeriod,
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
    exempt: Exempt,
}

fn ci(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        // reason: the patterns are the constants of this module, covered by the tests below.
        .expect("dose pattern compiles")
}

fn patterns() -> [(DoseRule, String, Condition, Exempt); 9] {
    [
        (
            DoseRule::Measure,
            format!(r"{QUANTITY}\s*-?\s*{MEASURE}\b"),
            Condition::None,
            Exempt::Nothing,
        ),
        (
            DoseRule::EverydayMeasure,
            format!(r"{QUANTITY}\s*-?\s*{EVERYDAY}\b{OF_WITHIN_THREE}"),
            Condition::None,
            Exempt::Nothing,
        ),
        (
            DoseRule::VagueMeasure,
            format!(r"\ba\s+(?:few\s+drops|splash|squirt)\b{OF_WITHIN_THREE}"),
            Condition::None,
            Exempt::Nothing,
        ),
        (
            DoseRule::Rate,
            format!(r"{RATE_PHRASE}{RATE_UNIT}\b|{RATE_BASE}"),
            Condition::QuantityOrStart,
            Exempt::Nothing,
        ),
        (
            DoseRule::ColonRatio,
            r"(?P<l>\d+)(?P<sb>\s*):(?P<sa>\s*)(?P<r>\d+)".to_owned(),
            Condition::None,
            Exempt::TimeOrLabel,
        ),
        (
            DoseRule::PartsRatio,
            format!(r"{RATIO_QUANTITY}\s+parts?\b.*?\bto\s+{RATIO_QUANTITY}\s+parts?\b"),
            Condition::None,
            Exempt::Nothing,
        ),
        (
            DoseRule::MixtureRatio,
            format!(r"{RATIO_QUANTITY}\s+parts?\b.*?\bto\s+{RATIO_QUANTITY}"),
            Condition::Mixture,
            Exempt::Nothing,
        ),
        (
            DoseRule::MixtureRatio,
            format!(r"(?P<l>{RATIO_QUANTITY})\s+in\s+(?P<r>{RATIO_QUANTITY})"),
            Condition::Mixture,
            Exempt::YearOrPeriod,
        ),
        (
            DoseRule::Percentage,
            r"\d+(?:[.,]\d+)?\s*(?:%|percent\b|per\s+cent\b)".to_owned(),
            Condition::Mixture,
            Exempt::Nothing,
        ),
    ]
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    patterns()
        .into_iter()
        .map(|(kind, pattern, condition, exempt)| Rule {
            kind,
            regex: ci(&pattern),
            condition,
            exempt,
        })
        .collect()
});

/// Every rule in one set: a sentence that matches none of them (almost every sentence) is
/// rejected in a single pass before any rule is run on its own.
static ANY: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSetBuilder::new(patterns().map(|(_, p, _, _)| p))
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

/// The words that number a place, a period or an item, so that a number after them and a colon
/// is a label ("Bed 3: 2 July", "Q3: 4 sprays"), not the first term of a ratio: the word, an
/// optional `#`, and the earlier numbers of a list that ends in `and`, `or`, `&` or a dash
/// ("Beds 1, 5 and 6: 2 July", "Rows 2-4: 3 sprays"); "bed 3, 1" is no list. Not `part`, `mix`
/// or `ratio`, which ratios use.
const NUMBERED: &str = r"\b(?:beds?|plots?|rows?|weeks?|days?|steps?|stages?|phases?|zones?|sections?|blocks?|areas?|fields?|tunnels?|greenhouses?|houses?|trays?|pots?|trees?|lines?|items?|entry|entries|notes?|questions?|runs?|rounds?|trials?|samples?|visits?|pages?|tables?|figures?|fig\.?|chapters?|sites?|scans?|photos?|tasks?|q)\s*#?\s*(?:\d+[a-z]?(?:\s*,\s*\d+[a-z]?)*(?:\s*,?\s*(?:and|or|&)\s+|\s*[-–]\s*))?$";

/// The month names and their abbreviations.
const MONTH: &str = r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)";

/// A label before a colon: [`NUMBERED`], or the start of a date that the number ends
/// ("2026-07-15", "15/07/2026", "July 15").
static LABEL_BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    ci(&format!(
        r"{NUMBERED}|(?:\d{{4}}-\d{{1,2}}-|\d{{1,2}}[/.]\d{{1,2}}[/.]|\d{{4}}[/.]\d{{1,2}}[/.]|\b{MONTH}\.?\s+)$"
    ))
});

/// A date that starts after a colon: "2 July", "2nd of July", "02/07/2026", "2026-07-02".
static DATE_AFTER: LazyLock<Regex> = LazyLock::new(|| {
    ci(&format!(
        r"^(?:\d{{1,2}}(?:st|nd|rd|th)?\s+(?:of\s+)?{MONTH}\b|\d{{1,2}}[/.]\d{{1,2}}[/.]\d{{2,4}}\b|\d{{4}}-\d{{1,2}}-\d{{1,2}}\b)"
    ))
});

/// A period after the second term of `<n> in <n>`: "1 in 10 days".
static PERIOD_AFTER: LazyLock<Regex> = LazyLock::new(|| {
    ci(r"^\s+(?:days?|weeks?|months?|years?|hours?|hrs?|minutes?|mins?|seasons?|nights?)\b")
});

/// What makes `H:MM` a clock time after it: `am`, `pm`, `o'clock`, `h`, `hrs`, `UTC`, `GMT`.
static CLOCK_AFTER: LazyLock<Regex> =
    LazyLock::new(|| ci(r"^\s*(?:a\.?\s?m\b|p\.?\s?m\b|o['’]?clock\b|h\b|hrs?\b|utc\b|gmt\b)"));

/// A part of the day after `H:MM`: "10:30 in the morning", "7:15 tonight".
static DAYTIME_AFTER: LazyLock<Regex> = LazyLock::new(|| {
    ci(
        r"^\s*(?:in\s+the\s+(?:morning|afternoon|evening)|(?:this|tomorrow|yesterday|each|every)\s+(?:morning|afternoon|evening|night)|tonight|at\s+night)\b",
    )
});

/// What makes a 24-hour `H:MM` a clock time before it: a word that introduces a time ("at
/// 18:40", "from 18:40 to 19:10", "18:40–19:10") or a date ("2 July 18:40", "July 2, 18:40").
static TIME_BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    ci(&format!(
        r"(?:\b(?:at|by|from|until|till|before|after|around|about|between|since|to|and|or|past)\s+|[-–]\s*|(?:\b\d{{1,2}}(?:st|nd|rd|th)?\s+{MONTH}|\b{MONTH}\.?\s+\d{{1,2}}(?:st|nd|rd|th)?|\d{{4}}-\d{{2}}-\d{{2}}),?\s+)$"
    ))
});

/// An ISO date and `T` just before a time: "2026-07-15T18:40:00Z".
static ISO_DATE_BEFORE: LazyLock<Regex> = LazyLock::new(|| ci(r"\d{4}-\d{2}-\d{2}t$"));

/// A colon and a number just before `H:MM`, as in the last two of "20:20:20" (a fertiliser's
/// analysis).
static COLON_BEFORE: LazyLock<Regex> = LazyLock::new(|| ci(r"\d\s*:\s*$"));

/// A colon and a number just after `H:MM`, as in "20:20:20" or "18:40:00".
static COLON_AFTER: LazyLock<Regex> = LazyLock::new(|| ci(r"^\s*:\s*\d"));

/// Whether `n` is a year that no dilution reads as: four digits from 1901 to 2099 that are not
/// a multiple of 100, so "1 in 2026" is a year and "1 in 2000" stays a ratio.
fn is_year(n: &str) -> bool {
    n.len() == 4
        && (n.starts_with("19") || n.starts_with("20"))
        && !n.ends_with("00")
        && n.bytes().all(|b| b.is_ascii_digit())
}

/// Whether the colon rule's `H:MM` match is a clock time (EON gap G22): no space around the
/// colon, `H` one or two digits up to 23, `MM` two digits up to 59, not one of three
/// colon-separated numbers ("20:20:20", "18:40:00") unless after an ISO date and `T`, and one
/// of: after an ISO date and `T`; a leading zero ("06:30"); `MM` `00` ("7:00"); `am`, `pm`, `o'clock`,
/// `h`, `UTC` or `GMT` after it ("6:30 am"); `H` from 13 with a time word or a date before it
/// ("at 18:40", "2 July 18:40"); `H` from 6 to 12 with a part of the day after it ("10:30 in
/// the morning"). A ratio's first term is small and has no leading zero, so "1:10", "at 1:20 in
/// the evening" and "a 20:20 feed" stay ratios.
fn is_clock_time(before: &str, l: &str, r: &str, after: &str) -> bool {
    let (Ok(hour), Ok(minute)) = (l.parse::<u32>(), r.parse::<u32>()) else {
        return false;
    };
    if l.len() > 2 || r.len() != 2 || hour > 23 || minute > 59 {
        return false;
    }
    if l.len() == 2 && ISO_DATE_BEFORE.is_match(before) {
        return true;
    }
    if COLON_BEFORE.is_match(before) || COLON_AFTER.is_match(after) {
        return false;
    }
    (l.len() == 2 && l.starts_with('0'))
        || minute == 0
        || CLOCK_AFTER.is_match(after)
        || (hour >= 13 && TIME_BEFORE.is_match(before))
        || ((6..=12).contains(&hour) && DAYTIME_AFTER.is_match(after))
}

/// The colon rule's exemptions (EON gap G22): a clock time ([`is_clock_time`]), or a label: a
/// number, no space, a colon and a space or more, where the number ends a label or a date or is
/// a year ("Bed 3: 2 July", "2026-07-15: 420 bulbs", "2026: 3 sprays"), or a date starts after
/// the colon ("…: 2 July"). Returns where to look for the next match, or `None` when this match
/// counts: after a time, or at the second number after a label, so "Bed 3: 1:10" still fails.
fn colon_exempt(sentence: &str, caps: &Captures<'_>) -> Option<usize> {
    let (l, r) = (caps.name("l")?, caps.name("r")?);
    let tight = caps.name("sb").is_none_or(|m| m.is_empty());
    let spaced = caps.name("sa").is_some_and(|m| !m.is_empty());
    let before = &sentence[..l.start()];
    if tight && !spaced && is_clock_time(before, l.as_str(), r.as_str(), &sentence[r.end()..]) {
        return Some(r.end());
    }
    let label = tight
        && spaced
        && (LABEL_BEFORE.is_match(before)
            || is_year(l.as_str())
            || DATE_AFTER.is_match(&sentence[r.start()..]));
    label.then_some(r.start())
}

/// The `in` rule's exemptions (EON gap G22): a year as the second term ("bed 1 in 2026",
/// "beds 1, 5 or 6 in 2026") or a period after it ("spray again 1 in 14 days"). Returns where to
/// look for the next match (the second term), or `None` when this match counts.
fn in_exempt(sentence: &str, caps: &Captures<'_>) -> Option<usize> {
    let r = caps.name("r")?;
    let exempt = is_year(r.as_str()) || PERIOD_AFTER.is_match(&sentence[r.end()..]);
    exempt.then_some(r.start())
}

/// The first match of a rule in a sentence that its exemptions leave.
fn first_match(rule: &Rule, sentence: &str) -> Option<Range<usize>> {
    let exempt = match rule.exempt {
        Exempt::Nothing => return rule.regex.find(sentence).map(|m| m.range()),
        Exempt::TimeOrLabel => colon_exempt,
        Exempt::YearOrPeriod => in_exempt,
    };
    let mut at = 0;
    while let Some(caps) = rule.regex.captures_at(sentence, at) {
        match exempt(sentence, &caps) {
            Some(next) => at = next,
            None => return caps.get(0).map(|m| m.range()),
        }
    }
    None
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
        .filter_map(|(_, r)| first_match(r, sentence).map(|m| (r, m)))
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

    /// Dates, years, clock times and labels with a colon pass (EON gap G22): the sentences E8.3's
    /// third run most likely replaced, and the forms the farm records use.
    #[test]
    fn dates_times_and_labels_pass() {
        let cases = [
            // `<n> in <year>` and `<n> in <n> <period>`.
            "Sprayed bed 1 in 2026.",
            "No copper on bed 1 in 2026.",
            "Spray log 2026 shows no sprays on bed 1 in 2026.",
            "The spray log has no sprays on beds 1, 5 or 6 in 2026.",
            "We sprayed 3 in 2025 and 4 in 2026.",
            "One in 2026 and two in 2025 were neem sprays.",
            "Spray bed 2 in 10 days if the spots spread.",
            "Spray again 1 in 14 days.",
            // A label or a date, a colon and a space.
            "Bed 3: 2 July",
            "Maria sprayed copper on bed 3: 2 July and 16 July.",
            "Bed 3: 2 July and 16 July (Kocide).",
            "2026-07-15: 420 bulbs",
            "2026-07-15: 420 bulbs planted in bed 4.",
            "Q3: 4 sprays on bed 3.",
            "Week 3: 2 sprays of copper on bed 3.",
            "Step 2: 3 rows of lettuce.",
            "Row 2: 15 plants, row 3: 14 plants.",
            "Beds 1 and 2: 6 sprays in all.",
            "Zone 2: 3 sprays of neem in June.",
            "July 15: 2 sprays of copper.",
            "15/07/2026: 3 copper sprays.",
            "2026: 3 sprays on bed 3.",
            "In 2025: 4 copper sprays on bed 2.",
            "Item 1: 10 copper sprays were logged.",
            // Clock times.
            "On 2 July at 18:40 Maria sprayed bed 3.",
            "Sprayed Kocide at 19:30 on 2 July.",
            "The spray went on from 18:40 to 19:10.",
            "Riley posted at 09:15.",
            "Spray before 06:30, while it is calm.",
            "Spray at 6:30 am before the bees fly.",
            "Maria sprayed at 7:45 pm.",
            "Spray copper between 7:00 and 9:00 in the morning.",
            "Water at 10:30 in the morning.",
            "Sprayed bed 3 at 10:30 in the morning.",
            "Water the beds at 7:15 tonight.",
            "The sync ran at 2026-07-15T18:40:00Z.",
            "Spray at 5 a.m. or after 20:00.",
        ];
        for text in cases {
            assert_eq!(check(text), None, "{text:?} should pass");
        }
    }

    /// Ratios with a colon or `in` still fail beside the G22 exemptions: a ratio's first term is
    /// small and has no leading zero, a space before the colon or none after it is no label, and
    /// the search goes on after an exempt time or label.
    #[test]
    fn ratios_beside_dates_and_times_fail() {
        use DoseRule::*;
        let cases: &[(&str, DoseRule)] = &[
            ("1:10", ColonRatio),
            ("mix 1:4 with water", ColonRatio),
            ("a 1:1 mix", ColonRatio),
            ("3:1 water to vinegar", ColonRatio),
            ("1 : 100", ColonRatio),
            ("Mix 1: 10 with water.", ColonRatio),
            ("Use it at 1:10 with water.", ColonRatio),
            ("Apply at 1:20 in the evening.", ColonRatio),
            ("Spray copper 1:10 in the morning.", ColonRatio),
            ("Use neem at 2:30 in the morning.", ColonRatio),
            ("Use 2:15.", ColonRatio),
            ("A 20:20:20 feed.", ColonRatio),
            ("Use 10:10:10.", ColonRatio),
            ("Mix at 10:1.", ColonRatio),
            ("Bed 3: 1:10 copper.", ColonRatio),
            ("Beds 1 and 2: 1:10 neem.", ColonRatio),
            ("Bed 3: mix 1: 10.", ColonRatio),
            ("Kocide: 1: 10.", ColonRatio),
            ("2026-07-15: 1:10 copper.", ColonRatio),
            ("2026-07-15 1:10 copper on bed 3.", ColonRatio),
            ("2026-07-15T1:10 copper on bed 3.", ColonRatio),
            ("Spray at 6:30 am with a 1:10 mix.", ColonRatio),
            ("At 18:40 Maria mixed 1:10.", ColonRatio),
            ("Spray 1:10 on 2 July at 18:40.", ColonRatio),
            ("Dilute 1:1000.", ColonRatio),
            ("Mix 1 in 10 with water.", MixtureRatio),
            ("dilute 1 in 50", MixtureRatio),
            ("Dilute 2 in 1000.", MixtureRatio),
            ("Mix copper 1 in 2000 with water.", MixtureRatio),
            ("Use a 1 in 1900 dilution of copper.", MixtureRatio),
            ("Mix copper 1 in 2000 in 2026.", MixtureRatio),
            ("In 2026 mix 1 in 10.", MixtureRatio),
            ("Spray bed 3, 1 in 10.", MixtureRatio),
            ("Dilute the neem for day 1 in 10 parts water.", MixtureRatio),
            ("Dilute 1 in 10 in 14 days.", MixtureRatio),
            ("Mix one in ten with water.", MixtureRatio),
        ];
        for (text, rule) in cases {
            assert_eq!(check(text), Some(*rule), "{text:?} should fail as {rule:?}");
        }
    }

    /// The known false positives of contracts 7: they fail. The prompts avoid the first three.
    #[test]
    fn known_false_positives_fail() {
        assert_eq!(check("Use a 1 gallon sprayer."), Some(DoseRule::Measure));
        // A 12-hour time with nothing after it that says so, a time with seconds, a 24-hour time
        // with no time word or date before it, and a label with no space after its colon (EON
        // gap G22).
        for text in [
            "Sprayed at 9:15.",
            "The spray started at 18:40:00.",
            "Bed 3 sprayed, 18:40.",
            "Bed 3:2 July",
        ] {
            assert_eq!(check(text), Some(DoseRule::ColonRatio), "{text:?}");
        }
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
