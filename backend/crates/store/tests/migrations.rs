//! The migration runner on the embedded 3.3.4 engine (`kv-mem`, one datastore per test)
//! ([database 1], applying the migrations; [backend store 4]).

use domain::ports::{ChunkFilter, Embedding, KnowledgeRepo};
use store::{
    Applied, MIGRATIONS, Migration, MigrationError, SurrealStore,
    migrate::{Records, apply_migrations, check_contents},
    migrate_with,
};
use surrealdb::{Surreal, engine::any::Any};

/// What `ml/knowledge/build_migration.py` writes for the fixture seed (the same copy
/// `embedded.rs` loads).
const KNOWLEDGE_V1: &str = include_str!("embedded/knowledge_fixture.surql");

const P1: &str = "CREATE product:`p1` CONTENT { name: 'p1', broad_spectrum: false } RETURN NONE;\n";
const P2: &str = "CREATE product:`p2` CONTENT { name: 'p2', broad_spectrum: false } RETURN NONE;";
const P3: &str = "-- the last line is a comment\nCREATE product:`p3` CONTENT { name: 'p3', broad_spectrum: true }\n-- end";

fn file(name: &'static str, sql: &'static str) -> Migration {
    Migration { name, sql }
}

async fn root() -> Surreal<Any> {
    surrealdb::engine::any::connect("mem://")
        .await
        .expect("embedded engine")
}

async fn count(db: &Surreal<Any>, sql: &str) -> i64 {
    let mut r = db.query(sql).await.expect("query sent");
    assert!(r.take_errors().is_empty());
    r.take::<Option<i64>>(0).expect("a count").unwrap_or(0)
}

async fn records(db: &Surreal<Any>) -> Vec<(String, String)> {
    Records::read(db)
        .await
        .expect("records read")
        .0
        .into_values()
        .map(|r| (r.name, r.hash))
        .collect()
}

/// v2 of the fixture: without the species `strawberry`, the chunks `.2` and `.3`, and the
/// `caused_by` edge.
fn knowledge_v2() -> &'static str {
    let lines: Vec<String> = KNOWLEDGE_V1
        .lines()
        .filter(|l| {
            !l.starts_with("UPSERT species:`strawberry`")
                && !l.starts_with("UPSERT knowledge_chunk:`example_extension_tomato_diseases.2`")
                && !l.starts_with("UPSERT knowledge_chunk:`example_extension_tomato_diseases.3`")
                && !l.starts_with("RELATE disease:`tomato_aphids`->caused_by->")
        })
        .map(|l| {
            if l.starts_with("DELETE species WHERE") {
                "DELETE species WHERE id NOT IN [species:`tomato`] RETURN NONE;".to_owned()
            } else if l.starts_with("DELETE knowledge_chunk WHERE") {
                "DELETE knowledge_chunk WHERE id NOT IN \
                 [knowledge_chunk:`example_extension_tomato_diseases.1`] RETURN NONE;"
                    .to_owned()
            } else {
                l.to_owned()
            }
        })
        .collect();
    assert_eq!(lines.len() + 4, KNOWLEDGE_V1.lines().count());
    (lines.join("\n") + "\n").leak()
}

fn one_hot(i: usize) -> Embedding {
    let mut v = vec![0.0; 384];
    v[i] = 1.0;
    Embedding(v)
}

#[tokio::test]
async fn committed_migrations_apply_twice() {
    let db = root().await;
    migrate_with(&db, MIGRATIONS).await.expect("first start");
    let second = migrate_with(&db, MIGRATIONS).await.expect("second start");
    assert_eq!(second, Applied::default(), "a second start applies nothing");
    assert_eq!(records(&db).await.len(), MIGRATIONS.len());
}

#[tokio::test]
async fn numbered_files_apply_once_in_order_with_their_records() {
    let db = root().await;
    // Given out of order: the runner sorts by number.
    let files = [file("0002_p2.surql", P2), file("0001_p1.surql", P1)];
    let applied = migrate_with(&db, &files).await.expect("first start");
    assert_eq!(applied.numbered, ["0001_p1.surql", "0002_p2.surql"]);
    assert!(!applied.knowledge);
    assert_eq!(
        records(&db).await,
        [
            ("0001_p1.surql".to_owned(), files[1].hash()),
            ("0002_p2.surql".to_owned(), files[0].hash()),
        ]
    );
    let kind = count(
        &db,
        "RETURN count(SELECT VALUE id FROM migration WHERE kind = 'versioned');",
    )
    .await;
    assert_eq!(kind, 2);

    // A third file, ending in a comment with no `;`, applies alone on the next start.
    let files = [files[0], files[1], file("0003_p3.surql", P3)];
    let applied = migrate_with(&db, &files).await.expect("second start");
    assert_eq!(applied.numbered, ["0003_p3.surql"]);
    assert_eq!(
        count(&db, "RETURN count(SELECT VALUE id FROM product);").await,
        3
    );
    assert_eq!(
        migrate_with(&db, &files).await.expect("third start"),
        Applied::default()
    );
}

#[tokio::test]
async fn a_changed_recorded_file_stops_the_start() {
    let db = root().await;
    migrate_with(&db, &[file("0001_p1.surql", P1)])
        .await
        .expect("first start");
    let edited = file(
        "0001_p1.surql",
        "-- a comment\nCREATE product:`p1` CONTENT { name: 'p1', broad_spectrum: false } RETURN NONE;\n",
    );
    let err = migrate_with(&db, &[edited]).await.unwrap_err();
    let MigrationError::Changed {
        name,
        recorded,
        current,
    } = err
    else {
        panic!("{err:?}");
    };
    assert_eq!(name, "0001_p1.surql");
    assert_eq!(recorded, file("0001_p1.surql", P1).hash());
    assert_eq!(current, edited.hash());

    // `\r\n` hashes as `\n`: a Windows checkout of the same file is not a change.
    let crlf = file(
        "0001_p1.surql",
        "CREATE product:`p1` CONTENT { name: 'p1', broad_spectrum: false } RETURN NONE;\r\n",
    );
    assert_eq!(
        migrate_with(&db, &[crlf]).await.expect("same file"),
        Applied::default()
    );
}

#[tokio::test]
async fn a_missing_recorded_file_stops_the_start() {
    let db = root().await;
    migrate_with(&db, &[file("0001_p1.surql", P1), file("0002_p2.surql", P2)])
        .await
        .expect("first start");
    // 0002 renamed to 0002_renamed: the record names a file this build doesn't have.
    let err = migrate_with(
        &db,
        &[file("0001_p1.surql", P1), file("0002_renamed.surql", P2)],
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&err, MigrationError::Missing { name } if name == "0002_p2.surql"),
        "{err:?}"
    );
    assert_eq!(records(&db).await.len(), 2, "nothing applied");
}

#[tokio::test]
async fn an_out_of_order_file_stops_the_start() {
    let db = root().await;
    migrate_with(&db, &[file("0002_p2.surql", P2)])
        .await
        .expect("first start");
    let err = migrate_with(&db, &[file("0001_p1.surql", P1), file("0002_p2.surql", P2)])
        .await
        .unwrap_err();
    assert!(
        matches!(&err, MigrationError::OutOfOrder { name, highest }
            if name == "0001_p1.surql" && highest == "0002_p2.surql"),
        "{err:?}"
    );
    assert_eq!(
        count(&db, "RETURN count(SELECT VALUE id FROM product);").await,
        1
    );
}

#[tokio::test]
async fn a_failing_file_commits_nothing() {
    let db = root().await;
    // Statement 2 writes a table the STRICT database doesn't define.
    let bad = file(
        "0002_bad.surql",
        "CREATE product:`p2` CONTENT { name: 'p2', broad_spectrum: false } RETURN NONE;\n\
         CREATE nosuch:`x` RETURN NONE;\n",
    );
    let err = migrate_with(&db, &[file("0001_p1.surql", P1), bad])
        .await
        .unwrap_err();
    assert!(
        matches!(&err, MigrationError::Failed { name, statement: Some(2), .. }
            if name == "0002_bad.surql"),
        "{err:?}"
    );
    // The file before it stays applied; the failed file left no row and no record.
    assert_eq!(
        records(&db).await,
        [("0001_p1.surql".to_owned(), file("0001_p1.surql", P1).hash())]
    );
    assert_eq!(
        count(&db, "RETURN count(SELECT VALUE id FROM product);").await,
        1
    );

    // A parse error fails the whole query: no statement index; nothing committed.
    let unparsable = file("0002_bad.surql", "CREATE product:`p2` CONTENT {;\n");
    let err = migrate_with(&db, &[file("0001_p1.surql", P1), unparsable])
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            MigrationError::Failed {
                statement: None,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(records(&db).await.len(), 1);
}

#[tokio::test]
async fn a_raced_file_is_skipped() {
    let db = root().await;
    let files = [
        file("0001_p1.surql", P1),
        file("knowledge.surql", KNOWLEDGE_V1),
    ];
    migrate_with(&db, &[]).await.expect("the schema");
    // This process reads the records; another applies both files before it writes.
    let stale = Records::read(&db).await.expect("records read");
    let other = apply_migrations(&db, &files, &stale)
        .await
        .expect("the other process");
    assert_eq!(other.numbered, ["0001_p1.surql"]);
    assert!(other.knowledge);
    // This one's transactions throw "migration raced", re-read the records and move on.
    let this = apply_migrations(&db, &files, &stale)
        .await
        .expect("the raced process");
    assert_eq!(this, Applied::default());
    assert_eq!(
        records(&db).await,
        [
            ("0001_p1.surql".to_owned(), files[0].hash()),
            ("knowledge.surql".to_owned(), files[1].hash()),
        ]
    );
}

#[tokio::test]
async fn a_raced_numbered_file_with_another_hash_is_a_change() {
    let db = root().await;
    migrate_with(&db, &[]).await.expect("the schema");
    let stale = Records::read(&db).await.expect("records read");
    let edited = file(
        "0001_p1.surql",
        "CREATE product:`px` CONTENT { name: 'px', broad_spectrum: false } RETURN NONE;",
    );
    apply_migrations(&db, &[edited], &stale)
        .await
        .expect("the other process");
    let err = apply_migrations(&db, &[file("0001_p1.surql", P1)], &stale)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, MigrationError::Changed { name, .. } if name == "0001_p1.surql"),
        "{err:?}"
    );
}

#[tokio::test]
async fn knowledge_replaces_the_set() {
    let db = root().await;
    let v1 = [file("knowledge.surql", KNOWLEDGE_V1)];
    let applied = migrate_with(&db, &v1).await.expect("v1");
    assert!(applied.knowledge);
    // Unchanged: skipped.
    assert_eq!(
        migrate_with(&db, &v1).await.expect("v1 again"),
        Applied::default()
    );

    let v2 = [file("knowledge.surql", knowledge_v2())];
    check_contents("knowledge.surql", v2[0].sql).expect("v2 follows the rules");
    assert!(migrate_with(&db, &v2).await.expect("v2").knowledge);
    assert_eq!(
        records(&db).await,
        [("knowledge.surql".to_owned(), v2[0].hash())]
    );

    let store = SurrealStore::from_client(db)
        .await
        .expect("select pdm/main");
    let k = store.load_knowledge().await.expect("the knowledge loads");
    let species: Vec<String> = k.species.iter().map(|s| s.id.to_string()).collect();
    assert_eq!(species, ["tomato"]);
    let aphids = k
        .problems
        .iter()
        .find(|p| p.id.as_str() == "tomato_aphids")
        .expect("aphids stay");
    assert!(aphids.caused_by.is_empty(), "the dropped edge is gone");
    assert_eq!(
        count(
            store.db(),
            "RETURN count(SELECT VALUE id FROM knowledge_chunk);"
        )
        .await,
        1
    );
    // The removed chunks' own vectors find only the chunk that stays.
    for i in [1, 2] {
        let hits = store
            .search_chunks(&one_hot(i), ChunkFilter::default(), 5)
            .await
            .expect("the vector search runs");
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].text.contains("Remove infected leaves"));
    }
    // The other tables kept their rows.
    assert_eq!(k.problems.len(), 3);
    assert_eq!(k.products.products.len(), 2);
}

#[tokio::test]
async fn a_numbered_file_reapplies_the_knowledge() {
    let db = root().await;
    let knowledge = file("knowledge.surql", KNOWLEDGE_V1);
    migrate_with(&db, &[knowledge]).await.expect("first start");
    // A numbered file that deletes knowledge rows, as a dimension change does.
    let drop_chunks = file("0001_drop_chunks.surql", "DELETE knowledge_chunk;");
    let applied = migrate_with(&db, &[drop_chunks, knowledge])
        .await
        .expect("second start");
    assert_eq!(applied.numbered, ["0001_drop_chunks.surql"]);
    assert!(applied.knowledge, "the unchanged knowledge file runs again");
    assert_eq!(
        count(&db, "RETURN count(SELECT VALUE id FROM knowledge_chunk);").await,
        3
    );
}

#[tokio::test]
async fn a_return_that_skips_the_record_stops_the_start() {
    let db = root().await;
    // A top-level RETURN ends the wrapper's transaction early on 3.3.4: the file's write
    // commits and the runner's record is never written.
    let early = file(
        "0001_early.surql",
        "CREATE product:`p9` CONTENT { name: 'p9', broad_spectrum: false };\nRETURN 'done';\n",
    );
    let err = migrate_with(&db, &[early]).await.unwrap_err();
    assert!(
        matches!(&err, MigrationError::Unrecorded { name } if name == "0001_early.surql"),
        "{err:?}"
    );
    assert!(
        check_contents(early.name, early.sql).is_err(),
        "the content rule refuses it too"
    );
}

#[tokio::test]
async fn a_start_that_dies_after_a_numbered_file_still_reapplies_the_knowledge() {
    let db = root().await;
    let knowledge = file("knowledge.surql", KNOWLEDGE_V1);
    migrate_with(&db, &[knowledge]).await.expect("first start");
    let drop_chunks = file("0001_drop_chunks.surql", "DELETE knowledge_chunk;");
    // The second start applies the numbered file, then dies in the knowledge file: here a
    // knowledge set whose last statement fails, as a lost connection would end it.
    let failing = file(
        "knowledge.surql",
        Box::leak(format!("{KNOWLEDGE_V1}CREATE nosuch:`x` RETURN NONE;\n").into_boxed_str()),
    );
    migrate_with(&db, &[drop_chunks, failing])
        .await
        .expect_err("the knowledge file fails");
    assert_eq!(
        count(&db, "RETURN count(SELECT VALUE id FROM knowledge_chunk);").await,
        0
    );
    // The third start has the first start's knowledge file, whose hash was recorded before.
    // The numbered file deleted that record in its own transaction, so it runs again.
    let applied = migrate_with(&db, &[drop_chunks, knowledge])
        .await
        .expect("third start");
    assert_eq!(applied.numbered, [] as [&str; 0]);
    assert!(applied.knowledge, "the knowledge set is applied again");
    assert_eq!(
        count(&db, "RETURN count(SELECT VALUE id FROM knowledge_chunk);").await,
        3
    );
}

#[tokio::test]
async fn the_knowledge_meta_and_the_fixture_follow_the_rules() {
    check_contents("knowledge.surql", KNOWLEDGE_V1).expect("the loader's output follows the rules");
    let db = root().await;
    migrate_with(&db, &[]).await.expect("the schema");
    let store = SurrealStore::from_client(db)
        .await
        .expect("select pdm/main");
    assert_eq!(store.knowledge_meta().await.expect("read"), None);
    migrate_with(store.db(), &[file("knowledge.surql", KNOWLEDGE_V1)])
        .await
        .expect("the knowledge");
    let meta = store
        .knowledge_meta()
        .await
        .expect("read")
        .expect("knowledge_meta:current");
    assert_eq!(meta.embedding_model, "test-embed@0");
    assert!(meta.matches_embedder("test-embed", "0", 384));
}
