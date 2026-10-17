//! The context's beliefs block: loaded beside the memory search, rendered before the memories,
//! left out when Agent Memory is slow.

use std::{sync::Arc, time::Duration};

use domain::{
    id::GardenId,
    ports::{Embedding, MemoryHit, MockBeliefStore, MockMemoryStore},
    wire::Role,
};

use super::*;
use crate::{
    fixtures,
    testing::{GARDEN, SlowBeliefs, garden_deps, owner},
};

const CURRENT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E70";

fn memory_with_one_hit() -> MockMemoryStore {
    let mut memory = MockMemoryStore::new();
    memory.expect_search().returning(|_, _, _, _| {
        Ok(vec![MemoryHit {
            memory: fixtures::memory(
                "01JAX3K9T6R2B8V0QZ4M7D1E71",
                "user",
                true,
                "Watered every 3 days.",
            ),
            similarity: 0.8,
        }])
    });
    memory
}

fn texts(message: &ChatMessage) -> Vec<&str> {
    message
        .content
        .iter()
        .filter_map(|c| match c {
            ContentPart::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn current_beliefs_are_rendered_before_the_memories() {
    let garden: GardenId = GARDEN.parse().unwrap();
    let mut beliefs = MockBeliefStore::new();
    beliefs.expect_enabled().return_const(true);
    beliefs
        .expect_current()
        .withf(move |g, p| *g == garden && p.is_none())
        .times(1)
        .returning(|_, _| {
            Ok(vec![fixtures::belief(
                "watering",
                "Watered every 2 days.",
                false,
            )])
        });
    let deps = garden_deps(memory_with_one_hit(), Arc::new(beliefs));
    let ctx = load(
        &deps,
        owner(Role::Editor).await,
        Anchor::Garden(garden),
        CURRENT.parse().unwrap(),
        Some(&Embedding(vec![0.1; 4])),
    )
    .await
    .unwrap();
    assert_eq!(ctx.beliefs.len(), 1);

    let message = ctx.turn_message("How often should I water?", Vec::new());
    let parts = texts(&message);
    assert!(parts[0].starts_with("<data kind=\"context\">"));
    assert_eq!(
        parts[1],
        "<data kind=\"beliefs\">\n[{\"key\":\"watering\",\"value\":\"Watered every 2 days.\",\"about_plant\":false}]\n</data>"
    );
    assert!(parts[2].starts_with("<data kind=\"memories\">"));
    assert!(parts[2].contains("Watered every 3 days."));
    assert!(parts[3].starts_with("<data kind=\"user_message\">"));
}

#[tokio::test(start_paused = true)]
async fn a_slow_belief_store_leaves_the_block_out_within_the_timeout() {
    let deps = garden_deps(memory_with_one_hit(), Arc::new(SlowBeliefs));
    let start = tokio::time::Instant::now();
    let ctx = load(
        &deps,
        owner(Role::Editor).await,
        Anchor::Garden(GARDEN.parse().unwrap()),
        CURRENT.parse().unwrap(),
        Some(&Embedding(vec![0.1; 4])),
    )
    .await
    .unwrap();
    assert_eq!(start.elapsed(), Duration::from_millis(1_500));
    assert_eq!(ctx.beliefs, []);
    assert_eq!(ctx.memories.len(), 1, "the memory hits still arrive");
    let message = ctx.turn_message("hi", Vec::new());
    assert!(
        texts(&message)
            .iter()
            .all(|t| !t.starts_with("<data kind=\"beliefs\">"))
    );
}
