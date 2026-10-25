//! S6's account cascade, the store's side, on the embedded `SurrealDB` 3.3.4 engine: a leave
//! cut off after `remove_member` committed is finished from the records (`account_leftovers`),
//! a garden someone else is deleting is found through the membership, the rows keyed by the user
//! (`usage_daily`, `idempotency_key`) go, and after a full cascade nothing of the account is
//! left but its tombstone. The runner's order, retries and the objects are `api`'s
//! (`bins/api/src/tests/account_deletion.rs`).

#[path = "embedded/support.rs"]
#[expect(
    dead_code,
    reason = "the shared fixtures; these tests use only some of them"
)]
mod support;

use domain::{
    authz::{Editor, Membership, Owner, Viewer, check_membership_of},
    garden::gardens::TimeZone,
    id::{AuthId, ConversationId, GardenId, MessageId, ScanId, UserId},
    ports::{
        AccountLeftover, CascadeStep, DeletionRepo, DeletionTarget, GardenRepo, IdGenExt,
        IdentityRepo, NewGarden, NewScan, NewScanPlant, ResolvedLocation, ScanRepo,
    },
    records::weather::{Coordinates, LocationPrecision},
    scan::NewPlantInput,
};
use store::SurrealStore;
use support::{Fixture, exec, fixture, rec, t0};

async fn user(store: &SurrealStore, auth: &str) -> UserId {
    store
        .upsert_user(&auth.parse::<AuthId>().expect("auth id"), Some(auth.into()))
        .await
        .expect("user")
}

/// A garden `owner` creates, with `member` added as `role`.
async fn shared_garden(
    store: &SurrealStore,
    f: &Fixture,
    owner: UserId,
    member: UserId,
    role: &str,
) -> GardenId {
    let garden: GardenId = f.ids.id();
    store
        .create_garden(
            owner,
            NewGarden {
                id: garden,
                name: "Allotment".into(),
                location: ResolvedLocation {
                    coordinates: Coordinates {
                        lat: 43.65,
                        lon: -79.38,
                    },
                    precision: LocationPrecision::About1Km,
                    label: "Toronto, ON".into(),
                    time_zone: TimeZone("America/Toronto".into()),
                },
            },
        )
        .await
        .expect("garden");
    exec(
        store,
        &format!(
            "LET $u = {}; LET $g = {};
INSERT RELATION INTO member_of {{ id: type::record('member_of', [$u, $g]), in: $u, out: $g, role: '{role}' }};",
            rec("user", member),
            rec("garden", garden)
        ),
    )
    .await;
    garden
}

/// `user`'s conversation in `garden` with a message, a chat image, a pending action and a
/// rating, and an unshared chat memory; plus a shared one, which stays with the garden.
async fn chat(store: &SurrealStore, f: &Fixture, user: UserId, garden: GardenId) -> ConversationId {
    let conv: ConversationId = f.ids.id();
    let msg: MessageId = f.ids.id();
    let (who, at) = (rec("user", user), rec("garden", garden));
    let (conv_rec, msg_rec) = (rec("conversation", conv), rec("message", msg));
    let image = rec("chat_image", f.ids.id::<domain::id::table::ChatImage>());
    let action = rec(
        "pending_action",
        f.ids.id::<domain::id::table::PendingAction>(),
    );
    let unshared = rec("memory", f.ids.id::<domain::id::table::Memory>());
    let shared = rec("memory", f.ids.id::<domain::id::table::Memory>());
    exec(
        store,
        &format!(
            "CREATE {conv_rec} SET user = {who}, garden = {at}, anchor = {at};
CREATE {msg_rec} SET conversation = {conv_rec}, role = 'user', text = 'Why are the leaves yellow?';
CREATE {image} SET conversation = {conv_rec}, width = 10, height = 10, storage_key = 'gardens/{garden}/chat/{conv}/1.jpg';
CREATE {action} SET conversation = {conv_rec}, message = {msg_rec}, tool = 'create_task', card = {{}}, preview = {{}},
  expires_at = d'2026-10-19T14:00:00Z';
CREATE type::record('message_feedback', [{msg_rec}]) SET rating = 'up';
CREATE {unshared} SET garden = {at}, kind = 'fact', text = 'I water at 7', source = 'chat', shared = false,
  author = {who}, embedding = [1.0, 0.0], embedding_model = 'test';
CREATE {shared} SET garden = {at}, kind = 'fact', text = 'The bed drains slowly', source = 'chat', shared = true,
  author = {who}, embedding = [1.0, 0.0], embedding_model = 'test';"
        ),
    )
    .await;
    conv
}

/// Every row of the account's own: its rate counters for two days and two idempotency keys.
async fn keyed_rows(store: &SurrealStore, user: UserId) {
    let u = rec("user", user);
    exec(
        store,
        &format!(
            "UPSERT usage_daily:[{u}, '2026-10-17'] SET requests += 1;
UPSERT usage_daily:[{u}, '2026-10-18'] SET requests += 1;
CREATE idempotency_key:[{u}, 'POST /scans', 'k1'] SET fingerprint = 'f', expires_at = d'2026-10-19T14:00:00Z';
CREATE idempotency_key:[{u}, 'POST /chat', 'k2'] SET fingerprint = 'f', expires_at = d'2026-10-19T14:00:00Z';"
        ),
    )
    .await;
}

async fn count(store: &SurrealStore, sql: &str) -> i64 {
    exec(store, &format!("RETURN count({sql});"))
        .await
        .take::<Option<i64>>(0)
        .expect("a count")
        .expect("one value")
}

/// Every step of `target`, each until it deletes nothing (the runner's `delete_records`; no
/// connectors in these gardens).
async fn steps(store: &SurrealStore, target: DeletionTarget) {
    for &step in CascadeStep::steps_for(target) {
        while store.delete_batch(target, step).await.expect("a batch") > 0 {}
    }
}

/// The store calls of `api`'s `run_account`, in its order (objects aside).
async fn account_cascade(store: &SurrealStore, user: UserId) {
    let plan = store.plan_account_deletion(user).await.expect("plan");
    for transfer in plan.transfer {
        store
            .transfer_ownership(transfer, user)
            .await
            .expect("transfer");
    }
    for garden in plan.delete {
        let owner: Membership<Owner> = check_membership_of(store, user, garden)
            .await
            .expect("owner");
        let started = store
            .start_garden_deletion(owner, t0())
            .await
            .expect("start");
        steps(store, started.target).await;
    }
    for garden in plan.leave {
        let member: Membership<Viewer> = check_membership_of(store, user, garden)
            .await
            .expect("member");
        store.remove_member(member, user).await.expect("left");
    }
    for left in store.account_leftovers(user).await.expect("leftovers") {
        let target = if left.deleting {
            DeletionTarget::Garden(left.garden)
        } else {
            DeletionTarget::Member {
                garden: left.garden,
                user,
            }
        };
        steps(store, target).await;
    }
    assert_eq!(store.account_leftovers(user).await.expect("again"), []);
    store.delete_user_feedback(user).await.expect("feedback");
    store.delete_user(user).await.expect("the user goes");
    while store.delete_user_rows(user).await.expect("a batch") > 0 {}
}

/// A cut-off leave: `remove_member` committed, then the objects' delete failed (or `api`
/// restarted). The membership is gone, so the resumed plan no longer lists the garden, but the
/// conversation still refuses `delete_user`; `account_leftovers` finds it from the record.
#[tokio::test]
async fn a_leave_cut_off_after_the_membership_went_is_finished_from_the_records() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let bob = user(&store, "user_bob").await;
    let theirs = shared_garden(&store, &f, bob, f.user, "editor").await;
    let c = chat(&store, &f, f.user, theirs).await;
    let member: Membership<Viewer> = check_membership_of(&*store, f.user, theirs).await.unwrap();

    // The first run: the leave commits, then stops.
    let removed = store.remove_member(member, f.user).await.unwrap();
    assert_eq!(removed.conversations, [c]);

    // The resumed run's plan: only the solo garden, which it deletes.
    let plan = store.plan_account_deletion(f.user).await.unwrap();
    assert_eq!(
        plan.leave,
        [] as [GardenId; 0],
        "the leave left no membership"
    );
    assert_eq!(plan.delete, [f.garden]);
    assert!(
        store.delete_user(f.user).await.is_err(),
        "the conversation refuses the user's deletion"
    );
    assert_eq!(
        store.account_leftovers(f.user).await.unwrap(),
        [AccountLeftover {
            garden: theirs,
            deleting: false,
            conversations: vec![c],
        }]
    );

    account_cascade(&store, f.user).await;
    for (table, kept) in [
        ("conversation", 0),
        ("message", 0),
        ("chat_image", 0),
        ("pending_action", 0),
        ("message_feedback", 0),
        ("memory", 1),
    ] {
        assert_eq!(
            count(&store, &format!("SELECT VALUE id FROM {table}")).await,
            kept,
            "{table}"
        );
    }
    assert_eq!(
        count(
            &store,
            "SELECT VALUE id FROM memory WHERE shared = true AND author = NONE"
        )
        .await,
        1,
        "the shared memory stays with the garden, its author unset"
    );
}

/// The rows keyed by the user hold no reference, so `delete_user` leaves them; the cascade
/// deletes them in batches, and only theirs.
#[tokio::test]
async fn the_usage_and_idempotency_rows_go_with_the_account() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let bob = user(&store, "user_bob").await;
    keyed_rows(&store, f.user).await;
    keyed_rows(&store, bob).await;
    let mine = |table: &str| {
        format!(
            "SELECT VALUE id FROM {table} WHERE record::id(id)[0] = {}",
            rec("user", f.user)
        )
    };
    // The fixture's scan counted against the daily limit on t0's day too.
    assert_eq!(count(&store, &mine("usage_daily")).await, 2);
    assert_eq!(count(&store, &mine("idempotency_key")).await, 2);

    assert_eq!(store.delete_user_rows(f.user).await.unwrap(), 4);
    assert_eq!(store.delete_user_rows(f.user).await.unwrap(), 0);
    assert_eq!(count(&store, &mine("usage_daily")).await, 0);
    assert_eq!(count(&store, &mine("idempotency_key")).await, 0);
    assert_eq!(
        count(&store, "SELECT VALUE id FROM usage_daily").await,
        2,
        "bob's stay"
    );
    assert_eq!(
        count(&store, "SELECT VALUE id FROM idempotency_key").await,
        2
    );
}

/// The exit check's database half: after a full account cascade over a solo garden, a shared
/// garden the user leaves and one another owner is deleting, no table holds a row keyed by or
/// pointing at the user; only the tombstone is left, which keeps refusing the old session token.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one scenario, then every table")]
async fn nothing_of_the_account_is_left_but_the_tombstone() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let auth: AuthId = "user_2abc".parse().unwrap();
    let bob = user(&store, "user_bob").await;
    let carol = user(&store, "user_carol").await;
    let u = rec("user", f.user);

    // The solo garden (the fixture's plant and scan) has a chat of the user's too.
    chat(&store, &f, f.user, f.garden).await;
    // Bob's garden, which the user leaves: their chat, a scan, a task, a map, an invite they
    // used, and feedback on their scan.
    let bobs = shared_garden(&store, &f, bob, f.user, "editor").await;
    chat(&store, &f, f.user, bobs).await;
    let editor: Membership<Editor> = check_membership_of(&*store, f.user, bobs).await.unwrap();
    let scan: ScanId = f.ids.id();
    store
        .create_scan(
            editor,
            NewScan {
                id: scan,
                plant: NewScanPlant::New {
                    id: f.ids.id::<domain::id::table::Plant>(),
                    plant: NewPlantInput {
                        name: "Kale".into(),
                        species_id: None,
                    },
                },
                created_by: f.user,
                created_at: t0(),
            },
        )
        .await
        .unwrap();
    let b = rec("garden", bobs);
    exec(
        &store,
        &format!(
            "CREATE {} SET garden = {b}, title = 'Water', source = 'user', created_by = {u};
CREATE {} SET garden = {b}, version = 1, beds = [], pins = [], created_by = {u};
CREATE {} SET garden = {b}, code = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', role = 'editor', created_by = {},
  expires_at = d'2026-10-25T14:00:00Z', used_at = d'2026-10-18T14:00:00Z', used_by = {u};
CREATE scan_feedback SET scan = {}, user = {u}, verdict = 'correct';
CREATE push_token:['ExponentPushToken[abc]'] SET user = {u}, platform = 'ios';",
            rec("task", f.ids.id::<domain::id::table::Task>()),
            rec("garden_map", f.ids.id::<domain::id::table::GardenMap>()),
            rec("invite", f.ids.id::<domain::id::table::Invite>()),
            rec("user", bob),
            rec("scan", scan),
        ),
    )
    .await;
    // Carol's garden, which she started deleting before the account deletion: its cascade was
    // cut off, and the membership is the only way to it once the user's chat there is gone.
    let carols = shared_garden(&store, &f, carol, f.user, "viewer").await;
    chat(&store, &f, f.user, carols).await;
    store
        .start_garden_deletion(
            check_membership_of(&*store, carol, carols).await.unwrap(),
            t0(),
        )
        .await
        .unwrap();
    keyed_rows(&store, f.user).await;
    assert!(
        store
            .start_account_deletion(f.user, &auth, t0())
            .await
            .unwrap()
    );

    let leftovers = store.account_leftovers(f.user).await.unwrap();
    assert!(
        leftovers.iter().any(|l| l.garden == carols && l.deleting),
        "carol's garden is the account's to wait for: {leftovers:?}"
    );
    account_cascade(&store, f.user).await;

    for (what, sql) in [
        ("user", format!("SELECT VALUE id FROM {u}")),
        (
            "push_token",
            format!("SELECT VALUE id FROM push_token WHERE user = {u}"),
        ),
        (
            "member_of",
            format!("SELECT VALUE id FROM member_of WHERE in = {u}"),
        ),
        (
            "invite",
            format!("SELECT VALUE id FROM invite WHERE created_by = {u} OR used_by = {u}"),
        ),
        (
            "garden_map",
            format!("SELECT VALUE id FROM garden_map WHERE created_by = {u}"),
        ),
        (
            "scan",
            format!("SELECT VALUE id FROM scan WHERE created_by = {u}"),
        ),
        (
            "scan_feedback",
            format!("SELECT VALUE id FROM scan_feedback WHERE user = {u}"),
        ),
        (
            "task",
            format!("SELECT VALUE id FROM task WHERE created_by = {u}"),
        ),
        (
            "conversation",
            format!("SELECT VALUE id FROM conversation WHERE user = {u}"),
        ),
        (
            "memory",
            format!("SELECT VALUE id FROM memory WHERE author = {u}"),
        ),
        (
            "connector",
            format!("SELECT VALUE id FROM connector WHERE created_by = {u}"),
        ),
        (
            "usage_daily",
            format!("SELECT VALUE id FROM usage_daily WHERE record::id(id)[0] = {u}"),
        ),
        (
            "idempotency_key",
            format!("SELECT VALUE id FROM idempotency_key WHERE record::id(id)[0] = {u}"),
        ),
        // The user's chats everywhere, with everything under them.
        ("message", "SELECT VALUE id FROM message".to_owned()),
        ("chat_image", "SELECT VALUE id FROM chat_image".to_owned()),
        (
            "pending_action",
            "SELECT VALUE id FROM pending_action".to_owned(),
        ),
        (
            "message_feedback",
            "SELECT VALUE id FROM message_feedback".to_owned(),
        ),
        (
            "unshared memory",
            "SELECT VALUE id FROM memory WHERE shared = false".to_owned(),
        ),
        // The solo garden and carol's are gone.
        (
            "gardens",
            format!(
                "SELECT VALUE id FROM garden WHERE id IN [{}, {}]",
                rec("garden", f.garden),
                rec("garden", carols)
            ),
        ),
    ] {
        assert_eq!(count(&store, &sql).await, 0, "{what} left");
    }
    // Bob's garden keeps what the user contributed, its links to them unset.
    assert_eq!(
        count(
            &store,
            &format!("SELECT VALUE id FROM scan WHERE garden = {b} AND created_by = NONE")
        )
        .await,
        1
    );
    assert_eq!(
        count(&store, "SELECT VALUE id FROM memory WHERE shared = true").await,
        1,
        "bob's garden's shared memory; the others went with their gardens"
    );
    // The tombstone is the one row left: it refuses the old token's sign-in until Clerk confirms
    // the deletion and `TOMBSTONE_KEEP` has passed.
    assert_eq!(count(&store, "SELECT VALUE id FROM deleted_user").await, 1);
    assert!(store.unfinished_account_deletions().await.unwrap()[0].user_deleted);
    assert!(store.upsert_user(&auth, None).await.is_err());
}
