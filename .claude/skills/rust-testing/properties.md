# Property tests

Part of [`rust-testing`](SKILL.md). Each property below states a rule from the contracts and checks it over generated inputs with proptest 1.12.0 ([proptest book](https://proptest-rs.github.io/proptest/proptest/tutorial/macro-proptest.html)). The functions are small stand-ins written next to their tests; the real ones live in `domain` and `api` and get the same tests. The backend plans property tests for the dose check, scoring, the scan state machine and RLE round-trips too ([backend README 11](../../../docs/backend/README.md#11-testing-and-tooling)); they follow the same rules: the expected value comes from the document, and exact boundaries go in a plain table.

## Confidence bands

[Contracts §1, wire format](../../../docs/architecture/contracts.md#wire-format): **high** ≥ 0.8, **medium** ≥ 0.5, **low** below that, computed by the API. The tests take their expected bands from that sentence, never from the function's own conditions: a table of examples written from the contract, and one property per band whose strategy is the contract's range for it. A float strategy almost never produces exactly 0.8 or 0.5, so the boundaries are in the example table.

```rust
use proptest::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ConfidenceBand {
    Low,
    Medium,
    High,
}

fn confidence_band(confidence: f64) -> ConfidenceBand {
    if confidence >= 0.8 {
        ConfidenceBand::High
    } else if confidence >= 0.5 {
        ConfidenceBand::Medium
    } else {
        ConfidenceBand::Low
    }
}

/// Examples written from contracts §1, not from the function: (confidence, band).
const CONTRACT_EXAMPLES: [(f64, ConfidenceBand); 9] = [
    (0.0, ConfidenceBand::Low),
    (0.3, ConfidenceBand::Low),
    (0.499_999, ConfidenceBand::Low),
    (0.5, ConfidenceBand::Medium),
    (0.65, ConfidenceBand::Medium),
    (0.799_999, ConfidenceBand::Medium),
    (0.8, ConfidenceBand::High),
    (0.93, ConfidenceBand::High),
    (1.0, ConfidenceBand::High),
];

#[test]
fn bands_match_the_contract_examples() {
    for (confidence, band) in CONTRACT_EXAMPLES {
        assert_eq!(confidence_band(confidence), band, "confidence {confidence}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // The ranges are the contract's: [0.8, 1] high, [0.5, 0.8) medium, [0, 0.5) low.
    #[test]
    fn every_value_in_the_high_range_is_high(c in 0.8..=1.0f64) {
        prop_assert_eq!(confidence_band(c), ConfidenceBand::High);
    }

    #[test]
    fn every_value_in_the_medium_range_is_medium(c in 0.5..0.8f64) {
        prop_assert_eq!(confidence_band(c), ConfidenceBand::Medium);
    }

    #[test]
    fn every_value_in_the_low_range_is_low(c in 0.0..0.5f64) {
        prop_assert_eq!(confidence_band(c), ConfidenceBand::Low);
    }

    #[test]
    fn band_never_drops_as_confidence_rises(a in 0.0..=1.0f64, b in 0.0..=1.0f64) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(confidence_band(lo) <= confidence_band(hi));
    }
}
```

## Region boxes

[Contracts §1](../../../docs/architecture/contracts.md#wire-format): a `BBox` is `[x, y, w, h]`, each from 0 to 1, origin top-left, normalised to the stored image; [§6](../../../docs/architecture/contracts.md#image-preparation) has the worker divide each pixel box by the image's width and height. The property: any pixel box inside the image normalises into the unit square.

```rust
use proptest::prelude::*;

/// A pixel box `[x, y, w, h]` on a `width` × `height` image, normalised to 0..1.
fn normalise(px: [u32; 4], width: u32, height: u32) -> [f64; 4] {
    let (w, h) = (f64::from(width), f64::from(height));
    [
        f64::from(px[0]) / w,
        f64::from(px[1]) / h,
        f64::from(px[2]) / w,
        f64::from(px[3]) / h,
    ]
}

/// A box that fits inside a `width` × `height` image.
fn pixel_box(width: u32, height: u32) -> impl Strategy<Value = [u32; 4]> {
    (0..width, 0..height).prop_flat_map(move |(x, y)| {
        (Just(x), Just(y), 0..=width - x, 0..=height - y).prop_map(|(x, y, w, h)| [x, y, w, h])
    })
}

proptest! {
    #[test]
    fn boxes_inside_the_image_stay_in_the_unit_square(
        (width, height, px) in (1u32..=8192, 1u32..=8192)
            .prop_flat_map(|(w, h)| (Just(w), Just(h), pixel_box(w, h)))
    ) {
        let [x, y, w, h] = normalise(px, width, height);
        for v in [x, y, w, h] {
            prop_assert!((0.0..=1.0).contains(&v), "{v} outside 0..1");
        }
        prop_assert!(x + w <= 1.0 + f64::EPSILON);
        prop_assert!(y + h <= 1.0 + f64::EPSILON);
    }
}
```

## SSE `seq` dedupe

[Contracts §4, transport](../../../docs/architecture/contracts.md#transport): `seq` comes from the append's transaction, gap-free per scan ([contracts 12](../../../docs/architecture/contracts.md#12-open-questions), question 22, settled; [backend store 21](../../../docs/backend/store.md#21-decisions-on-open-questions)). On resume the server subscribes to the live broadcast, replays every stored event after `Last-Event-ID`, then forwards live events, dropping any `seq` it has already sent. Live events can overlap the end of the replay. The property: whatever the overlap, every `seq` after `Last-Event-ID` is sent exactly once, in order, with no gaps.

```rust
use proptest::prelude::*;

/// Forwards each `seq` once, in increasing order.
struct SeqDedupe {
    last_sent: u64,
}

impl SeqDedupe {
    fn admit(&mut self, seq: u64) -> bool {
        if seq > self.last_sent {
            self.last_sent = seq;
            true
        } else {
            false
        }
    }
}

proptest! {
    #[test]
    fn replay_then_live_sends_each_seq_once(
        last_event_id in 0u64..50,
        stored in 1u64..100,
        overlap in 0u64..10,
        new_live in 0u64..10,
    ) {
        let live_from = stored.saturating_sub(overlap).max(1);
        let replay = last_event_id + 1..=stored;
        let live = live_from..=stored + new_live;
        let mut dedupe = SeqDedupe { last_sent: last_event_id };
        let sent: Vec<u64> = replay.chain(live).filter(|&seq| dedupe.admit(seq)).collect();
        let expected: Vec<u64> = (last_event_id + 1..=stored + new_live).collect();
        prop_assert_eq!(sent, expected);
    }
}
```

## The multipart idempotency fingerprint

[Contracts §1, idempotency](../../../docs/architecture/contracts.md#idempotency): for multipart uploads the fingerprint is "a hash of the form fields and the file bytes, never the raw body, whose boundary changes on each retry". The properties: the same form sent with two different boundaries gives byte-for-byte different bodies but the same fingerprint input; and a different form (any image byte changed, or another `kind`) gives a different one, so a constant fingerprint can't pass. The parts are read with axum's `Multipart` extractor, the one the handler uses ([`Multipart`](https://docs.rs/axum/0.8.9/axum/extract/struct.Multipart.html)). Which hash runs over the input, and how it is stored, is up to the idempotency code in [`rust-axum-api`](../rust-axum-api/SKILL.md).

```rust
use axum::body::Body;
use axum::extract::{FromRequest, Multipart};
use axum::http::{Request, header};
use proptest::prelude::*;

/// Every part's name, file name, content type and bytes, each length-prefixed.
async fn fingerprint_input(mut form: Multipart) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(field) = form.next_field().await.expect("valid multipart") {
        let name = field.name().unwrap_or_default().to_owned();
        let file_name = field.file_name().unwrap_or_default().to_owned();
        let content_type = field.content_type().unwrap_or_default().to_owned();
        let bytes = field.bytes().await.expect("part bytes");
        for part in [
            name.as_bytes(),
            file_name.as_bytes(),
            content_type.as_bytes(),
            &bytes,
        ] {
            let len = u64::try_from(part.len()).expect("part length fits u64");
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(part);
        }
    }
    out
}

fn multipart_body(boundary: &str, kind: &str, image: &[u8]) -> Vec<u8> {
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\n{kind}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"leaf.jpg\"\r\n\
         Content-Type: image/jpeg\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(image);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn parse(boundary: &str, body: Vec<u8>) -> Vec<u8> {
    let request = Request::post("/scans/01JAX3K9T6R2B8V0QZ4M7D1E5F/images")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("request");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let form = Multipart::from_request(request, &())
            .await
            .expect("multipart");
        fingerprint_input(form).await
    })
}

proptest! {
    #[test]
    fn the_boundary_never_changes_the_fingerprint(
        a in "[A-Za-z0-9]{16,40}",
        b in "[A-Za-z0-9]{16,40}",
        kind in "(whole_plant|leaf_closeup|soil)",
        image in proptest::collection::vec(any::<u8>(), 0..2048),
    ) {
        prop_assume!(a != b);
        let body_a = multipart_body(&a, &kind, &image);
        let body_b = multipart_body(&b, &kind, &image);
        prop_assert_ne!(&body_a, &body_b);
        prop_assert_eq!(parse(&a, body_a), parse(&b, body_b));
    }

    // Without this, a constant fingerprint would pass the property above.
    #[test]
    fn changing_any_image_byte_changes_the_fingerprint(
        image in proptest::collection::vec(any::<u8>(), 1..2048),
        at in any::<proptest::sample::Index>(),
        flip in 1u8..=255,
    ) {
        let boundary = "pdm0123456789boundary";
        let mut changed = image.clone();
        let i = at.index(changed.len());
        changed[i] ^= flip;
        prop_assert_ne!(
            parse(boundary, multipart_body(boundary, "leaf_closeup", &image)),
            parse(boundary, multipart_body(boundary, "leaf_closeup", &changed))
        );
    }

    #[test]
    fn a_different_kind_changes_the_fingerprint(
        image in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let boundary = "pdm0123456789boundary";
        prop_assert_ne!(
            parse(boundary, multipart_body(boundary, "whole_plant", &image)),
            parse(boundary, multipart_body(boundary, "soil", &image))
        );
    }
}
```

`proptest!` bodies are synchronous, so the async extractor runs on a current-thread runtime built per case ([`Builder::new_current_thread`](https://docs.rs/tokio/1.53.3/tokio/runtime/struct.Builder.html#method.new_current_thread)). `prop_assume!` drops a generated case instead of failing it ([`prop_assume`](https://docs.rs/proptest/1.12.0/proptest/macro.prop_assume.html)). `Index` picks a position in a collection whose length is only known inside the test ([`sample::Index`](https://docs.rs/proptest/1.12.0/proptest/sample/struct.Index.html)).
