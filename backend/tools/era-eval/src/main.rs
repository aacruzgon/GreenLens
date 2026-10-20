//! `era-eval`: the farm connectors' agent evaluation (docs/EON E8.1, E8.2; [connectors 9.3]).
//!
//! One evaluation: a fresh tenant on each of Era's simulators with the Maple Row Farm fixture
//! injected and a credential minted for it (S9 7); `garden.json` loaded into a fresh embedded
//! store; the fixture synced into it through `api`'s real sync; the request journal checked
//! (E8.2); then every question of `answers.toml` asked of the real `ChatAgent` on each
//! candidate model, `--runs` times, each in a new conversation, and scored ([`score`]). The
//! JSON results go to `--out`, a Markdown table to stdout.
//!
//! ```sh
//! cd backend
//! set -a; . ../infra/dev/.env; set +a        # LLM_API_KEY, never printed
//! ERA_SLACK_BASE=http://127.0.0.1:8091 ERA_GOOGLE_BASE=http://127.0.0.1:8092 \
//! MODELS_DIR=/models ORT_DYLIB_PATH=/path/to/libonnxruntime.so \
//!   cargo run -p era-eval -- --model claude-sonnet-5-5 --model claude-opus-5-5 --runs 3 \
//!   --out era-eval.json
//! ```
//!
//! Until the Drive adapter is wired in ([`bench::drive_source`]), only Slack is synced and the
//! questions that need Drive are skipped. Wiring it in is more than one line once the Drive sync
//! (PR #82) merges: `api::connect` must be public (or re-exported), the harness needs a stub
//! `ConnectVendors` (Era's connectors have no OAuth), `FarmSync::new` takes a new `vendors`
//! argument, and the source is `DriveSource::new(DriveClient::new(client,
//! DriveConfig::era(base)))`. The plan-writer question (`target = "plan"`) waits on E5.4 and is
//! reported as not measured.
//!
//! [connectors 9.3]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#93-the-agent-evaluation

mod answers;
mod bench;
mod garden;
mod journal;
mod meter;
mod report;
mod score;

use std::{collections::BTreeMap, io::Write as _, path::PathBuf, sync::Arc};

use anyhow::Context as _;
use chrono::Datelike as _;
use clap::Parser;
use futures::StreamExt as _;
use secrecy::SecretString;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use domain::{
    authz::{Viewer, authenticate, check_conversation, check_membership_of},
    chat::{Anchor, ChatEvent, SendMessageRequest},
    connectors::{ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource},
    id::{ConversationId, GardenId, UserId},
    ports::{ChatRepo as _, Clock, FarmRecordStore as _, IdGenExt as _, MemoryStore as _, Store},
};
use integrations::era_sim::{SimKind, Tenant, fixture};

use crate::{
    answers::{Answers, Question, Target},
    garden::GardenFile,
    meter::Meter,
    report::{QuestionReport, Report, RunReport, SyncReport},
    score::Context,
};

/// The fixture the evaluation reads.
const FIXTURE: &str = "maple-row";
/// Its folder.
const FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/era/maple-row"
);
/// The Slack channels synced: every channel of the fixture.
const CHANNELS: [&str; 2] = ["C_FIELD", "C_CREW"];
/// The Drive folder synced.
const DRIVE_FOLDER: &str = "F_FARM";

/// The farm connectors' agent evaluation on Era's simulators (docs/EON E8).
#[derive(Debug, Parser)]
#[command(name = "era-eval")]
struct Args {
    /// A candidate chat model; repeat for several. Default: claude-opus-5-5, api's default.
    #[arg(long = "model")]
    models: Vec<String>,
    /// Runs of each question per model.
    #[arg(long, default_value_t = 3)]
    runs: u32,
    /// Only these questions, comma-separated (`Q01,Q09`).
    #[arg(long, value_delimiter = ',')]
    questions: Vec<String>,
    /// Write every run, with its answer, to this JSON file.
    #[arg(long)]
    out: Option<PathBuf>,
    /// The folder holding manifest.toml and the text embedder (`python3 -m ml.models.fetch
    /// --for api`).
    #[arg(long, env = "MODELS_DIR")]
    models_dir: PathBuf,
    /// The model for the router's classifier, memory extraction and off-topic replies.
    #[arg(long, env = "LLM_CHEAP_MODEL", default_value = "claude-haiku-5-5")]
    cheap_model: String,
    /// Stop asking once the estimated spend has passed this many US dollars. The check runs
    /// before each question, so the run may exceed the cap by up to one question.
    #[arg(long, default_value_t = 10.0)]
    max_cost_usd: f64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    let mut args = Args::parse();
    if args.models.is_empty() {
        args.models.push("claude-opus-5-5".to_owned());
    }
    // Before any call: a model with no price would make the cost cap blind.
    meter::ensure_priced(
        args.models
            .iter()
            .map(String::as_str)
            .chain([args.cheap_model.as_str()]),
    )?;
    let api_key = SecretString::from(
        std::env::var("LLM_API_KEY").context("LLM_API_KEY is not set (Anthropic's API key)")?,
    );

    let report = evaluate(&args, &api_key).await?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(report.markdown().as_bytes())?;
    if let Some(path) = &args.out {
        std::fs::write(path, serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    anyhow::ensure!(
        report.sync.slack_journal.passed()
            && report
                .sync
                .drive_journal
                .as_ref()
                .is_none_or(journal::JournalCheck::passed),
        "the first sync's vendor calls aren't the expected ones (T-E8.2)"
    );
    Ok(())
}

/// Everything one evaluation shares.
struct Run {
    store: Arc<store::SurrealStore>,
    clock: Arc<dyn Clock>,
    garden: GardenId,
    owner: UserId,
    owner_auth: domain::id::AuthId,
}

#[expect(
    clippy::too_many_lines,
    reason = "the evaluation's steps in order: fixture, store, tenants, sync, journal, questions"
)]
async fn evaluate(args: &Args, api_key: &SecretString) -> anyhow::Result<Report> {
    let answers = Answers::parse(&std::fs::read_to_string(format!(
        "{FIXTURE_DIR}/answers.toml"
    ))?)?;
    let file = GardenFile::parse(&std::fs::read_to_string(format!(
        "{FIXTURE_DIR}/garden.json"
    ))?)?;
    let questions: Vec<&Question> = answers
        .questions
        .iter()
        .filter(|q| args.questions.is_empty() || args.questions.contains(&q.id))
        .collect();
    anyhow::ensure!(!questions.is_empty(), "no question matches --questions");

    // The bench: store, clock, embedder, the garden.
    let clock: Arc<dyn Clock> = Arc::new(bench::Pinned(file.as_of));
    let embedder = bench::embedder(args.models_dir.clone())
        .await
        .context("the text embedder (MODELS_DIR, ORT_DYLIB_PATH)")?;
    let store = bench::store().await?;
    let ids = bench::ids();
    let loaded = garden::load(
        &file,
        store.as_ref(),
        store.as_ref(),
        embedder.as_ref(),
        ids.as_ref(),
    )
    .await
    .context("loading garden.json")?;
    let owner_key = garden::owner(&file)?.key.clone();
    let run = Run {
        store: Arc::clone(&store),
        clock: Arc::clone(&clock),
        garden: loaded.garden,
        owner: loaded.users[&owner_key],
        owner_auth: loaded.auth_ids[&owner_key].clone(),
    };

    // The tenants, the connectors and the first sync. The tenants are closed on every exit path.
    let http = bench::http()?;
    let slack_fixture = fixture(FIXTURE, "slack.json");
    let slack = Tenant::create(SimKind::Slack, Some(&slack_fixture)).await;
    let mut drive: Option<Tenant> = None;
    let result: anyhow::Result<Report> = async {
    bench::connector(
        store.as_ref(),
        ids.as_ref(),
        run.garden,
        run.owner,
        ConnectorKind::Slack,
        ConnectorAccount::Slack {
            team_id: "T_MAPLE".into(),
            team_name: "Maple Row Farm".into(),
            url: format!("{}/", slack.base()),
        },
        channels(&slack_fixture),
        &slack.token().await,
        run.clock.now(),
    )
    .await?;
    let mut sources = api::farm_sync::FarmSources::default().with(
        ConnectorEndpoint::Era,
        bench::slack_source(&http, slack.base()),
    );
    let drive_base = std::env::var("ERA_GOOGLE_BASE").ok();
    if let Some(source) = bench::drive_source(&http, drive_base.as_deref()) {
        let tenant =
            Tenant::create(SimKind::Drive, Some(&fixture(FIXTURE, "google-drive.json"))).await;
        bench::connector(
            store.as_ref(),
            ids.as_ref(),
            run.garden,
            run.owner,
            ConnectorKind::GoogleDrive,
            ConnectorAccount::GoogleDrive {
                email: "maria@maplerow.example".into(),
            },
            vec![ConnectorSource {
                id: DRIVE_FOLDER.into(),
                name: "Farm records".into(),
            }],
            &tenant.token().await,
            run.clock.now(),
        )
        .await?;
        sources = sources.with(ConnectorEndpoint::Era, source);
        drive = Some(tenant);
    }
    let connectors = 1 + usize::from(drive.is_some());
    let ends = bench::sync(
        Arc::clone(&store) as Arc<dyn Store>,
        Arc::clone(&embedder),
        Arc::clone(&clock),
        sources,
        connectors,
    )
    .await?;
    tracing::info!(?ends, "first sync");
    let records = store.garden_record_count(run.garden).await?;

    // E8.2: the first sync's vendor calls, from the journal of the minted credential.
    let slack_journal = journal::check_slack(
        &slack.requests().await,
        journal::slack_expected(&slack_fixture, &CHANNELS, bench::SLACK_PAGE),
    );
    let drive_journal = match &drive {
        Some(d) => Some(journal::check_drive(&d.requests().await)),
        None => None,
    };

    // The questions.
    let tracker = TaskTracker::new();
    let ctx = Context {
        safety: &answers.safety,
        year: file.as_of.utc_day().year(),
        drive_synced: drive.is_some(),
        replaced_sentence: agent::safety::REPLACED_SENTENCE,
    };
    let mut reports: Vec<QuestionReport> = questions
        .iter()
        .map(|q| QuestionReport {
            id: q.id.clone(),
            measure: q.measure.as_str(),
            ask: q.ask.clone(),
            skipped: skip_reason(q, drive.is_some()),
            runs: Vec::new(),
        })
        .collect();
    let mut cost_usd: BTreeMap<String, f64> = BTreeMap::new();
    let mut stopped = None;
    'models: for model in &args.models {
        let meter = bench::chat_model(&http, api_key.clone(), model, &args.cheap_model);
        let agent = bench::chat_agent(
            &store,
            &embedder,
            &clock,
            Arc::clone(&meter) as Arc<dyn domain::ports::ChatModel>,
            tracker.clone(),
        )
        .await?;
        for (q, report) in questions.iter().zip(reports.iter_mut()) {
            if report.skipped.is_some() {
                continue;
            }
            for n in 1..=args.runs {
                let spent: f64 = cost_usd.values().sum();
                if over_cap(spent, args.max_cost_usd) {
                    stopped = Some(format!(
                        "the estimated spend, US${spent:.2}, passed --max-cost-usd {}",
                        args.max_cost_usd
                    ));
                    break 'models;
                }
                let mark = meter.len();
                let r = match ask(&run, &agent, &meter, q, model, n, ctx).await {
                    Ok(r) => r,
                    Err(e) => {
                        // One question's error is a failed run, not the loss of the evaluation.
                        tracing::warn!(question = %q.id, model, run = n, error = %e, "the question errored");
                        failed_run(
                            q,
                            ctx,
                            (model, n),
                            "failed",
                            vec!["internal: the question could not be asked".to_owned()],
                            meter.since(mark),
                        )
                    }
                };
                *cost_usd.entry(model.clone()).or_default() += r.cost_usd;
                tracing::info!(question = %q.id, model, run = n, cost = r.cost_usd, "asked");
                report.runs.push(r);
            }
        }
    }
    tracker.close();
    tracker.wait().await;

    let failed_turns = reports
        .iter()
        .flat_map(|q| &q.runs)
        .filter(|r| r.turn_failed())
        .count();
    Ok(Report {
        fixture: answers.fixture.clone(),
        as_of: file.as_of.to_string(),
        models: args.models.clone(),
        cheap_model: args.cheap_model.clone(),
        runs: args.runs,
        drive_synced: ctx.drive_synced,
        sync: SyncReport {
            records,
            slack_journal,
            drive_journal,
        },
        skipped_scans: loaded.skipped_scans,
        questions: reports,
        cost_usd,
        failed_turns: u32::try_from(failed_turns).unwrap_or(u32::MAX),
        stopped,
    })
    }
    .await;
    slack.close().await;
    if let Some(d) = drive {
        d.close().await;
    }
    result
}

/// Whether the spend has passed the cap. Written so that a NaN (an unpriced call) or a NaN cap
/// stops the run too.
#[expect(
    clippy::neg_cmp_op_on_partial_ord,
    reason = "the negation is the point: a NaN is not under the cap"
)]
fn over_cap(spent: f64, max: f64) -> bool {
    !(spent <= max)
}

/// The tokens and the estimated cost of a turn's model calls. A call to a model with no price
/// counts as NaN, which stops the run ([`over_cap`]).
fn totals(calls: &[meter::Call]) -> (u64, u64, f64) {
    (
        calls.iter().map(|c| u64::from(c.tokens_in)).sum(),
        calls.iter().map(|c| u64::from(c.tokens_out)).sum(),
        calls
            .iter()
            .map(|c| meter::cost(&c.model, c.tokens_in, c.tokens_out).unwrap_or(f64::NAN))
            .sum(),
    )
}

/// A run whose turn didn't complete: every measure that applies fails ([`score::failed_turn`]).
/// `errors` holds error codes, never a key or a vendor's body.
fn failed_run(
    q: &Question,
    ctx: Context<'_>,
    (model, n): (&str, u32),
    outcome: &str,
    errors: Vec<String>,
    calls: Vec<meter::Call>,
) -> RunReport {
    let (tokens_in, tokens_out, cost_usd) = totals(&calls);
    RunReport {
        model: model.to_owned(),
        run: n,
        outcome: outcome.to_owned(),
        route: String::new(),
        text: String::new(),
        scores: score::failed_turn(q, ctx, outcome),
        tools: Vec::new(),
        farm_citations: Vec::new(),
        calls,
        tokens_in,
        tokens_out,
        cost_usd,
        errors,
    }
}

/// The fixture's channels, with their names.
fn channels(slack: &serde_json::Value) -> Vec<ConnectorSource> {
    CHANNELS
        .iter()
        .map(|id| ConnectorSource {
            id: (*id).to_owned(),
            name: slack["channels"]
                .as_array()
                .and_then(|cs| cs.iter().find(|c| c["id"] == *id))
                .and_then(|c| c["name"].as_str())
                .unwrap_or(id)
                .to_owned(),
        })
        .collect()
}

/// Why a question isn't asked: it needs Drive before E4.3, or it scores the plan writer, which
/// waits on E5.4.
fn skip_reason(q: &Question, drive_synced: bool) -> Option<String> {
    if q.target == Target::Plan {
        return Some("not measured: the plan writer's farm records wait on E5.4".into());
    }
    if !drive_synced && q.needs_drive() {
        return Some("skipped: needs Drive's records (E4.3)".into());
    }
    None
}

/// What a turn's events showed.
#[derive(Debug, Default)]
struct Streamed {
    /// The route of `message_started`, so a question sent off-topic shows (gap G10).
    route: String,
    /// The text deltas, joined.
    text: String,
    /// The tools started, in order.
    tools: Vec<String>,
    /// The memories the turn saved.
    saved: Vec<domain::id::MemoryId>,
    /// The error events.
    errors: Vec<String>,
}

/// Reads a turn's events to the end.
async fn drain(mut events: impl futures::Stream<Item = ChatEvent> + Unpin) -> Streamed {
    let mut seen = Streamed::default();
    while let Some(event) = events.next().await {
        match event {
            ChatEvent::MessageStarted(m) => {
                seen.route = serde_json::to_value(m.route)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
            }
            ChatEvent::TextDelta(d) => seen.text.push_str(&d.text),
            ChatEvent::ToolStarted(t) => seen.tools.push(t.tool),
            ChatEvent::MemorySaved(m) => seen.saved.push(m.memory.id),
            ChatEvent::Error(e) => seen.errors.push(format!("{:?}: {}", e.code, e.message)),
            _ => {}
        }
    }
    seen
}

#[expect(
    clippy::too_many_lines,
    reason = "one question's steps in order: conversation, turn, events, outcome, scores"
)]
/// Asks one question in a new conversation and scores the answer. A memory the turn saved is
/// deleted afterwards, so no run sees another's.
async fn ask(
    run: &Run,
    agent: &agent::ChatAgent,
    meter: &Meter,
    q: &Question,
    model: &str,
    n: u32,
    ctx: Context<'_>,
) -> anyhow::Result<RunReport> {
    let store = run.store.as_ref();
    let now = run.clock.now();
    let viewer = check_membership_of::<Viewer>(store, run.owner, run.garden).await?;
    let conversation: ConversationId = api::wiring::UlidGen.id();
    store
        .create_conversation(
            viewer,
            domain::ports::NewConversation {
                id: conversation,
                user: run.owner,
                anchor: Anchor::Garden(run.garden),
                created_at: now,
            },
        )
        .await?;
    let user = authenticate(
        &bench::SignedIn(run.owner_auth.clone()),
        store,
        "era-eval",
        now,
    )
    .await
    .map_err(|e| anyhow::anyhow!("sign-in: {e:?}"))?;
    let owner = check_conversation::<Viewer>(store, &user, conversation)
        .await
        .map_err(|e| anyhow::anyhow!("conversation: {e:?}"))?;

    let mark = meter.len();
    let started = agent
        .start_turn(agent::TurnRequest {
            owner,
            message: SendMessageRequest {
                text: q.ask.clone(),
                image_ids: None,
            },
            shutdown: CancellationToken::new(),
        })
        .await;
    let turn = match started {
        Ok(turn) => turn,
        Err(e) => {
            let code = match &e {
                agent::TurnError::Domain(d) => d.kind().code().to_string(),
                agent::TurnError::Store(_) => "internal".to_owned(),
            };
            return Ok(failed_run(
                q,
                ctx,
                (model, n),
                "failed",
                vec![format!("the turn could not start: {code}")],
                meter.since(mark),
            ));
        }
    };
    let (events, outcome) = turn.into_parts();
    let Streamed {
        route,
        text: streamed,
        tools,
        saved,
        errors,
    } = drain(events).await;
    let (outcome, text, farm_citations) = match outcome.await {
        Ok(agent::TurnOutcome::Completed(message)) => (
            "completed",
            message.text,
            message
                .farm_citations
                .iter()
                .map(|c| c.record_id.vendor_id.to_string())
                .collect(),
        ),
        Ok(agent::TurnOutcome::Failed) | Err(_) => ("failed", streamed, Vec::new()),
        Ok(agent::TurnOutcome::Interrupted) => ("interrupted", streamed, Vec::new()),
    };
    for memory in saved {
        store.delete(viewer, memory).await?;
    }

    let calls = meter.since(mark);
    let (tokens_in, tokens_out, cost_usd) = totals(&calls);
    let scores = if outcome == "completed" {
        score::score(q, ctx, &text, &farm_citations)
    } else {
        score::failed_turn(q, ctx, outcome)
    };
    Ok(RunReport {
        model: model.to_owned(),
        run: n,
        outcome: outcome.to_owned(),
        route,
        text,
        scores,
        tools,
        farm_citations,
        calls,
        tokens_in,
        tokens_out,
        cost_usd,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cost_cap_stops_on_nan_too() {
        assert!(!over_cap(1.0, 10.0));
        assert!(!over_cap(10.0, 10.0));
        assert!(over_cap(10.01, 10.0));
        assert!(over_cap(f64::NAN, 10.0));
        assert!(over_cap(1.0, f64::NAN));
    }

    #[test]
    fn an_errored_question_is_a_failed_run_with_its_applicable_measures_failed() {
        let answers = Answers::parse(include_str!(
            "../../../tests/fixtures/era/maple-row/answers.toml"
        ))
        .unwrap();
        let q = &answers.questions[0];
        let ctx = Context {
            safety: &answers.safety,
            year: 2026,
            drive_synced: false,
            replaced_sentence: "x",
        };
        let r = failed_run(
            q,
            ctx,
            ("claude-opus-5-5", 2),
            "failed",
            vec!["internal".into()],
            Vec::new(),
        );
        assert!(r.turn_failed());
        assert!(matches!(r.scores.answer, score::Verdict::Fail(_)));
        assert!(matches!(r.scores.safety, score::Verdict::NotApplicable(_)));
    }
}
