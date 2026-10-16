//! `SurrealQL` fragments shared by several transactions, as macros that expand to string
//! literals, so each whole transaction is still one named `const &str` built with `concat!`
//! (no runtime string building, and never a formatted value: every value is a bind).
//!
//! Each fragment states the binds it reads. Fragments that end a transaction put their result
//! in the last statement before `COMMIT`, which the caller takes at index
//! `num_statements() - 2`.

/// The step-claim re-check ([database 4], steps between jobs: "commit: re-check the claim").
/// Binds: `$scan`, `$step`, `$stamp`, `$run`. Leaves `$s` (the scan's claim fields, `status`
/// and the deleting flags) for later fragments.
macro_rules! claim_check {
    () => {
        "LET $s = (SELECT pipeline_step, pipeline_step_run, pipeline_step_at, run, status, rescore_pending,
                 plant.deleting_at AS plant_deleting, garden.deleting_at AS garden_deleting
          FROM ONLY $scan);
IF $s = NONE OR $s.pipeline_step != $step OR $s.pipeline_step_at != $stamp OR $s.pipeline_step_run != $s.run
   OR $s.run != $run OR $s.plant_deleting != NONE OR $s.garden_deleting != NONE { THROW 'claim_lost'; };
"
    };
}
pub(crate) use claim_check;

/// The hand-over ([database 4], steps between jobs). Binds: `$scan`, `$handover` (the step's own
/// next step, or `NONE` to release), `$now`. Reads `$s` from [`claim_check!`]. Returns the next
/// step, or `NONE` when released.
macro_rules! handover {
    () => {
        "LET $next = IF $s.rescore_pending { 'score' } ELSE { $handover };
UPDATE $scan SET pipeline_step = $next,
                 pipeline_step_run = IF $next = NONE { NONE } ELSE { run },
                 pipeline_step_at = IF $next = NONE { NONE } ELSE { $now },
                 rescore_pending = false;
RETURN $next;
"
    };
}
pub(crate) use handover;

/// A stage's observations, replacing that stage's earlier ones for the images it covers, and
/// those tied to no image when `replace_imageless` ([database 5]: replacement per image).
/// Binds: `$scan`, `$obs` (an [`crate::repo::analysis::ObservationWriteRow`] or `NONE`).
/// Measurement observations get the top-level `plant` and `taken_at` (the scan's
/// `created_at`) for the growth series.
macro_rules! observation_write {
    () => {
        "IF $obs != NONE {
  LET $sc = (SELECT plant, created_at FROM ONLY $scan);
  DELETE observation WHERE scan = $scan AND pipeline_stage = $obs.stage
    AND (image IN $obs.images OR ($obs.replace_imageless AND image = NONE));
  FOR $o IN $obs.rows {
    CREATE $o.id CONTENT $o.row;
    IF $o.row.quantity != NONE { UPDATE $o.id SET plant = $sc.plant, taken_at = $sc.created_at; };
  };
  FOR $m IN $obs.masks { CREATE $m.id CONTENT $m.row; };
};
"
    };
}
pub(crate) use observation_write;

/// Indicator answers with the ranking user > measurement > vlm (contracts 7; schema v1.4
/// `indicator_answer`): re-measured photos first lose their measured answers, then each answer
/// is written only when its rank is at least the stored one. Binds: `$scan`, `$run`,
/// `$measured_images`, `$answers` ([`crate::repo::analysis::AnswerWriteRow`]s).
macro_rules! evidence_write {
    () => {
        "IF array::len($measured_images) > 0 {
  DELETE indicator_answer WHERE scan = $scan AND run = $run AND source = 'measurement' AND image IN $measured_images;
};
FOR $a IN $answers {
  IF ((SELECT VALUE rank FROM ONLY $a.id) ?? 0) <= $a.rank { UPSERT $a.id CONTENT $a.row; };
};
"
    };
}
pub(crate) use evidence_write;

/// Queues a job unless the run already has one of that kind for the same question
/// ([contracts 3]: "queueing checks for an existing job of the run"). Binds: `$job` (a
/// [`crate::repo::jobs::JobWriteRow`] or `NONE`).
macro_rules! job_enqueue {
    () => {
        "IF $job != NONE AND array::len(SELECT VALUE id FROM job WHERE scan = $job.row.scan AND run = $job.row.run
      AND kind = $job.row.kind AND question = $job.row.question AND status IN ['queued', 'leased', 'done'] LIMIT 1) = 0 {
  CREATE $job.id CONTENT $job.row;
};
"
    };
}
pub(crate) use job_enqueue;

/// Resolves a question answered by this step (a photo answer's `verify` result, or a late
/// answer). Binds: `$scan`, `$resolved` (a question id or `NONE`), `$now`.
macro_rules! resolve_question {
    () => {
        "IF $resolved != NONE {
  UPDATE $resolved SET status = 'answered', resolved_by = resolved_by ?? 'photo', resolved_at = $now, step = NONE, step_at = NONE;
  UPDATE $scan SET open_question = NONE WHERE open_question = $resolved;
};
"
    };
}
pub(crate) use resolve_question;

/// The plan compare-and-set ([database 4], plan writes ✓), with what a supersede does in the
/// same transaction ([contracts 7]: the old plan's open tasks are dismissed with
/// `plan_superseded` and its trigger rules removed) and the new tasks and rules, each skipped
/// when its plant isn't live in the garden ([contracts 7], tasks only on live plants).
/// Binds: `$scan`, `$plan` ([`crate::repo::plans::PlanWriteRow`]), `$expected`, `$tasks`,
/// `$triggers`, `$now`.
macro_rules! plan_cas {
    () => {
        "CREATE $plan.id CONTENT $plan.row;
LET $won = (UPDATE $scan SET plan = $plan.id WHERE plan = $expected RETURN VALUE plan);
IF array::len($won) = 0 { THROW 'stale_plan' };
IF $expected != NONE {
  UPDATE task SET dismissed_at = $now, dismissed_reason = 'plan_superseded'
    WHERE plan = $expected AND done_at = NONE AND dismissed_at = NONE;
  UPDATE plan_trigger SET active = false WHERE plan = $expected;
};
FOR $t IN $tasks {
  LET $p = $t.row.plant;
  IF $p = NONE OR array::len(SELECT VALUE id FROM $p WHERE garden = $plan.row.garden AND deleting_at = NONE) > 0 {
    CREATE $t.id CONTENT $t.row;
  };
};
FOR $r IN $triggers {
  LET $p = $r.row.plant;
  IF array::len(SELECT VALUE id FROM $p WHERE garden = $plan.row.garden AND deleting_at = NONE) > 0 {
    CREATE $r.id CONTENT $r.row;
  };
};
"
    };
}
pub(crate) use plan_cas;

/// An entry point's claim ([database 4], steps between jobs ✓: "claim, by an entry point"),
/// with the database's two statements made conditional on each other: the flag is set only
/// when the claim came back `NONE` because another step holds it. A scan that is being deleted
/// or whose status claims nothing (`pending`, `queued`, `failed`) is `not_needed`; an optional
/// `$only_statuses` narrows the statuses that claim (a guided check claims only on a `diagnosed`
/// or `planned` scan; left unbound it is `NONE` and allows all). Binds: `$scan`, `$step`, `$now`.
/// Returns `{ outcome, run, stamp, status }`.
macro_rules! claim_entry {
    () => {
        "LET $c = (SELECT status, deleting_at, plant.deleting_at AS plant_deleting, garden.deleting_at AS garden_deleting FROM ONLY $scan);
LET $eligible = $c != NONE AND $c.deleting_at = NONE AND $c.plant_deleting = NONE AND $c.garden_deleting = NONE
  AND !($c.status IN ['pending', 'queued', 'failed']) AND ($only_statuses = NONE OR $c.status IN $only_statuses);
LET $claimed = IF $eligible {
  (UPDATE ONLY $scan SET pipeline_step = $step, pipeline_step_run = run, pipeline_step_at = $now,
                         rescore_pending = IF $step = 'score' { false } ELSE { rescore_pending }
     WHERE pipeline_step = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE RETURN AFTER)
} ELSE { NONE };
IF $eligible AND $claimed = NONE { UPDATE ONLY $scan SET rescore_pending = true WHERE pipeline_step != NONE; };
RETURN { outcome: IF !$eligible { 'not_needed' } ELSE IF $claimed = NONE { 'busy' } ELSE { 'claimed' },
         run: $claimed.pipeline_step_run, stamp: $claimed.pipeline_step_at, status: $c.status };
"
    };
}
pub(crate) use claim_entry;

/// A photo answer's step re-check ([contracts 3], a photo answer that stalls). Binds:
/// `$question`, `$step`, `$stamp`, `$run`. Leaves `$q` with the question's scan.
macro_rules! question_step_check {
    () => {
        "LET $q = (SELECT scan, status, step, step_at, run, scan.run AS scan_run,
                 scan.plant.deleting_at AS plant_deleting, scan.garden.deleting_at AS garden_deleting
          FROM ONLY $question);
IF $q = NONE OR $q.status != 'answering' OR $q.step != $step OR $q.step_at != $stamp OR $q.run != $q.scan_run
   OR $q.run != $run OR $q.plant_deleting != NONE OR $q.garden_deleting != NONE { THROW 'question_step_lost'; };
"
    };
}
pub(crate) use question_step_check;

/// The projection of an `observation` row into the contract's `Observation` (read with
/// [`crate::read`]): `scan AS scan_id`, and `ref` as `{ table, id }`.
macro_rules! observation_fields {
    () => {
        "id, scan AS scan_id, run, stage, pipeline_stage, label,
  IF ref != NONE { { table: record::tb(ref), id: record::id(ref) } } ELSE { NONE } AS ref,
  role, detail, confidence, region, source, model_version, created_at"
    };
}
pub(crate) use observation_fields;
