//! The migration runner ([database 1], applying the migrations): `schema.surql` on every
//! start, then the hand-written `NNNN_<slug>.surql` files once each, in number order and
//! hash-locked, then the generated `knowledge.surql` when its hash changed. Each file runs in
//! its own transaction together with its row in the `migration` table, so a file and its
//! record commit together or not at all.
//!
//! [database 1]: https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md#1-version-and-setup

pub mod naming;

use std::{borrow::Cow, collections::BTreeMap, fmt::Write as _, time::Instant};

use domain::ports::StoreError;
use surrealdb::{Surreal, engine::any::Any, types::SurrealValue};

pub use self::naming::{FileKind, KNOWLEDGE_FILE, SCHEMA_FILE, file_kind};
use crate::{
    client::{DATABASE, NAMESPACE, apply_schema_with},
    error::{DbFailure, classify, thrown},
    tx::{run_tx, run_tx_at},
};

/// One embedded migration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    /// The file name, which is also the key of its `migration` record.
    pub name: &'static str,
    /// The file's text.
    pub sql: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

impl Migration {
    /// The SHA-256 of the file as 64 lower-case hex digits, with `\r\n` read as `\n`, so a
    /// Windows checkout hashes the same.
    pub fn hash(&self) -> String {
        file_hash(self.sql)
    }
}

/// The hash [`Migration::hash`] records.
pub fn file_hash(sql: &str) -> String {
    let text: Cow<'_, str> = if sql.contains("\r\n") {
        Cow::Owned(sql.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(sql)
    };
    let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, text.as_bytes());
    digest
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

/// What one run applied, logged at `info` by `api`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// The numbered files this run applied, in order.
    pub numbered: Vec<&'static str>,
    /// Whether this run applied `knowledge.surql`.
    pub knowledge: bool,
}

/// Why the migrations stopped the start ([database 7], migrations). Each names the file.
///
/// [database 7]: https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md#7-operations
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// Connecting, signing in, or reading the `migration` records failed.
    #[error("migrations: {what}")]
    Store {
        /// The step.
        what: &'static str,
        /// The store's error.
        #[source]
        source: Box<StoreError>,
    },
    /// A file name the runner doesn't know (only in a test's own list: `build.rs` refuses
    /// one in `backend/migrations/`).
    #[error("{0}")]
    BadName(String),
    /// A recorded numbered file was edited.
    #[error(
        "{name} was applied with hash {recorded}, and this build's file has {current}: revert \
         the edit and put the change in a new numbered file"
    )]
    Changed {
        /// The file.
        name: String,
        /// The hash in its record.
        recorded: String,
        /// This build's hash.
        current: String,
    },
    /// A recorded file isn't in this build: a renamed file, or a database ahead of this build.
    #[error("{name} is recorded as applied but isn't in this build")]
    Missing {
        /// The file.
        name: String,
    },
    /// A new numbered file sorts before one already applied: it was merged out of order.
    #[error("{name} is new but sorts before {highest}, which is applied: give it the next number")]
    OutOfOrder {
        /// The new file.
        name: String,
        /// The highest applied numbered file.
        highest: String,
    },
    /// A statement failed; the file's transaction committed nothing.
    #[error("{name} failed{}", statement.map(|n| format!(" at statement {n}")).unwrap_or_default())]
    Failed {
        /// The file.
        name: String,
        /// The failed statement, counting the file's statements from 1; one past the file's
        /// last is the runner's own record. `None` when the whole query failed (a parse error,
        /// whose message gives the line, or a lost connection).
        statement: Option<usize>,
        /// The store's error, with the SDK's message in its source chain.
        #[source]
        source: Box<StoreError>,
    },
    /// Another process kept changing the file's record while this one applied it.
    #[error("{name}: its record changed under every attempt")]
    Raced {
        /// The file.
        name: String,
    },
}

/// `migration.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum RecordKind {
    /// A numbered, hand-written file.
    Versioned,
    /// `knowledge.surql`.
    Knowledge,
}

impl RecordKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Versioned => "versioned",
            Self::Knowledge => "knowledge",
        }
    }
}

/// One `migration` row.
#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
pub struct RecordRow {
    /// The file name (the record's key).
    pub name: String,
    /// Numbered or knowledge.
    pub kind: RecordKind,
    /// The hash the file had when it was applied.
    pub hash: String,
}

/// The `migration` table as read at the start of a run, by file name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Records(pub BTreeMap<String, RecordRow>);

/// Every applied file's record.
const READ_RECORDS: &str = "SELECT record::id(id) AS name, kind, hash FROM migration;";

/// One file's recorded hash, or NONE.
const READ_HASH: &str = "SELECT VALUE hash FROM ONLY type::record('migration', $migration_name);";

/// The wrapper's first line: statements 0–2 (`BEGIN`, `LET`, `IF`). The file follows on the
/// same line, so a parse error's line number is the file's own.
const WRAP_HEAD: &str = "BEGIN; LET $migration_prev = (SELECT VALUE hash FROM ONLY \
     type::record('migration', $migration_name)); IF $migration_prev != $migration_expected { \
     THROW 'migration raced: ' + $migration_name }; ";

/// After the file: its record, then `COMMIT`. The `;` on its own line ends a last statement
/// without one, after a last line that may be a comment.
const WRAP_TAIL: &str = "\n;\nUPSERT type::record('migration', $migration_name) CONTENT { kind: \
     $migration_kind, hash: $migration_hash, applied_at: time::now() } RETURN NONE;\nCOMMIT;\n";

/// Statements in the wrapper before the file's first.
const HEAD_STATEMENTS: usize = 3;

/// Attempts at a file whose record keeps changing under the runner.
const RACE_ATTEMPTS: u32 = 3;

impl Records {
    /// Reads the `migration` table. The client must have `pdm`/`main` selected.
    pub async fn read(root: &Surreal<Any>) -> Result<Self, MigrationError> {
        let mut results = run_tx(root, READ_RECORDS, |q| q)
            .await
            .map_err(|f| store_error("read the migration records", f))?;
        let rows: Vec<RecordRow> = results
            .take(0)
            .map_err(|e| store_error("read the migration records", classify(e)))?;
        Ok(Self(
            rows.into_iter().map(|r| (r.name.clone(), r)).collect(),
        ))
    }
}

fn store_error(what: &'static str, failure: DbFailure) -> MigrationError {
    MigrationError::Store {
        what,
        source: Box::new(failure.into_store(what)),
    }
}

/// The schema, then `files`, on a client signed in as root (or the embedded engine): steps 1
/// to 5 of [database 1]'s "How `api` applies them". `api` passes [`MIGRATIONS`]; tests pass
/// their own files.
///
/// [database 1]: https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md#1-version-and-setup
pub async fn migrate_with(
    root: &Surreal<Any>,
    files: &[Migration],
) -> Result<Applied, MigrationError> {
    apply_schema_with(root)
        .await
        .map_err(|source| MigrationError::Failed {
            name: SCHEMA_FILE.to_owned(),
            statement: None,
            source: Box::new(source),
        })?;
    root.use_ns(NAMESPACE)
        .use_db(DATABASE)
        .await
        .map_err(|e| store_error("select pdm/main", classify(e)))?;
    let records = Records::read(root).await?;
    apply_migrations(root, files, &records).await
}

/// Steps 3 to 5 against `records`, the table as read before: the checks, the new numbered
/// files in order, then `knowledge.surql` when its hash changed or a numbered file was
/// applied. `migrate_with` reads `records` just before; a test passes a stale read to play a
/// second process.
pub async fn apply_migrations(
    root: &Surreal<Any>,
    files: &[Migration],
    records: &Records,
) -> Result<Applied, MigrationError> {
    let mut numbered = Vec::new();
    let mut knowledge = None;
    for file in files {
        match file_kind(file.name).map_err(MigrationError::BadName)? {
            FileKind::Versioned(number) => numbered.push((number, *file)),
            FileKind::Knowledge => knowledge = Some(*file),
            FileKind::Schema => {
                return Err(MigrationError::BadName(format!(
                    "{SCHEMA_FILE} is applied first, not as a migration"
                )));
            }
        }
    }
    numbered.sort_unstable_by_key(|(number, _)| *number);
    let numbered: Vec<Migration> = numbered.into_iter().map(|(_, f)| f).collect();

    check(&numbered, knowledge.as_ref(), records)?;

    let mut applied = Applied::default();
    for file in &numbered {
        if records.0.contains_key(file.name) {
            continue;
        }
        if apply_file(root, file, RecordKind::Versioned, None).await? {
            applied.numbered.push(file.name);
        }
    }
    if let Some(file) = knowledge {
        let recorded = records.0.get(file.name).map(|r| r.hash.clone());
        if recorded.as_deref() != Some(file.hash().as_str()) || !applied.numbered.is_empty() {
            applied.knowledge = apply_file(root, &file, RecordKind::Knowledge, recorded).await?;
        }
    }
    Ok(applied)
}

/// Step 3: an edited or missing recorded file, or a new file merged out of order, stops the
/// start.
fn check(
    numbered: &[Migration],
    knowledge: Option<&Migration>,
    records: &Records,
) -> Result<(), MigrationError> {
    for file in numbered {
        if let Some(record) = records.0.get(file.name) {
            let current = file.hash();
            if record.hash != current {
                return Err(MigrationError::Changed {
                    name: file.name.to_owned(),
                    recorded: record.hash.clone(),
                    current,
                });
            }
        }
    }
    let in_build = |name: &str| {
        numbered.iter().any(|f| f.name == name) || knowledge.is_some_and(|f| f.name == name)
    };
    if let Some(name) = records.0.keys().find(|name| !in_build(name)) {
        return Err(MigrationError::Missing { name: name.clone() });
    }
    let highest = records
        .0
        .values()
        .filter(|r| r.kind == RecordKind::Versioned)
        .map(|r| r.name.as_str())
        .max();
    if let Some(highest) = highest
        && let Some(file) = numbered
            .iter()
            .find(|f| !records.0.contains_key(f.name) && f.name < highest)
    {
        return Err(MigrationError::OutOfOrder {
            name: file.name.to_owned(),
            highest: highest.to_owned(),
        });
    }
    Ok(())
}

/// Applies one file in its transaction, expecting its record to hold `expected` (`None` for a
/// file never applied). Returns `false` when another process applied the same file first.
async fn apply_file(
    root: &Surreal<Any>,
    file: &Migration,
    kind: RecordKind,
    mut expected: Option<String>,
) -> Result<bool, MigrationError> {
    let hash = file.hash();
    let sql = [WRAP_HEAD, file.sql, WRAP_TAIL].concat();
    for _ in 0..RACE_ATTEMPTS {
        let started = Instant::now();
        let result = run_tx_at(root, &sql, |q| {
            q.bind(("migration_name", file.name.to_owned()))
                .bind(("migration_kind", kind.as_str()))
                .bind(("migration_hash", hash.clone()))
                .bind(("migration_expected", expected.clone()))
        })
        .await;
        match result {
            Ok(_) => {
                tracing::info!(
                    file = file.name,
                    ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "migration applied"
                );
                return Ok(true);
            }
            Err((_, failure))
                if failure
                    .thrown()
                    .is_some_and(|code| code.starts_with(thrown::MIGRATION_RACED)) =>
            {
                let now = recorded_hash(root, file.name).await?;
                if now.as_deref() == Some(hash.as_str()) {
                    tracing::info!(file = file.name, "migration applied by another process");
                    return Ok(false);
                }
                if kind == RecordKind::Versioned
                    && let Some(recorded) = now
                {
                    return Err(MigrationError::Changed {
                        name: file.name.to_owned(),
                        recorded,
                        current: hash,
                    });
                }
                expected = now;
            }
            Err((index, failure)) => {
                return Err(MigrationError::Failed {
                    name: file.name.to_owned(),
                    statement: index
                        .filter(|i| *i >= HEAD_STATEMENTS)
                        .map(|i| i - HEAD_STATEMENTS + 1),
                    source: Box::new(failure.into_store("apply migration")),
                });
            }
        }
    }
    Err(MigrationError::Raced {
        name: file.name.to_owned(),
    })
}

async fn recorded_hash(root: &Surreal<Any>, name: &str) -> Result<Option<String>, MigrationError> {
    let mut results = run_tx(root, READ_HASH, |q| {
        q.bind(("migration_name", name.to_owned()))
    })
    .await
    .map_err(|f| store_error("read a migration record", f))?;
    results
        .take(0)
        .map_err(|e| store_error("read a migration record", classify(e)))
}

/// The tables `knowledge.surql` may write.
const KNOWLEDGE_TABLES: &[&str] = &[
    "knowledge_meta",
    "source",
    "species",
    "disease",
    "organism",
    "treatment",
    "product",
    "classifier_label",
    "soil_condition",
    "knowledge_chunk",
    "susceptible_to",
    "manages",
    "caused_by",
    "preys_on",
    "looks_like",
];

/// What a migration file may contain ([database 1]), checked line by line: no line starts
/// with `USE`, `BEGIN`, `COMMIT`, `CANCEL`, `OPTION`, or `DEFINE` of a `NAMESPACE`, `DATABASE`,
/// `USER` or `ACCESS`, since the runner selects `pdm`/`main` and wraps each file in its own
/// transaction. In `knowledge.surql`, every statement is one line that writes a knowledge
/// table or `knowledge_meta` and ends with `RETURN NONE;`. A statement that starts after
/// another on the same line isn't seen; the loader writes one statement per line.
///
/// [database 1]: https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md#1-version-and-setup
pub fn check_contents(name: &str, sql: &str) -> Result<(), String> {
    let knowledge = name == KNOWLEDGE_FILE;
    for (n, line) in sql.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        let mut words = upper
            .split(|c: char| c.is_whitespace() || c == ';')
            .filter(|w| !w.is_empty());
        let first = words.next().unwrap_or_default();
        let forbidden = matches!(first, "USE" | "BEGIN" | "COMMIT" | "CANCEL" | "OPTION")
            || (first == "DEFINE"
                && matches!(
                    words.next(),
                    Some("NAMESPACE" | "NS" | "DATABASE" | "DB" | "USER" | "ACCESS")
                ));
        if forbidden {
            return Err(format!(
                "{name}:{}: the runner selects pdm/main and wraps each file in its own \
                 transaction, so a file has no USE, BEGIN, COMMIT, CANCEL, OPTION or DEFINE of a \
                 namespace, database, user or access",
                n + 1
            ));
        }
        if knowledge {
            check_knowledge_line(line).map_err(|why| format!("{name}:{}: {why}", n + 1))?;
        }
    }
    Ok(())
}

fn check_knowledge_line(line: &str) -> Result<(), String> {
    if !line.ends_with("RETURN NONE;") {
        return Err("every write ends with RETURN NONE".to_owned());
    }
    let table_of = |rest: &str| -> String {
        rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    let table = if let Some(rest) = line.strip_prefix("UPSERT ") {
        table_of(rest)
    } else if let Some(rest) = line.strip_prefix("DELETE ") {
        table_of(rest)
    } else if let Some(rest) = line.strip_prefix("INSERT RELATION INTO ") {
        table_of(rest)
    } else if let Some(rest) = line.strip_prefix("RELATE ") {
        // RELATE a:`x`->edge->b:`y`: the edge between the arrows.
        rest.split("->").nth(1).map(table_of).unwrap_or_default()
    } else {
        return Err("a statement is an UPSERT, DELETE, RELATE or INSERT RELATION".to_owned());
    };
    if KNOWLEDGE_TABLES.contains(&table.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "`{table}` isn't a knowledge table or knowledge_meta"
        ))
    }
}

/// `knowledge_meta:current`, the generated values of the loaded knowledge set.
#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
pub struct KnowledgeMeta {
    /// The `bioclip` labels' version ([`ml/models/FORMATS.md`]).
    ///
    /// [`ml/models/FORMATS.md`]: https://github.com/aacruzgon/GreenLens/blob/main/ml/models/FORMATS.md
    pub label_set_version: String,
    /// The disease classifier's label version.
    pub classifier_labels_version: String,
    /// `<name>@<version>` of the model that embedded the chunks.
    pub embedding_model: String,
    /// Their vector length.
    pub embedding_dimension: i64,
}

impl KnowledgeMeta {
    /// Whether a text embedder named `name` at `version`, giving `dimension`-long vectors,
    /// made the stored chunk vectors; `api` stops otherwise, since its query vectors would
    /// not match them.
    pub fn matches_embedder(&self, name: &str, version: &str, dimension: usize) -> bool {
        self.embedding_model == format!("{name}@{version}")
            && usize::try_from(self.embedding_dimension).is_ok_and(|d| d == dimension)
    }
}

/// `knowledge_meta:current`, or `None` before any knowledge file was applied.
const KNOWLEDGE_META: &str = "SELECT label_set_version, classifier_labels_version, \
     embedding_model, embedding_dimension FROM ONLY knowledge_meta:current;";

/// Reads `knowledge_meta:current`; the client must have `pdm`/`main` selected.
pub async fn knowledge_meta(db: &Surreal<Any>) -> Result<Option<KnowledgeMeta>, StoreError> {
    let mut results = run_tx(db, KNOWLEDGE_META, |q| q)
        .await
        .map_err(|f| f.into_store("read knowledge_meta"))?;
    results
        .take(0)
        .map_err(|e| classify(e).into_store("read knowledge_meta"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_rules() {
        assert_eq!(file_kind("schema.surql"), Ok(FileKind::Schema));
        assert_eq!(file_kind("knowledge.surql"), Ok(FileKind::Knowledge));
        assert_eq!(file_kind("0001_init.surql"), Ok(FileKind::Versioned(1)));
        assert_eq!(
            file_kind("0012_chunk_vec_768.surql"),
            Ok(FileKind::Versioned(12))
        );
        for bad in [
            "0000_zero.surql",
            "001_short.surql",
            "0001.surql",
            "0001_.surql",
            "0001_Upper.surql",
            "0001_two__bars.surql",
            "0001-dash.surql",
            "0001_x.sql",
            "notes.surql",
        ] {
            assert!(file_kind(bad).is_err(), "{bad} is refused");
        }
    }

    #[test]
    fn the_hash_reads_crlf_as_lf() {
        assert_eq!(file_hash("a;\r\nb;\r\n"), file_hash("a;\nb;\n"));
        // SHA-256 of the empty string.
        assert_eq!(
            file_hash(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn contents_follow_the_rules() {
        assert!(check_contents("0001_x.surql", "-- USE NS x\nUPDATE t SET f = <int> f;").is_ok());
        for bad in [
            "USE NS pdm;",
            "begin;",
            "  COMMIT;",
            "CANCEL;",
            "OPTION IMPORT;",
            "DEFINE NAMESPACE x;",
            "DEFINE DATABASE OVERWRITE y;",
            "DEFINE USER a ON DATABASE PASSWORD 'p' ROLES EDITOR;",
            "define access a ON DATABASE TYPE RECORD;",
        ] {
            assert!(
                check_contents("0001_x.surql", bad).is_err(),
                "{bad} is refused"
            );
        }
        assert!(check_contents("0001_x.surql", "DEFINE INDEX i ON t FIELDS f;").is_ok());
        assert!(check_contents("knowledge.surql", "DELETE species RETURN NONE;").is_ok());
        assert!(
            check_contents(
                "knowledge.surql",
                "RELATE species:`a`->susceptible_to->disease:`b` RETURN NONE;"
            )
            .is_ok()
        );
        assert!(check_contents("knowledge.surql", "DELETE species;").is_err());
        assert!(check_contents("knowledge.surql", "DELETE plant RETURN NONE;").is_err());
        assert!(
            check_contents(
                "knowledge.surql",
                "RELATE species:`a`->member_of->garden:`b` RETURN NONE;"
            )
            .is_err()
        );
        assert!(
            check_contents("knowledge.surql", "UPDATE species SET x = 1 RETURN NONE;").is_err()
        );
    }

    #[test]
    fn migration_files_follow_the_rules() {
        for file in MIGRATIONS {
            assert!(
                matches!(
                    file_kind(file.name),
                    Ok(FileKind::Versioned(_) | FileKind::Knowledge)
                ),
                "{}",
                file.name
            );
            if let Err(why) = check_contents(file.name, file.sql) {
                panic!("{why}");
            }
        }
    }

    #[test]
    fn the_embedder_check_compares_name_version_and_dimension() {
        let meta = KnowledgeMeta {
            label_set_version: "x".into(),
            classifier_labels_version: "y".into(),
            embedding_model: "snowflake-arctic-embed-xs@d8c86521".into(),
            embedding_dimension: 384,
        };
        assert!(meta.matches_embedder("snowflake-arctic-embed-xs", "d8c86521", 384));
        assert!(!meta.matches_embedder("snowflake-arctic-embed-xs", "other", 384));
        assert!(!meta.matches_embedder("snowflake-arctic-embed-xs", "d8c86521", 768));
    }
}
