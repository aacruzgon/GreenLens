//! `answers.toml`, format 1: the evaluation's questions, their expected facts and the records
//! they must cite (the fixture's README, `answers.toml`).

use serde::Deserialize;

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answers {
    /// `1`.
    pub format: u32,
    /// `"maple-row"`.
    pub fixture: String,
    /// The day the evaluation runs at, `YYYY-MM-DD`.
    #[expect(
        dead_code,
        reason = "part of the format; garden.json's as_of pins the clock"
    )]
    pub as_of: String,
    /// The farm's IANA time zone.
    #[expect(dead_code, reason = "part of the format; garden.json's garden has it")]
    pub time_zone: String,
    /// What the safety and injection measures look for.
    pub safety: Safety,
    /// The questions, in order.
    #[serde(rename = "question")]
    pub questions: Vec<Question>,
}

/// `[safety]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Safety {
    /// A spray rate, as a regular expression.
    pub rate_pattern: String,
    /// Phrases only following the planted injection would produce.
    pub injection_markers: Vec<String>,
}

/// One `[[question]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    /// `Q01` …
    pub id: String,
    /// The measure it is scored on.
    pub measure: Measure,
    /// The question as the user types it.
    pub ask: String,
    /// Chat (absent) or the plan writer's plan.
    #[serde(default)]
    pub target: Target,
    /// What kind of fact the answer is.
    pub kind: Kind,
    /// The expected fact; empty for `refusal` and `behaviour`.
    pub answer: String,
    /// Equivalent spellings.
    #[serde(default)]
    pub accept: Vec<String>,
    /// The unit of a count, with alternatives separated by `|` (`"applications|sprays"`). A
    /// number counts as the answer only when the unit follows it within a word or two.
    #[serde(default)]
    pub unit: Option<String>,
    /// Facts that must also appear, each with its alternatives separated by `|`.
    #[serde(default)]
    pub must_include: Vec<String>,
    /// How a person scores a `refusal` or `behaviour` answer.
    #[serde(default)]
    pub rubric: Option<String>,
    /// The records to cite.
    pub cite: Vec<Cite>,
    /// Whether every record or one of them.
    pub cite_mode: CiteMode,
    /// A rate in the answer fails the safety measure.
    #[serde(default)]
    pub no_rate: bool,
    /// Whether the injection markers apply (`Some(false)`: the rubric scores it).
    #[serde(default)]
    pub markers: Option<bool>,
    /// Why the answer is what it is.
    #[serde(default)]
    #[expect(
        dead_code,
        reason = "part of the format; for people reading the results"
    )]
    pub note: Option<String>,
}

/// connectors 9.3's measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Measure {
    /// The expected fact.
    Answer,
    /// The cited record.
    Citation,
    /// A fact that needs both Slack and Drive.
    CrossSource,
    /// No rate.
    Safety,
    /// The planted instruction is not followed.
    Injection,
}

impl Measure {
    /// As `answers.toml` spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Answer => "answer",
            Self::Citation => "citation",
            Self::CrossSource => "cross-source",
            Self::Safety => "safety",
            Self::Injection => "injection",
        }
    }
}

/// What is scored: a chat answer or a plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// A chat answer.
    #[default]
    Chat,
    /// The plan writer's plan (E5.4).
    Plan,
}

/// The expected fact's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A day, `YYYY-MM-DD`.
    Date,
    /// A number.
    Count,
    /// A bed number.
    Bed,
    /// A product name.
    Product,
    /// A disease or pest.
    Problem,
    /// `yes` or `no`.
    YesNo,
    /// Declines; scored by the rubric.
    Refusal,
    /// Behaves as the rubric says.
    Behaviour,
}

/// A record to cite.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum Cite {
    /// A Slack message or thread, by its parent's `ts`.
    Slack {
        /// The channel id.
        channel: String,
        /// The message's `ts`.
        ts: String,
    },
    /// A Drive file.
    Drive {
        /// The file id.
        file: String,
    },
    /// A memory of `garden.json`.
    Memory {
        /// Its key.
        key: String,
    },
    /// A scan of `garden.json`.
    Scan {
        /// Its key.
        key: String,
    },
}

/// `all` or `any`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiteMode {
    /// Every record.
    All,
    /// One of them.
    Any,
}

impl Question {
    /// Whether it can only be answered or cited with Drive's records: every cross-source
    /// question, and a question whose required citations are all on Drive. An injection
    /// question never needs Drive: its planted message is in Slack, and without Drive its
    /// Drive citation is simply not applicable.
    pub fn needs_drive(&self) -> bool {
        if self.measure == Measure::Injection {
            return false;
        }
        if self.measure == Measure::CrossSource {
            return true;
        }
        let drive = self
            .cite
            .iter()
            .filter(|c| matches!(c, Cite::Drive { .. }))
            .count();
        match self.cite_mode {
            CiteMode::All => drive > 0,
            CiteMode::Any => drive > 0 && drive == self.cite.len(),
        }
    }
}

impl Answers {
    /// Parses and checks the format.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let answers: Self = toml::from_str(text)?;
        anyhow::ensure!(
            answers.format == 1,
            "answers.toml format {}",
            answers.format
        );
        regex::Regex::new(&answers.safety.rate_pattern)?;
        Ok(answers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/era/maple-row/answers.toml");

    #[test]
    fn the_fixture_parses() {
        let answers = Answers::parse(FIXTURE).unwrap();
        assert_eq!(answers.questions.len(), 19);
        assert_eq!(answers.fixture, "maple-row");
    }

    #[test]
    fn the_drive_questions_are_the_cross_source_and_drive_only_ones() {
        let answers = Answers::parse(FIXTURE).unwrap();
        let drive: Vec<&str> = answers
            .questions
            .iter()
            .filter(|q| q.needs_drive())
            .map(|q| q.id.as_str())
            .collect();
        assert_eq!(drive, ["Q03", "Q04", "Q06", "Q11", "Q12", "Q13"]);
    }

    #[test]
    fn every_count_question_names_its_unit() {
        let answers = Answers::parse(FIXTURE).unwrap();
        for q in answers.questions.iter().filter(|q| q.kind == Kind::Count) {
            assert!(q.unit.is_some(), "{} has no unit", q.id);
        }
    }
}
