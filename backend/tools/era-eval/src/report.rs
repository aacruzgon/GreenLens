//! The results: one JSON file with every run (the answer's text included, for the rubric
//! questions a person scores), and a Markdown table per model.

use std::{collections::BTreeMap, fmt::Write as _};

use serde::Serialize;

use crate::{journal::JournalCheck, meter::Call, score::Scores};

/// The whole evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// The fixture's name.
    pub fixture: String,
    /// The pinned clock.
    pub as_of: String,
    /// The candidate models.
    pub models: Vec<String>,
    /// The model for the classifier, memory extraction and off-topic replies.
    pub cheap_model: String,
    /// Runs per question per model.
    pub runs: u32,
    /// Whether Drive's records were synced (E4.3).
    pub drive_synced: bool,
    /// The sync and E8.2's journal check.
    pub sync: SyncReport,
    /// The scans of `garden.json` the loader couldn't write (they need the knowledge base).
    pub skipped_scans: Vec<String>,
    /// Every question, with its runs.
    pub questions: Vec<QuestionReport>,
    /// Estimated cost, US dollars, by model.
    pub cost_usd: BTreeMap<String, f64>,
    /// Why the evaluation stopped early, if it did.
    pub stopped: Option<String>,
}

/// The first sync.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    /// Farm records stored.
    pub records: u32,
    /// The Slack tenant's journal (T-E8.2).
    pub slack_journal: JournalCheck,
    /// The Drive tenant's journal, once Drive syncs.
    pub drive_journal: Option<JournalCheck>,
}

/// One question.
#[derive(Debug, Clone, Serialize)]
pub struct QuestionReport {
    /// `Q01` …
    pub id: String,
    /// Its measure.
    pub measure: &'static str,
    /// The question.
    pub ask: String,
    /// Why it wasn't run, if it wasn't.
    pub skipped: Option<String>,
    /// Its runs.
    pub runs: Vec<RunReport>,
}

/// One run of one question on one model.
#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    /// The model.
    pub model: String,
    /// 1-based.
    pub run: u32,
    /// How the turn ended: `completed`, `failed` or `interrupted`.
    pub outcome: String,
    /// The stored answer.
    pub text: String,
    /// The scores.
    pub scores: Scores,
    /// The tools called, in order.
    pub tools: Vec<String>,
    /// The vendor ids of `farm_citations`.
    pub farm_citations: Vec<String>,
    /// Every model call of the turn.
    pub calls: Vec<Call>,
    /// Input tokens over every call.
    pub tokens_in: u64,
    /// Output tokens over every call.
    pub tokens_out: u64,
    /// Estimated cost, US dollars.
    pub cost_usd: f64,
    /// The error events, if any.
    pub errors: Vec<String>,
}

/// `passed/scored` for one measure over runs, or the one verdict when only one run counted.
fn tally<'a>(verdicts: impl Iterator<Item = &'a crate::score::Verdict>) -> String {
    let verdicts: Vec<_> = verdicts.collect();
    let scored: Vec<_> = verdicts
        .iter()
        .filter(|v| {
            matches!(
                v,
                crate::score::Verdict::Pass | crate::score::Verdict::Fail(_)
            )
        })
        .collect();
    if scored.is_empty() {
        return verdicts.first().map_or("", |v| v.cell()).to_owned();
    }
    let passed = scored.iter().filter(|v| v.passed()).count();
    if verdicts.len() == 1 {
        return verdicts[0].cell().to_owned();
    }
    format!("{passed}/{}", scored.len())
}

impl Report {
    /// One Markdown table per model: each question's measures over its runs, its tool calls,
    /// tokens and cost (means over runs).
    #[expect(
        clippy::cast_precision_loss,
        reason = "means of small counts, for a table"
    )]
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        for model in &self.models {
            let _ = writeln!(out, "### {model}\n");
            let _ = writeln!(
                out,
                "| Q | Measure | Answer | Citation | Cross-source | Safety | Injection | Tools | Tokens in / out | Cost (US$) |"
            );
            let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|");
            let mut total = 0.0;
            for q in &self.questions {
                let runs: Vec<&RunReport> = q.runs.iter().filter(|r| &r.model == model).collect();
                if let Some(why) = &q.skipped {
                    let _ = writeln!(out, "| {} | {} | {why} | | | | | | | |", q.id, q.measure);
                    continue;
                }
                if runs.is_empty() {
                    continue;
                }
                let n = runs.len() as f64;
                let mean = |f: fn(&RunReport) -> f64| runs.iter().map(|r| f(r)).sum::<f64>() / n;
                let cost: f64 = runs.iter().map(|r| r.cost_usd).sum();
                total += cost;
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} | {} | {:.1} | {:.0} / {:.0} | {:.3} |",
                    q.id,
                    q.measure,
                    tally(runs.iter().map(|r| &r.scores.answer)),
                    tally(runs.iter().map(|r| &r.scores.citation)),
                    tally(runs.iter().map(|r| &r.scores.cross_source)),
                    tally(runs.iter().map(|r| &r.scores.safety)),
                    tally(runs.iter().map(|r| &r.scores.injection)),
                    mean(|r| r.tools.len() as f64),
                    mean(|r| r.tokens_in as f64),
                    mean(|r| r.tokens_out as f64),
                    cost / n,
                );
            }
            let _ = writeln!(out, "\nTotal for {model}: US${total:.2}\n");
        }
        let j = &self.sync.slack_journal;
        let _ = writeln!(
            out,
            "Sync: {} records. Slack journal (T-E8.2): {}{}",
            self.sync.records,
            if j.passed() { "pass" } else { "FAIL" },
            if j.passed() {
                String::new()
            } else {
                format!(" ({})", j.problems.join("; "))
            }
        );
        if let Some(why) = &self.stopped {
            let _ = writeln!(out, "\nStopped early: {why}");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Verdict;

    #[test]
    fn a_tally_counts_passes_over_scored_runs() {
        let v = [Verdict::Pass, Verdict::Fail("x".into()), Verdict::Pass];
        assert_eq!(tally(v.iter()), "2/3");
        assert_eq!(tally([Verdict::Pass].iter()), "pass");
        let review = [Verdict::Review("r".into()), Verdict::Review("r".into())];
        assert_eq!(tally(review.iter()), "review");
        let na = [Verdict::NotApplicable("x".into())];
        assert_eq!(tally(na.iter()), "n/a");
    }
}
