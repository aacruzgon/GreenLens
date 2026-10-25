//! The orphan sweep's database side on the embedded `SurrealDB` 3.3.4 engine ([storage 4]):
//! `UNREFERENCED_KEYS` keeps every key a record points to, through each of the five key fields,
//! and answers a stray; `EXISTING_GARDENS` answers the gardens that still have a record, one
//! being deleted included. The sweep itself is `api`'s, tested over the fake store in
//! `bins/api/src/tests/orphans.rs`.

#[path = "embedded/support.rs"]
#[expect(
    dead_code,
    reason = "the shared fixtures; these tests use only some of them"
)]
mod support;

use domain::{
    id::{ChatImageId, ConversationId, GardenId, ImageId, MaskId},
    ports::{DeletionRepo, GardenRepo, IdGenExt, ObjectKey},
};
use support::{exec, fixture, rec};

#[tokio::test]
async fn every_key_field_keeps_its_object_and_a_stray_is_unreferenced() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let image: ImageId = f.ids.id();
    let mask: MaskId = f.ids.id();
    let conversation: ConversationId = f.ids.id();
    let chat_image: ChatImageId = f.ids.id();
    let photo = ObjectKey::scan_image(f.garden, f.scan, image);
    let depth = ObjectKey::depth(f.garden, f.scan, image);
    let confidence = ObjectKey::depth_confidence(f.garden, f.scan, image);
    let mask_key = ObjectKey::mask(f.garden, f.scan, mask);
    let chat_key = ObjectKey::chat_image(f.garden, conversation, chat_image);
    let stray = ObjectKey::scan_image(f.garden, f.scan, f.ids.id());
    exec(
        &store,
        &format!(
            "CREATE {image} SET scan = {scan}, garden = {garden}, kind = 'whole_plant', width = 1024, height = 768,
                storage_key = '{photo}', depth_key = '{depth}', depth_confidence_key = '{confidence}', quality = {{ blur_score: 120.0, exposure: 0.5 }};
             CREATE {mask} SET scan = {scan}, image = {image}, region_id = 'r1', storage_key = '{mask_key}';
             CREATE {conversation} SET user = {user}, garden = {garden}, anchor = {garden};
             CREATE {chat_image} SET conversation = {conversation}, width = 640, height = 480, storage_key = '{chat_key}';",
            image = rec("scan_image", image),
            scan = rec("scan", f.scan),
            garden = rec("garden", f.garden),
            mask = rec("mask", mask),
            conversation = rec("conversation", conversation),
            user = rec("user", f.user),
            chat_image = rec("chat_image", chat_image),
        ),
    )
    .await;

    let keys = [photo, depth, confidence, mask_key, chat_key, stray.clone()];
    assert_eq!(
        store.unreferenced_keys(&keys).await.expect("key check"),
        vec![stray],
        "each of the five key fields counts as a pointer"
    );
    assert_eq!(
        store.unreferenced_keys(&[]).await.expect("no keys"),
        Vec::<ObjectKey>::new()
    );
}

#[tokio::test]
async fn existing_gardens_are_those_with_a_record_being_deleted_or_not() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let gone: GardenId = f.ids.id();
    assert_eq!(
        store
            .existing_gardens(&[f.garden, gone])
            .await
            .expect("existing"),
        vec![f.garden]
    );
    exec(
        &store,
        &format!(
            "UPDATE {} SET deleting_at = time::now();",
            rec("garden", f.garden)
        ),
    )
    .await;
    assert_eq!(
        store
            .existing_gardens(&[gone, f.garden])
            .await
            .expect("existing"),
        vec![f.garden],
        "a garden being deleted still has its record"
    );
    assert_eq!(
        store.existing_gardens(&[]).await.expect("none asked"),
        Vec::<GardenId>::new()
    );
}
