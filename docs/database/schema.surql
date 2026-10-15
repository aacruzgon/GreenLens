-- PDM database schema
-- SurrealDB 3.3.4 (pin: surrealdb/surrealdb:v3.3.4, crate surrealdb = "=3.3.4").
-- Design notes, query patterns and caveats: README.md in this folder.
--
-- Idempotent: safe to run on every API start. OVERWRITE for tables, fields, events and
-- functions; IF NOT EXISTS for indexes, because OVERWRITE on an index rebuilds it.
-- Run as the root or migration user, before the API serves traffic: an index defined while
-- writes are happening can miss them (surrealdb#7533).
--
-- Conventions
--   ids        ULID string keys generated in Rust (RecordId::new("scan", Ulid::new())), except
--              knowledge records (slugs) and tables whose key is the natural key ([a, b] arrays).
--   NONE       an optional field is option<T>: absent, never NULL. The Rust SDK sends None as NONE.
--   enums      literal union types, so a bad value fails with "Expected 'a' | 'b'".
--   dates      datetime for instants; LocalDate ("YYYY-MM-DD" in the garden's time zone) as a
--              checked string.
--   blobs      shapes owned by contracts.md and validated in Rust are object FLEXIBLE; fields
--              that queries filter on are always top-level and typed.
--   links      REFERENCE ON DELETE REJECT on parent links, so a missed step in a deletion
--              cascade fails loudly instead of orphaning records. The API deletes children
--              first (README.md, deletion). CASCADE only for leaf records with nothing outside
--              the database (push tokens, invites, scan feedback). Author links that outlive
--              their user use UNSET.

DEFINE NAMESPACE IF NOT EXISTS pdm;
USE NS pdm;
DEFINE DATABASE IF NOT EXISTS main STRICT;
USE DB main;

-- ---------------------------------------------------------------------------------------
-- Shared helpers
-- ---------------------------------------------------------------------------------------

DEFINE FUNCTION OVERWRITE fn::is_local_date($d: any) -> bool {
    RETURN $d = NONE OR (type::is_string($d) AND $d = /^\d{4}-\d{2}-\d{2}$/);
};

-- ---------------------------------------------------------------------------------------
-- Identity
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE user SCHEMAFULL;
DEFINE FIELD OVERWRITE auth_id          ON user TYPE string READONLY;   -- the auth provider's user id
DEFINE FIELD OVERWRITE display_name     ON user TYPE string;
DEFINE FIELD OVERWRITE location_consent ON user TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE created_at       ON user TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS user_auth ON user FIELDS auth_id UNIQUE;

-- id = push_token:[token]; registering the same token again is an UPSERT on the id
DEFINE TABLE OVERWRITE push_token SCHEMAFULL;
DEFINE FIELD OVERWRITE id         ON push_token TYPE [string];
DEFINE FIELD OVERWRITE user       ON push_token TYPE record<user> REFERENCE ON DELETE CASCADE;
DEFINE FIELD OVERWRITE platform   ON push_token TYPE "ios" | "android" | "web";
DEFINE FIELD OVERWRITE created_at ON push_token TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS push_token_user ON push_token FIELDS user;

-- ---------------------------------------------------------------------------------------
-- Gardens, membership, plants
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE garden SCHEMAFULL;
DEFINE FIELD OVERWRITE name                ON garden TYPE string;
DEFINE FIELD OVERWRITE centroid            ON garden TYPE geometry<point>;  -- (lon, lat): longitude first
DEFINE FIELD OVERWRITE location_precision  ON garden TYPE "exact" | "about_1_km" | "about_10_km" | "postal_code";
DEFINE FIELD OVERWRITE location_label      ON garden TYPE string;           -- "Toronto, ON"
DEFINE FIELD OVERWRITE time_zone           ON garden TYPE string;           -- IANA, sets every LocalDate
DEFINE FIELD OVERWRITE boundary            ON garden TYPE option<geometry<polygon>>;
-- Weather cache cell: centroid rounded to 0.01° (contracts 7, WeatherSummary), "lat,lon" such as
-- "43.65,-79.38". Stored, so the scheduler groups gardens with one indexed scan; the API reads it
-- from the garden rather than recomputing it. Cast through decimal: <string> of a float adds an
-- "f" suffix ("43.65f").
DEFINE FIELD OVERWRITE weather_cell        ON garden VALUE string::concat(
    <string> <decimal> math::fixed(centroid.coordinates[1], 2), ",",
    <string> <decimal> math::fixed(centroid.coordinates[0], 2));
-- No spatial index exists (surrealdb#5567): a 5-character geohash (about 5 km) narrows the
-- regional-alert query to neighbouring cells before geo::distance.
DEFINE FIELD OVERWRITE geohash             ON garden VALUE geo::hash::encode(centroid, 5);
DEFINE FIELD OVERWRITE soil_profile        ON garden TYPE option<object> FLEXIBLE;  -- SoilProfile, contracts 10
DEFINE FIELD OVERWRITE soil_profile_status ON garden TYPE "pending" | "ready" | "unavailable" DEFAULT "pending";
DEFINE FIELD OVERWRITE map_version         ON garden TYPE int DEFAULT 0;      -- the current garden_map version, 0 for none
DEFINE FIELD OVERWRITE deleting_at         ON garden TYPE option<datetime>;  -- set when DELETE starts; hides the garden
DEFINE FIELD OVERWRITE created_at          ON garden TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS garden_cell    ON garden FIELDS weather_cell;
DEFINE INDEX IF NOT EXISTS garden_geohash ON garden FIELDS geohash;

-- id = member_of:[user, garden]: the key is the uniqueness rule, and the membership check is
-- a direct record read. Create with INSERT RELATION, which fails on an existing member;
-- RELATE with a fixed id would silently overwrite the role.
DEFINE TABLE OVERWRITE member_of TYPE RELATION IN user OUT garden ENFORCED SCHEMAFULL;
DEFINE FIELD OVERWRITE id    ON member_of TYPE [record<user>, record<garden>];
DEFINE FIELD OVERWRITE role  ON member_of TYPE "owner" | "editor" | "viewer" | "reviewer";
DEFINE FIELD OVERWRITE since ON member_of TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS member_garden ON member_of FIELDS out, since;  -- members list, "longest-standing editor"

DEFINE TABLE OVERWRITE invite SCHEMAFULL;
DEFINE FIELD OVERWRITE garden     ON invite TYPE record<garden> REFERENCE ON DELETE CASCADE;
DEFINE FIELD OVERWRITE code       ON invite TYPE string READONLY;   -- 26 base32 characters, 128 bits
DEFINE FIELD OVERWRITE role       ON invite TYPE "owner" | "editor" | "viewer" | "reviewer";
DEFINE FIELD OVERWRITE created_by ON invite TYPE option<record<user>> REFERENCE ON DELETE UNSET;
DEFINE FIELD OVERWRITE expires_at ON invite TYPE datetime;
DEFINE FIELD OVERWRITE used_at    ON invite TYPE option<datetime>;
DEFINE FIELD OVERWRITE used_by    ON invite TYPE option<record<user>> REFERENCE ON DELETE UNSET;
DEFINE FIELD OVERWRITE revoked_at ON invite TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at ON invite TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS invite_code   ON invite FIELDS code UNIQUE;
DEFINE INDEX IF NOT EXISTS invite_garden ON invite FIELDS garden;

DEFINE TABLE OVERWRITE plant SCHEMAFULL;
DEFINE FIELD OVERWRITE garden              ON plant TYPE record<garden> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE name                ON plant TYPE string;
DEFINE FIELD OVERWRITE species             ON plant TYPE option<record<species>>;
DEFINE FIELD OVERWRITE custom_species_name ON plant TYPE option<string> ASSERT $value = NONE OR string::len($value) <= 80;
DEFINE FIELD OVERWRITE species_status      ON plant TYPE "unknown" | "suggested" | "confirmed" DEFAULT "unknown";
DEFINE FIELD OVERWRITE species_candidates  ON plant TYPE array<object> DEFAULT [];
DEFINE FIELD OVERWRITE species_candidates.*.species ON plant TYPE option<record<species>>;
DEFINE FIELD OVERWRITE species_candidates.*.name    ON plant TYPE string;
DEFINE FIELD OVERWRITE species_candidates.*.score   ON plant TYPE float;
DEFINE FIELD OVERWRITE species_candidates.*.source  ON plant TYPE "bioclip" | "vlm" | "user";
DEFINE FIELD OVERWRITE status              ON plant TYPE "healthy" | "watch" | "sick" | "dead" DEFAULT "healthy";
-- from the garden map (architecture 6.8); absent when the plant has no pin on the current map
DEFINE FIELD OVERWRITE position            ON plant TYPE option<object>;
DEFINE FIELD OVERWRITE position.map_version ON plant TYPE int;
DEFINE FIELD OVERWRITE position.x_m         ON plant TYPE float;
DEFINE FIELD OVERWRITE position.y_m         ON plant TYPE float;
DEFINE FIELD OVERWRITE deleting_at         ON plant TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at          ON plant TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS plant_garden ON plant FIELDS garden, name;

-- The garden map, made on a LiDAR device (architecture 6.8, contracts 9). Only the current
-- version is kept: a PUT checks base_version against garden.map_version, increments it, writes
-- the new map and deletes the older ones in one transaction, so pins never point at a plant
-- deleted since. Deleting a plant removes its pin from this map in place.
DEFINE TABLE OVERWRITE garden_map SCHEMAFULL;
DEFINE FIELD OVERWRITE garden     ON garden_map TYPE record<garden> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE version    ON garden_map TYPE int;
DEFINE FIELD OVERWRITE beds       ON garden_map TYPE array<object> FLEXIBLE;   -- [{id, name?, polygon_m, area_m2}], validated in Rust
DEFINE FIELD OVERWRITE pins       ON garden_map TYPE array<object>;
DEFINE FIELD OVERWRITE pins.*.plant  ON garden_map TYPE record<plant>;   -- no reference: the API removes the pin before the plant
DEFINE FIELD OVERWRITE pins.*.x_m    ON garden_map TYPE float;
DEFINE FIELD OVERWRITE pins.*.y_m    ON garden_map TYPE float;
DEFINE FIELD OVERWRITE pins.*.canopy_radius_m ON garden_map TYPE option<float>;
DEFINE FIELD OVERWRITE pins.*.bed_id ON garden_map TYPE option<string>;
DEFINE FIELD OVERWRITE north_deg  ON garden_map TYPE option<float> ASSERT $value = NONE OR ($value >= 0 AND $value < 360);
DEFINE FIELD OVERWRITE created_by ON garden_map TYPE option<record<user>> REFERENCE ON DELETE UNSET;
DEFINE FIELD OVERWRITE created_at ON garden_map TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS garden_map_version ON garden_map FIELDS garden, version UNIQUE;

-- ---------------------------------------------------------------------------------------
-- Scans
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE scan SCHEMAFULL;
DEFINE FIELD OVERWRITE garden         ON scan TYPE record<garden> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE plant          ON scan TYPE record<plant> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE created_by     ON scan TYPE option<record<user>> REFERENCE ON DELETE UNSET;
DEFINE FIELD OVERWRITE status         ON scan TYPE "pending" | "queued" | "analysing" | "awaiting_answer" | "diagnosed" | "planned" | "failed" DEFAULT "pending";
DEFINE FIELD OVERWRITE run            ON scan TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE working_species ON scan TYPE option<record<species>>;
DEFINE FIELD OVERWRITE failure        ON scan TYPE option<object>;
DEFINE FIELD OVERWRITE failure.code   ON scan TYPE "no_plant_found" | "photos_unusable" | "processing_error" | "upstream_unavailable";
DEFINE FIELD OVERWRITE failure.message   ON scan TYPE string;
DEFINE FIELD OVERWRITE failure.retryable ON scan TYPE bool;
DEFINE FIELD OVERWRITE open_question  ON scan TYPE option<record<question>>;
DEFINE FIELD OVERWRITE questions_asked ON scan TYPE int DEFAULT 0;   -- in the current run, at most 2
DEFINE FIELD OVERWRITE diagnosis      ON scan TYPE option<record<diagnosis>>;
DEFINE FIELD OVERWRITE plan           ON scan TYPE option<record<action_plan>>;  -- compare-and-set target
DEFINE FIELD OVERWRITE weather        ON scan TYPE option<object> FLEXIBLE;     -- WeatherSummary snapshot
DEFINE FIELD OVERWRITE last_event_seq ON scan TYPE int DEFAULT 0;   -- the scan_event counter
-- The api step between a run's jobs (contracts 3, steps between jobs), claimed by compare-and-set
-- and re-run by the scheduler's sweep when its claim is older than its limit
DEFINE FIELD OVERWRITE pipeline_step     ON scan TYPE option<"retrieve" | "score" | "plan">;
DEFINE FIELD OVERWRITE pipeline_step_run ON scan TYPE option<int>;        -- the run the claim belongs to
DEFINE FIELD OVERWRITE pipeline_step_at  ON scan TYPE option<datetime>;
DEFINE FIELD OVERWRITE rescore_pending   ON scan TYPE bool DEFAULT false; -- new input arrived while a step held the claim
DEFINE FIELD OVERWRITE submitted_at   ON scan TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at     ON scan TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS scan_plant  ON scan FIELDS plant, created_at;   -- timeline, "most recent scan"
DEFINE INDEX IF NOT EXISTS scan_garden ON scan FIELDS garden, created_at;
DEFINE INDEX IF NOT EXISTS scan_stalled ON scan FIELDS pipeline_step, pipeline_step_at;   -- the stalled-step sweep

DEFINE TABLE OVERWRITE scan_image SCHEMAFULL;
DEFINE FIELD OVERWRITE scan        ON scan_image TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE garden      ON scan_image TYPE record<garden>;   -- denormalised for the storage key and similar-case search
DEFINE FIELD OVERWRITE kind        ON scan_image TYPE "whole_plant" | "leaf_closeup" | "leaf_underside" | "soil_surface" | "flower_fruit" | "squeeze_test" | "jar_test";
DEFINE FIELD OVERWRITE answers_question ON scan_image TYPE option<record<question>>;
DEFINE FIELD OVERWRITE width       ON scan_image TYPE int;
DEFINE FIELD OVERWRITE height      ON scan_image TYPE int;
DEFINE FIELD OVERWRITE storage_key ON scan_image TYPE string READONLY;   -- docs/storage/README.md
DEFINE FIELD OVERWRITE quality     ON scan_image TYPE object FLEXIBLE;  -- { blur_score, exposure }
-- LiDAR depth (architecture 6.7): never served, never sent to workers, deleted with the photo
DEFINE FIELD OVERWRITE depth_key            ON scan_image TYPE option<string> READONLY;
DEFINE FIELD OVERWRITE depth_confidence_key ON scan_image TYPE option<string> READONLY;
DEFINE FIELD OVERWRITE depth_meta           ON scan_image TYPE option<object> FLEXIBLE;   -- DepthMeta in the stored image's frame, with the map's width and height
-- Image embedding for find_similar_cases. Searched exactly within one garden (README.md,
-- vector search), so no HNSW index. The dimension is fixed with the image model on day 1.
DEFINE FIELD OVERWRITE embedding   ON scan_image TYPE option<array<float>>;
DEFINE FIELD OVERWRITE embedding_model ON scan_image TYPE option<string>;
DEFINE FIELD OVERWRITE uploaded_at ON scan_image TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS scan_image_scan   ON scan_image FIELDS scan;
DEFINE INDEX IF NOT EXISTS scan_image_garden ON scan_image FIELDS garden;

DEFINE TABLE OVERWRITE mask SCHEMAFULL;
DEFINE FIELD OVERWRITE scan        ON mask TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE image       ON mask TYPE record<scan_image> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE region_id   ON mask TYPE string;
DEFINE FIELD OVERWRITE storage_key ON mask TYPE string READONLY;
DEFINE FIELD OVERWRITE created_at  ON mask TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS mask_scan ON mask FIELDS scan;

-- id = scan_event:[scan, seq]. Records are stored in id order, so SSE replay is a range read:
--   SELECT * FROM scan_event:[$scan, $after + 1]..=[$scan, ..];
-- seq comes from scan.last_event_seq, incremented in the same transaction (README.md).
DEFINE TABLE OVERWRITE scan_event SCHEMAFULL;
DEFINE FIELD OVERWRITE id      ON scan_event TYPE [record<scan>, int];
DEFINE FIELD OVERWRITE run     ON scan_event TYPE int;
DEFINE FIELD OVERWRITE type    ON scan_event TYPE string;
DEFINE FIELD OVERWRITE payload ON scan_event TYPE object FLEXIBLE;   -- the event body, contracts 4
DEFINE FIELD OVERWRITE ts      ON scan_event TYPE datetime DEFAULT time::now() READONLY;

DEFINE TABLE OVERWRITE observation SCHEMAFULL;
DEFINE FIELD OVERWRITE scan           ON observation TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE run            ON observation TYPE int;
DEFINE FIELD OVERWRITE stage          ON observation TYPE "plant_check" | "segmentation" | "species" | "disease" | "soil" | "organism" | "damage" | "measurement";
DEFINE FIELD OVERWRITE pipeline_stage ON observation TYPE string;   -- contracts 4, Stage; runs replace by this, per image
DEFINE FIELD OVERWRITE image          ON observation TYPE option<record<scan_image>>;   -- the image it came from; replacement is per image
DEFINE FIELD OVERWRITE label          ON observation TYPE string;
DEFINE FIELD OVERWRITE ref            ON observation TYPE option<record<species | disease | organism>>;
DEFINE FIELD OVERWRITE role           ON observation TYPE option<"pest" | "beneficial" | "neutral" | "invasive" | "unknown">;
DEFINE FIELD OVERWRITE detail         ON observation TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE confidence     ON observation TYPE float ASSERT $value >= 0 AND $value <= 1;
DEFINE FIELD OVERWRITE region         ON observation TYPE option<object> FLEXIBLE;   -- Region, with region_id
DEFINE FIELD OVERWRITE source         ON observation TYPE "segmentation" | "bioclip" | "classifier" | "vlm" | "llm_fallback" | "user" | "measurement";
-- Measurement observations (contracts 7) only: top-level so the growth series is one indexed
-- read per plant. The full Measurement stays in detail.
DEFINE FIELD OVERWRITE plant          ON observation TYPE option<record<plant>>;
DEFINE FIELD OVERWRITE quantity       ON observation TYPE option<"plant_height_cm" | "canopy_width_cm" | "leaf_length_cm" | "leaf_area_cm2" | "lesion_diameter_mm" | "lesion_count" | "affected_area_pct" | "hole_diameter_mm" | "hole_count">;
DEFINE FIELD OVERWRITE measured_value ON observation TYPE option<float>;
DEFINE FIELD OVERWRITE taken_at       ON observation TYPE option<datetime>;   -- the scan's created_at: dates the growth point
DEFINE FIELD OVERWRITE model_version  ON observation TYPE string;
DEFINE FIELD OVERWRITE retracted_at   ON observation TYPE option<datetime>;   -- finding_retracted
DEFINE FIELD OVERWRITE created_at     ON observation TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS observation_scan ON observation FIELDS scan, pipeline_stage, image;
DEFINE INDEX IF NOT EXISTS observation_growth ON observation FIELDS plant, quantity, taken_at;

DEFINE TABLE OVERWRITE question SCHEMAFULL;
DEFINE FIELD OVERWRITE scan        ON question TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE run         ON question TYPE int;
DEFINE FIELD OVERWRITE kind        ON question TYPE "indicator" | "photo_request";
DEFINE FIELD OVERWRITE text        ON question TYPE string;
DEFINE FIELD OVERWRITE options     ON question TYPE option<array<object>> FLEXIBLE;
DEFINE FIELD OVERWRITE photo_kind  ON question TYPE option<string>;
DEFINE FIELD OVERWRITE photo_hint  ON question TYPE option<string>;
DEFINE FIELD OVERWRITE candidates  ON question TYPE array<record<disease>>;
-- open and answering are what the app sees; the rest record how the question resolved
DEFINE FIELD OVERWRITE status      ON question TYPE "open" | "answering" | "answered" | "superseded" | "dropped" DEFAULT "open";
DEFINE FIELD OVERWRITE answer      ON question TYPE option<"yes" | "no" | "unsure">;
DEFINE FIELD OVERWRITE resolved_by ON question TYPE option<"user" | "photo" | "superseded" | "dropped">;
DEFINE FIELD OVERWRITE asked_at    ON question TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD OVERWRITE continue_at ON question TYPE datetime;
DEFINE FIELD OVERWRITE continued_at ON question TYPE option<datetime>;   -- the scheduler continued without it
-- A photo answer's current step and when it started (contracts 3); claimed by compare-and-set
DEFINE FIELD OVERWRITE step         ON question TYPE option<"segmenting" | "measuring" | "verifying">;
DEFINE FIELD OVERWRITE step_at      ON question TYPE option<datetime>;
DEFINE FIELD OVERWRITE resolved_at ON question TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS question_scan  ON question FIELDS scan;
DEFINE INDEX IF NOT EXISTS question_sweep ON question FIELDS status, continue_at;   -- the 10-minute sweep
DEFINE INDEX IF NOT EXISTS question_stalled ON question FIELDS step, step_at;   -- photo answers stuck while being measured

DEFINE TABLE OVERWRITE guided_check SCHEMAFULL;
DEFINE FIELD OVERWRITE scan       ON guided_check TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE image      ON guided_check TYPE record<scan_image> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE kind       ON guided_check TYPE "squeeze_test" | "jar_test";
DEFINE FIELD OVERWRITE status     ON guided_check TYPE "analysing" | "done" | "failed" DEFAULT "analysing";
DEFINE FIELD OVERWRITE result     ON guided_check TYPE option<record<observation>>;
DEFINE FIELD OVERWRITE created_at ON guided_check TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS guided_check_scan ON guided_check FIELDS scan;

DEFINE TABLE OVERWRITE diagnosis SCHEMAFULL;
DEFINE FIELD OVERWRITE scan           ON diagnosis TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE run            ON diagnosis TYPE int;
DEFINE FIELD OVERWRITE supersedes     ON diagnosis TYPE option<record<diagnosis>>;
DEFINE FIELD OVERWRITE status         ON diagnosis TYPE "confirmed" | "uncertain" | "healthy";
DEFINE FIELD OVERWRITE primary_problem ON diagnosis TYPE option<record<disease>>;   -- for the timeline and similar cases
DEFINE FIELD OVERWRITE primary        ON diagnosis TYPE option<object> FLEXIBLE;    -- CandidateScore
DEFINE FIELD OVERWRITE candidates     ON diagnosis TYPE array<object> FLEXIBLE;
DEFINE FIELD OVERWRITE organisms      ON diagnosis TYPE array<object> FLEXIBLE;
DEFINE FIELD OVERWRITE soil           ON diagnosis TYPE array<record<observation>>;
DEFINE FIELD OVERWRITE risks          ON diagnosis TYPE array<object> FLEXIBLE;
DEFINE FIELD OVERWRITE evidence_basis ON diagnosis TYPE object FLEXIBLE;
DEFINE FIELD OVERWRITE citations      ON diagnosis TYPE array<object> FLEXIBLE;
DEFINE FIELD OVERWRITE model_versions ON diagnosis TYPE object FLEXIBLE;
DEFINE FIELD OVERWRITE created_at     ON diagnosis TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS diagnosis_scan ON diagnosis FIELDS scan;

DEFINE TABLE OVERWRITE scan_feedback SCHEMAFULL;
DEFINE FIELD OVERWRITE scan            ON scan_feedback TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE user            ON scan_feedback TYPE record<user> REFERENCE ON DELETE CASCADE;
DEFINE FIELD OVERWRITE verdict         ON scan_feedback TYPE "correct" | "wrong" | "unsure";
DEFINE FIELD OVERWRITE corrected_label ON scan_feedback TYPE option<string>;
DEFINE FIELD OVERWRITE notes           ON scan_feedback TYPE option<string>;
DEFINE FIELD OVERWRITE created_at      ON scan_feedback TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS scan_feedback_scan ON scan_feedback FIELDS scan;

-- ---------------------------------------------------------------------------------------
-- Plans and tasks
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE action_plan SCHEMAFULL;
DEFINE FIELD OVERWRITE scan          ON action_plan TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE plant         ON action_plan TYPE record<plant>;
DEFINE FIELD OVERWRITE garden        ON action_plan TYPE record<garden>;
DEFINE FIELD OVERWRITE version       ON action_plan TYPE int;
DEFINE FIELD OVERWRITE supersedes    ON action_plan TYPE option<record<action_plan>>;
DEFINE FIELD OVERWRITE reason        ON action_plan TYPE "initial" | "late_answer" | "species_changed" | "guided_check" | "chat_revision";
DEFINE FIELD OVERWRITE provisional   ON action_plan TYPE bool;
DEFINE FIELD OVERWRITE diagnosis     ON action_plan TYPE record<diagnosis>;
DEFINE FIELD OVERWRITE open_question ON action_plan TYPE option<record<question>>;
DEFINE FIELD OVERWRITE constraints   ON action_plan TYPE array<object> DEFAULT [];
DEFINE FIELD OVERWRITE constraints.*.text       ON action_plan TYPE string;
DEFINE FIELD OVERWRITE constraints.*.from_pending_action ON action_plan TYPE string;   -- an id, not a link: the card may be deleted with its conversation
DEFINE FIELD OVERWRITE constraints.*.created_at ON action_plan TYPE datetime;
DEFINE FIELD OVERWRITE summary       ON action_plan TYPE string;
DEFINE FIELD OVERWRITE steps         ON action_plan TYPE array<object> FLEXIBLE;   -- PlanStep[], validated in Rust
DEFINE FIELD OVERWRITE watch_for     ON action_plan TYPE array<string>;
DEFINE FIELD OVERWRITE citations     ON action_plan TYPE array<object> FLEXIBLE;
DEFINE FIELD OVERWRITE llm_model     ON action_plan TYPE string;
DEFINE FIELD OVERWRITE prompt_version ON action_plan TYPE string;
DEFINE FIELD OVERWRITE created_at    ON action_plan TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS action_plan_version ON action_plan FIELDS scan, version UNIQUE;

-- One row per on_trigger step of a plan; the scheduler evaluates the active ones
DEFINE TABLE OVERWRITE plan_trigger SCHEMAFULL;
DEFINE FIELD OVERWRITE plan       ON plan_trigger TYPE record<action_plan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE garden     ON plan_trigger TYPE record<garden>;
DEFINE FIELD OVERWRITE plant      ON plan_trigger TYPE record<plant>;   -- the plant its tasks go to: the step's plant_id (a neighbour) or the plan's plant
DEFINE FIELD OVERWRITE step_id    ON plan_trigger TYPE string;
DEFINE FIELD OVERWRITE trigger    ON plan_trigger TYPE "frost" | "heat" | "humid_stretch" | "heavy_rain" | "after_rain" | "dry_soil";
DEFINE FIELD OVERWRITE until      ON plan_trigger TYPE option<string> ASSERT fn::is_local_date($value);
DEFINE FIELD OVERWRITE active     ON plan_trigger TYPE bool DEFAULT true;   -- false once the plan is superseded
DEFINE INDEX IF NOT EXISTS plan_trigger_active ON plan_trigger FIELDS active, garden;
DEFINE INDEX IF NOT EXISTS plan_trigger_plan   ON plan_trigger FIELDS plan;
DEFINE INDEX IF NOT EXISTS plan_trigger_plant  ON plan_trigger FIELDS plant;   -- deleting a plant finds other plans' rules for it

DEFINE TABLE OVERWRITE task SCHEMAFULL;
DEFINE FIELD OVERWRITE garden        ON task TYPE record<garden> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE plant         ON task TYPE option<record<plant>> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE plan          ON task TYPE option<record<action_plan>> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE plan_step_id  ON task TYPE option<string>;
DEFINE FIELD OVERWRITE title         ON task TYPE string;
DEFINE FIELD OVERWRITE detail        ON task TYPE option<string>;
DEFINE FIELD OVERWRITE due           ON task TYPE option<string> ASSERT fn::is_local_date($value);
DEFINE FIELD OVERWRITE repeat        ON task TYPE option<object>;
DEFINE FIELD OVERWRITE repeat.every_days ON task TYPE int ASSERT $value >= 1;
DEFINE FIELD OVERWRITE repeat.until  ON task TYPE option<string> ASSERT fn::is_local_date($value);
DEFINE FIELD OVERWRITE source        ON task TYPE "plan" | "forecast_alert" | "chat" | "user";
DEFINE FIELD OVERWRITE trigger       ON task TYPE option<object>;
DEFINE FIELD OVERWRITE trigger.kind  ON task TYPE "frost" | "heat" | "humid_stretch" | "heavy_rain" | "after_rain" | "dry_soil";
DEFINE FIELD OVERWRITE trigger.date  ON task TYPE string ASSERT fn::is_local_date($value);
-- forecast alerts: "<garden_id>:<trigger>:<date>"; plan triggers: "plan:<plan_id>:<step_id>:<date>".
-- A UNIQUE index skips records without the field, so other tasks don't collide (verified on 3.3.4).
DEFINE FIELD OVERWRITE alert_key     ON task TYPE option<string>;
DEFINE FIELD OVERWRITE created_by    ON task TYPE option<record<user>> REFERENCE ON DELETE UNSET;
DEFINE FIELD OVERWRITE done_at       ON task TYPE option<datetime>;
DEFINE FIELD OVERWRITE dismissed_at  ON task TYPE option<datetime>;
DEFINE FIELD OVERWRITE dismissed_reason ON task TYPE option<"user" | "plan_superseded">;
-- open | done | dismissed, stored so the task list filters on one indexed field
DEFINE FIELD OVERWRITE state         ON task VALUE IF done_at != NONE { "done" } ELSE IF dismissed_at != NONE { "dismissed" } ELSE { "open" };
DEFINE FIELD OVERWRITE created_at    ON task TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS task_alert  ON task FIELDS alert_key UNIQUE;
DEFINE INDEX IF NOT EXISTS task_garden ON task FIELDS garden, state, due;
DEFINE INDEX IF NOT EXISTS task_plan   ON task FIELDS plan;
DEFINE INDEX IF NOT EXISTS task_plant  ON task FIELDS plant;

-- ---------------------------------------------------------------------------------------
-- Chat
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE conversation SCHEMAFULL;
DEFINE FIELD OVERWRITE user            ON conversation TYPE record<user> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE garden          ON conversation TYPE record<garden> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE anchor          ON conversation TYPE record<scan | plant | garden>;
DEFINE FIELD OVERWRITE title           ON conversation TYPE option<string>;
DEFINE FIELD OVERWRITE summary         ON conversation TYPE option<string>;
DEFINE FIELD OVERWRITE last_message_at ON conversation TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at      ON conversation TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS conversation_user   ON conversation FIELDS user, garden, last_message_at;
DEFINE INDEX IF NOT EXISTS conversation_anchor ON conversation FIELDS anchor;
DEFINE INDEX IF NOT EXISTS conversation_garden ON conversation FIELDS garden;

DEFINE TABLE OVERWRITE chat_image SCHEMAFULL;
DEFINE FIELD OVERWRITE conversation ON chat_image TYPE record<conversation> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE width        ON chat_image TYPE int;
DEFINE FIELD OVERWRITE height       ON chat_image TYPE int;
DEFINE FIELD OVERWRITE storage_key  ON chat_image TYPE string READONLY;
DEFINE FIELD OVERWRITE uploaded_at  ON chat_image TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS chat_image_conversation ON chat_image FIELDS conversation;

DEFINE TABLE OVERWRITE message SCHEMAFULL;
DEFINE FIELD OVERWRITE conversation   ON message TYPE record<conversation> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE role           ON message TYPE "user" | "assistant";
DEFINE FIELD OVERWRITE text           ON message TYPE string;
DEFINE FIELD OVERWRITE attachments    ON message TYPE array<record<chat_image>> DEFAULT [];
DEFINE FIELD OVERWRITE citations      ON message TYPE array<object> FLEXIBLE DEFAULT [];
DEFINE FIELD OVERWRITE tool_calls     ON message TYPE array<object> FLEXIBLE DEFAULT [];   -- stored, never sent to the app
DEFINE FIELD OVERWRITE route          ON message TYPE option<"diagnosis_question" | "care_question" | "task_change" | "safety_sensitive" | "off_topic">;
DEFINE FIELD OVERWRITE interrupted    ON message TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE model          ON message TYPE option<string>;
DEFINE FIELD OVERWRITE prompt_version ON message TYPE option<string>;
DEFINE FIELD OVERWRITE tokens_in      ON message TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE tokens_out     ON message TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE created_at     ON message TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS message_conversation ON message FIELDS conversation, created_at;

DEFINE TABLE OVERWRITE pending_action SCHEMAFULL;
DEFINE FIELD OVERWRITE conversation ON pending_action TYPE record<conversation> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE message      ON pending_action TYPE record<message> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE tool         ON pending_action TYPE "create_task" | "complete_task" | "revise_plan";
DEFINE FIELD OVERWRITE status       ON pending_action TYPE "proposed" | "confirmed" | "rejected" | "expired" DEFAULT "proposed";
DEFINE FIELD OVERWRITE card         ON pending_action TYPE object FLEXIBLE;
DEFINE FIELD OVERWRITE preview      ON pending_action TYPE object FLEXIBLE;
DEFINE FIELD OVERWRITE result       ON pending_action TYPE option<object> FLEXIBLE;   -- replayed on a confirm retry
DEFINE FIELD OVERWRITE expires_at   ON pending_action TYPE datetime;
DEFINE FIELD OVERWRITE decided_at   ON pending_action TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at   ON pending_action TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS pending_action_conversation ON pending_action FIELDS conversation;
DEFINE INDEX IF NOT EXISTS pending_action_expiry       ON pending_action FIELDS status, expires_at;

-- id = message_feedback:[message]: one rating per message, set with UPSERT
DEFINE TABLE OVERWRITE message_feedback SCHEMAFULL;
DEFINE FIELD OVERWRITE id         ON message_feedback TYPE [record<message>];
DEFINE FIELD OVERWRITE rating     ON message_feedback TYPE "up" | "down";
DEFINE FIELD OVERWRITE notes      ON message_feedback TYPE option<string>;
DEFINE FIELD OVERWRITE updated_at ON message_feedback TYPE datetime VALUE time::now();

-- ---------------------------------------------------------------------------------------
-- Memory
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE memory SCHEMAFULL;
DEFINE FIELD OVERWRITE garden     ON memory TYPE record<garden> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE plant      ON memory TYPE option<record<plant>> REFERENCE ON DELETE REJECT;   -- NONE: a garden-level fact
DEFINE FIELD OVERWRITE kind       ON memory TYPE "fact" | "event" | "summary";
DEFINE FIELD OVERWRITE text       ON memory TYPE string;
DEFINE FIELD OVERWRITE source     ON memory TYPE "pipeline" | "chat" | "user";
DEFINE FIELD OVERWRITE shared     ON memory TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE author     ON memory TYPE option<record<user>> REFERENCE ON DELETE UNSET;   -- chat memories and garden notes
DEFINE FIELD OVERWRITE source_msg ON memory TYPE option<record<message>>;   -- plain link: the message may be deleted first
-- Searched exactly within one garden (README.md, vector search), so no HNSW index.
-- The dimension is set by the pinned text-embedding model (architecture 10).
DEFINE FIELD OVERWRITE embedding  ON memory TYPE array<float>;
DEFINE FIELD OVERWRITE embedding_model ON memory TYPE string;
DEFINE FIELD OVERWRITE created_at ON memory TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS memory_garden ON memory FIELDS garden, shared;
DEFINE INDEX IF NOT EXISTS memory_author ON memory FIELDS author;
DEFINE INDEX IF NOT EXISTS memory_plant  ON memory FIELDS plant;

-- ---------------------------------------------------------------------------------------
-- Knowledge base (loaded by migrations from ml/knowledge/seed; ids are slugs)
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE source SCHEMAFULL;
DEFINE FIELD OVERWRITE title        ON source TYPE string;
DEFINE FIELD OVERWRITE publisher    ON source TYPE string;
DEFINE FIELD OVERWRITE url          ON source TYPE string;
DEFINE FIELD OVERWRITE licence      ON source TYPE option<string>;
DEFINE FIELD OVERWRITE retrieved_at ON source TYPE string ASSERT fn::is_local_date($value);

DEFINE TABLE OVERWRITE species SCHEMAFULL;
DEFINE FIELD OVERWRITE scientific_name      ON species TYPE string;
DEFINE FIELD OVERWRITE common_names         ON species TYPE array<string>;
DEFINE FIELD OVERWRITE profile              ON species TYPE "full" | "label_only";
DEFINE FIELD OVERWRITE care                 ON species TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE classifier_supported ON species TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE bioclip_label        ON species TYPE string;
DEFINE FIELD OVERWRITE sources              ON species TYPE array<record<source>> DEFAULT [];
DEFINE FIELD OVERWRITE reviewed_by          ON species TYPE string;

-- every kind of problem: fungal, bacterial, viral, pest, wildlife and abiotic
DEFINE TABLE OVERWRITE disease SCHEMAFULL;
DEFINE FIELD OVERWRITE name            ON disease TYPE string;
DEFINE FIELD OVERWRITE kind            ON disease TYPE "fungal" | "bacterial" | "viral" | "pest" | "wildlife" | "abiotic";
DEFINE FIELD OVERWRITE damage_patterns ON disease TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE prevalence      ON disease TYPE "common" | "occasional" | "rare";
DEFINE FIELD OVERWRITE indicators      ON disease TYPE array<object> FLEXIBLE;   -- validated against the seed JSON Schema
DEFINE FIELD OVERWRITE indicators.*.id     ON disease TYPE string;
DEFINE FIELD OVERWRITE indicators.*.weight ON disease TYPE 1 | 2 | 3;
DEFINE FIELD OVERWRITE indicators.*.discriminating ON disease TYPE bool;
DEFINE FIELD OVERWRITE conditions      ON disease TYPE object FLEXIBLE;
DEFINE FIELD OVERWRITE sources         ON disease TYPE array<record<source>>;
DEFINE FIELD OVERWRITE reviewed_by     ON disease TYPE string;
DEFINE FIELD OVERWRITE reviewed_at     ON disease TYPE string ASSERT fn::is_local_date($value);

DEFINE TABLE OVERWRITE organism SCHEMAFULL;
DEFINE FIELD OVERWRITE name            ON organism TYPE string;
DEFINE FIELD OVERWRITE scientific_name ON organism TYPE string;
DEFINE FIELD OVERWRITE taxon_group     ON organism TYPE "insect" | "mite" | "mollusc" | "worm" | "mammal" | "bird";
DEFINE FIELD OVERWRITE role            ON organism TYPE "pest" | "beneficial" | "neutral" | "invasive";
DEFINE FIELD OVERWRITE recognition     ON organism TYPE string;
DEFINE FIELD OVERWRITE reporting_url   ON organism TYPE option<string>;
DEFINE FIELD OVERWRITE bioclip_label   ON organism TYPE string;
DEFINE FIELD OVERWRITE sources         ON organism TYPE array<record<source>>;
DEFINE FIELD OVERWRITE reviewed_by     ON organism TYPE string;

DEFINE TABLE OVERWRITE treatment SCHEMAFULL;
DEFINE FIELD OVERWRITE kind    ON treatment TYPE "cultural" | "biological" | "mechanical" | "chemical";
DEFINE FIELD OVERWRITE text    ON treatment TYPE string;
DEFINE FIELD OVERWRITE sources ON treatment TYPE array<record<source>>;

DEFINE TABLE OVERWRITE product SCHEMAFULL;
DEFINE FIELD OVERWRITE name           ON product TYPE string;
DEFINE FIELD OVERWRITE broad_spectrum ON product TYPE bool;
-- indexes take fields, not expressions (3.3.4), so case-insensitive uniqueness uses a stored key
DEFINE FIELD OVERWRITE name_key       ON product VALUE string::lowercase(name);
DEFINE INDEX IF NOT EXISTS product_name ON product FIELDS name_key UNIQUE;

-- id = classifier_label:[model_version, index]
DEFINE TABLE OVERWRITE classifier_label SCHEMAFULL;
DEFINE FIELD OVERWRITE id      ON classifier_label TYPE [string, int];
DEFINE FIELD OVERWRITE label   ON classifier_label TYPE string;
DEFINE FIELD OVERWRITE species ON classifier_label TYPE record<species>;
DEFINE FIELD OVERWRITE problem ON classifier_label TYPE option<record<disease>>;   -- NONE: "healthy"

-- id = soil_condition:[aspect, value]
DEFINE TABLE OVERWRITE soil_condition SCHEMAFULL;
DEFINE FIELD OVERWRITE id          ON soil_condition TYPE [string, string];
DEFINE FIELD OVERWRITE description ON soil_condition TYPE string;
DEFINE FIELD OVERWRITE what_to_do  ON soil_condition TYPE string;
DEFINE FIELD OVERWRITE sources     ON soil_condition TYPE array<record<source>>;

DEFINE TABLE OVERWRITE knowledge_chunk SCHEMAFULL;   -- id "<source_id>.<n>"
DEFINE FIELD OVERWRITE source    ON knowledge_chunk TYPE record<source>;
DEFINE FIELD OVERWRITE crop      ON knowledge_chunk TYPE option<record<species>>;
DEFINE FIELD OVERWRITE problem   ON knowledge_chunk TYPE option<record<disease>>;
DEFINE FIELD OVERWRITE organism  ON knowledge_chunk TYPE option<record<organism>>;
DEFINE FIELD OVERWRITE topic     ON knowledge_chunk TYPE "symptoms" | "conditions" | "management" | "care" | "recognition";
DEFINE FIELD OVERWRITE text      ON knowledge_chunk TYPE string;
DEFINE FIELD OVERWRITE text_hash ON knowledge_chunk TYPE string;   -- unchanged text keeps its vector
DEFINE FIELD OVERWRITE embedding ON knowledge_chunk TYPE array<float>;
DEFINE FIELD OVERWRITE embedding_model ON knowledge_chunk TYPE string;
DEFINE INDEX IF NOT EXISTS chunk_problem ON knowledge_chunk FIELDS problem;
DEFINE INDEX IF NOT EXISTS chunk_crop    ON knowledge_chunk FIELDS crop;
DEFINE INDEX IF NOT EXISTS chunk_topic   ON knowledge_chunk FIELDS topic;
-- The one HNSW index: search_knowledge searches the whole knowledge base. Filters on indexed
-- fields pre-filter (3.3+). DIMENSION must equal the pinned text-embedding model's output;
-- 384 is a placeholder until the model is picked on day 1 (architecture 15).
DEFINE INDEX IF NOT EXISTS chunk_vec ON knowledge_chunk FIELDS embedding HNSW DIMENSION 384 DIST COSINE TYPE F32;

-- Graph edges. Fieldless edges are LIGHTWEIGHT: the id is [in, out], RELATE is idempotent and
-- both ends must exist.
DEFINE TABLE OVERWRITE susceptible_to TYPE RELATION IN species  OUT disease  LIGHTWEIGHT;
DEFINE TABLE OVERWRITE manages        TYPE RELATION IN treatment OUT disease LIGHTWEIGHT;
DEFINE TABLE OVERWRITE caused_by      TYPE RELATION IN disease  OUT organism LIGHTWEIGHT;   -- pest and wildlife damage
DEFINE TABLE OVERWRITE preys_on       TYPE RELATION IN organism OUT organism LIGHTWEIGHT;   -- beneficial → pest
-- looks_like carries how_to_tell, so it is a normal edge with id [in, out]; written both ways
DEFINE TABLE OVERWRITE looks_like TYPE RELATION IN disease OUT disease ENFORCED SCHEMAFULL;
DEFINE FIELD OVERWRITE id          ON looks_like TYPE [record<disease>, record<disease>];
DEFINE FIELD OVERWRITE how_to_tell ON looks_like TYPE string;

-- ---------------------------------------------------------------------------------------
-- Operations
-- ---------------------------------------------------------------------------------------

DEFINE TABLE OVERWRITE job SCHEMAFULL;
DEFINE FIELD OVERWRITE kind        ON job TYPE "perception" | "vlm" | "verify" | "guided";
DEFINE FIELD OVERWRITE scan        ON job TYPE record<scan> REFERENCE ON DELETE REJECT;
DEFINE FIELD OVERWRITE run         ON job TYPE option<int>;   -- NONE for guided jobs
DEFINE FIELD OVERWRITE question    ON job TYPE option<record<question>>;   -- a photo answer's segmentation or verify job; top-level for the claim
DEFINE FIELD OVERWRITE payload     ON job TYPE object FLEXIBLE;
DEFINE FIELD OVERWRITE status      ON job TYPE "queued" | "leased" | "done" | "failed" | "cancelled" DEFAULT "queued";
DEFINE FIELD OVERWRITE worker      ON job TYPE option<string>;
DEFINE FIELD OVERWRITE lease_token ON job TYPE option<string>;
DEFINE FIELD OVERWRITE lease_until ON job TYPE option<datetime>;
DEFINE FIELD OVERWRITE claimed_at  ON job TYPE option<datetime>;
DEFINE FIELD OVERWRITE max_runtime ON job TYPE duration;
DEFINE FIELD OVERWRITE attempts    ON job TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE requeues    ON job TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE queued_at   ON job TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE result      ON job TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE error       ON job TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE finished_at ON job TYPE option<datetime>;
DEFINE FIELD OVERWRITE created_at  ON job TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS job_queue  ON job FIELDS status, kind, queued_at;   -- the claim
DEFINE INDEX IF NOT EXISTS job_lease  ON job FIELDS status, lease_until;       -- expired leases
DEFINE INDEX IF NOT EXISTS job_worker ON job FIELDS worker, status;           -- slots, stale workers
DEFINE INDEX IF NOT EXISTS job_scan   ON job FIELDS scan, status;

-- id = worker:[worker_id]
DEFINE TABLE OVERWRITE worker SCHEMAFULL;
DEFINE FIELD OVERWRITE id             ON worker TYPE [string];
DEFINE FIELD OVERWRITE caps           ON worker TYPE array<"perception" | "vlm" | "fallback">;
DEFINE FIELD OVERWRITE slots          ON worker TYPE int;
DEFINE FIELD OVERWRITE gpu            ON worker TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE versions       ON worker TYPE object FLEXIBLE;
DEFINE FIELD OVERWRITE last_heartbeat ON worker TYPE datetime;

-- id = usage_daily:[user, "YYYY-MM-DD"] (UTC day); UPSERT with += in the same transaction as
-- the action it limits
DEFINE TABLE OVERWRITE usage_daily SCHEMAFULL;
DEFINE FIELD OVERWRITE id         ON usage_daily TYPE [record<user>, string];
DEFINE FIELD OVERWRITE scans      ON usage_daily TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE requests   ON usage_daily TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE tokens_in  ON usage_daily TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE tokens_out ON usage_daily TYPE int DEFAULT 0;

-- id = idempotency_key:[user, "METHOD /path", key]. CREATE fails when the key exists, which is
-- the race-free "first request wins" (contracts 1, Idempotency).
DEFINE TABLE OVERWRITE idempotency_key SCHEMAFULL;
DEFINE FIELD OVERWRITE id              ON idempotency_key TYPE [record<user>, string, string];
DEFINE FIELD OVERWRITE fingerprint     ON idempotency_key TYPE string;
DEFINE FIELD OVERWRITE state           ON idempotency_key TYPE "running" | "done" DEFAULT "running";
DEFINE FIELD OVERWRITE response_status ON idempotency_key TYPE option<int>;
DEFINE FIELD OVERWRITE response_body   ON idempotency_key TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE expires_at      ON idempotency_key TYPE datetime;
DEFINE INDEX IF NOT EXISTS idempotency_expiry ON idempotency_key FIELDS expires_at;

-- id = weather_cache:[hour, cell]. Hour first, so cleanup is one range delete:
--   DELETE weather_cache:[NONE, NONE]..[$cutoff, NONE];
DEFINE TABLE OVERWRITE weather_cache SCHEMAFULL;
DEFINE FIELD OVERWRITE id         ON weather_cache TYPE [datetime, string];
DEFINE FIELD OVERWRITE summary    ON weather_cache TYPE object FLEXIBLE;   -- WeatherSummary
DEFINE FIELD OVERWRITE fetched_at ON weather_cache TYPE datetime DEFAULT time::now() READONLY;
