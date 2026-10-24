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
    /// The plants of `garden.json` created without a species: the knowledge seed hasn't it.
    pub plants_without_species: Vec<String>,
    /// The scans of `garden.json` the loader couldn't write, each with why.
    pub skipped_scans: Vec<String>,
    /// Every question, with its runs.
    pub questions: Vec<QuestionReport>,
    /// Estimated cost, US dollars, by model.
    pub cost_usd: BTreeMap<String, f64>,
    /// Turns that didn't complete (failed, interrupted, or a question that errored), over every
    /// question and model. Their applicable measures count as failed.
    pub failed_turns: u32,
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
    /// How the turn ended: `completed`, `failed` or `interrupted`. A question that errored
    /// before or during its turn is `failed`, with the error's code in `errors`.
    pub outcome: String,
    /// The route of `message_started` (`care_question`, `off_topic`, …); empty if none came.
    pub route: String,
    /// The stored answer; for a plan question, the plan's text: its summary, each step's title
    /// and detail, and what to watch for, one paragraph each.
    pub text: String,
    /// The model's text across all calls of the turn, before the dose check. It differs from
    /// `text` where a sentence was replaced. A paragraph break joins each call's text to the
    /// last, as the turn does between tool rounds. Not recorded: the off-topic route's text and
    /// the turn's own texts (budget note, refusal, loop cap, poison control), which are
    /// dose-checked but fixed. Splitting is per sentence on the whole text, so a streamed cut
    /// can differ (a line cut before "2. Remove" makes a list number; a 2,000-byte forced
    /// release) (EON gap G20). Fixture data only: the product never stores or logs it.
    pub raw_text: String,
    /// The sentences of `raw_text` the dose check fails, each with its rule and matched words
    /// (the production check with the built-in product words only).
    pub replaced: Vec<crate::score::Replaced>,
    /// The scores.
    pub scores: Scores,
    /// The tools called, in order.
    pub tools: Vec<String>,
    /// Every tool call the model made, with its arguments, including any the turn did not run
    /// (the last call at the loop cap, a stream that failed or refused) (EON gap G18).
    pub tool_calls: Vec<crate::meter::ToolCall>,
    /// The vendor ids of `farm_citations`; for a plan question, of the plan's `farm_records`.
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
    /// The plan the writer wrote, for a plan question.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanReport>,
}

/// A plan the plan writer wrote for a plan question, validated as the pipeline stores it.
#[derive(Debug, Clone, Serialize)]
pub struct PlanReport {
    /// The `garden.json` scan it was written for.
    pub scan: String,
    /// Its summary.
    pub summary: String,
    /// Its steps that passed validation.
    pub steps: Vec<PlanStepReport>,
    /// What to watch for.
    pub watch_for: Vec<String>,
    /// The farm records its steps cite (`ActionPlan.farm_records`).
    pub farm_records: Vec<FarmRecordReport>,
    /// The rules the final answer still broke; their steps were dropped.
    pub dropped: Vec<String>,
    /// The model that wrote it.
    pub llm_model: String,
}

/// One step of a [`PlanReport`].
#[derive(Debug, Clone, Serialize)]
pub struct PlanStepReport {
    /// Its title.
    pub title: String,
    /// Its detail.
    pub detail: String,
    /// The farm records it cites.
    pub farm_citations: Vec<String>,
}

/// A farm record a plan cites.
#[derive(Debug, Clone, Serialize)]
pub struct FarmRecordReport {
    /// Slack `"<channel>/<ts>"` or a Drive file id.
    pub vendor_id: String,
    /// `"#field-reports"`, `"Spray log 2026"`.
    pub title: String,
}

impl RunReport {
    /// Whether the turn ended without a completed answer.
    pub fn turn_failed(&self) -> bool {
        self.outcome != "completed"
    }
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
    /// What `garden.json`'s loader left out: plants without a species, scans not written.
    fn loading_notes(&self, out: &mut String) {
        if !self.plants_without_species.is_empty() {
            let _ = writeln!(
                out,
                "\nPlants without a species (not in the knowledge seed): {}.",
                self.plants_without_species.join("; ")
            );
        }
        if !self.skipped_scans.is_empty() {
            let _ = writeln!(
                out,
                "\nScans not loaded: {}.",
                self.skipped_scans.join("; ")
            );
        }
    }

    /// A line per plan question: the farm records each run's plan cites.
    fn plan_citations(&self, model: &str, out: &mut String) {
        for q in &self.questions {
            let cited: Vec<String> = q
                .runs
                .iter()
                .filter(|r| r.model == model && r.plan.is_some())
                .map(|r| {
                    let ids = if r.farm_citations.is_empty() {
                        "none".to_owned()
                    } else {
                        r.farm_citations.join(", ")
                    };
                    format!("run {}: {ids}", r.run)
                })
                .collect();
            if !cited.is_empty() {
                let _ = writeln!(
                    out,
                    "\n{}: the farm records the plan cites: {}.",
                    q.id,
                    cited.join("; ")
                );
            }
        }
    }

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
                "| Q | Measure | Route | Answer | Citation | Cross-source | Safety | Injection | Failed turns | Tools | Tokens in / out | Cost (US$) |"
            );
            let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|---|");
            let mut total = 0.0;
            for q in &self.questions {
                let runs: Vec<&RunReport> = q.runs.iter().filter(|r| &r.model == model).collect();
                if let Some(why) = &q.skipped {
                    let _ = writeln!(
                        out,
                        "| {} | {} | | {why} | | | | | | | | |",
                        q.id, q.measure
                    );
                    continue;
                }
                if runs.is_empty() {
                    continue;
                }
                let n = runs.len() as f64;
                let mean = |f: fn(&RunReport) -> f64| runs.iter().map(|r| f(r)).sum::<f64>() / n;
                let cost: f64 = runs.iter().map(|r| r.cost_usd).sum();
                // A failed run may have no route.
                let mut routes: Vec<&str> = runs
                    .iter()
                    .map(|r| r.route.as_str())
                    .filter(|r| !r.is_empty())
                    .collect();
                routes.sort_unstable();
                routes.dedup();
                total += cost;
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} | {} | {} | {}/{} | {:.1} | {:.0} / {:.0} | {:.3} |",
                    q.id,
                    q.measure,
                    routes.join(", "),
                    tally(runs.iter().map(|r| &r.scores.answer)),
                    tally(runs.iter().map(|r| &r.scores.citation)),
                    tally(runs.iter().map(|r| &r.scores.cross_source)),
                    tally(runs.iter().map(|r| &r.scores.safety)),
                    tally(runs.iter().map(|r| &r.scores.injection)),
                    runs.iter().filter(|r| r.turn_failed()).count(),
                    runs.len(),
                    mean(|r| r.tools.len() as f64),
                    mean(|r| r.tokens_in as f64),
                    mean(|r| r.tokens_out as f64),
                    cost / n,
                );
            }
            self.plan_citations(model, &mut out);
            let _ = writeln!(out, "\nTotal for {model}: US${total:.2}\n");
        }
        let verdict = |j: &JournalCheck| {
            if j.passed() {
                "pass".to_owned()
            } else {
                format!("FAIL ({})", j.problems.join("; "))
            }
        };
        let _ = write!(
            out,
            "Sync: {} records. Slack journal (T-E8.2): {}",
            self.sync.records,
            verdict(&self.sync.slack_journal),
        );
        match &self.sync.drive_journal {
            Some(j) => {
                let _ = writeln!(out, ". Drive journal: {}", verdict(j));
            }
            None => {
                let _ = writeln!(out, ". Drive: not synced");
            }
        }
        self.loading_notes(&mut out);
        if self.failed_turns > 0 {
            let _ = writeln!(
                out,
                "\nFailed turns: {}. Their applicable measures count as failed; see `errors` in the JSON.",
                self.failed_turns
            );
        }
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
