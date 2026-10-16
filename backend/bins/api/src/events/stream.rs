//! `GET /scans/{id}/events` ([contracts 4], transport and when the stream closes): subscribe,
//! replay after `Last-Event-ID`, then forward live events, dropping `seq`s already sent.

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::response::sse::Event;
use futures::{Stream, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use domain::{
    authz::{Membership, Viewer},
    events::ScanEvent,
    id::ScanId,
    ports::StoreError,
    url::{Sign, Stored},
    wire::EventSeq,
};

use crate::state::AppState;

/// How long a live stream waits without an event before it reads the scan's state again. Some
/// changes reach a resting point without an event (a step claim released with nothing to say),
/// and deleting a scan emits none, so only a fresh read can close those streams.
pub const REST_CHECK: Duration = Duration::from_secs(15);

/// The stream body. It owns everything it uses (`'static`): the `Arc<AppState>`, the proof
/// (`Copy`), the scan id and the receiver, because axum moves the body into hyper's connection
/// task. In order:
///
/// 1. subscribe to the scan's broadcast first, so an event stored during the replay is in the
///    replay or in the receiver, never in neither;
/// 2. read the stream state, then replay every stored event after `last`; state first, so every
///    event stored before the read is replayed. A step commits its state before it emits its
///    last events, so a stream can see the scan at rest without them and close early; the app
///    refetches the scan on every close, which covers that gap;
/// 3. at rest: end after the replay (a resting scan replays then closes);
/// 4. live: drop `seq <= last`, but end on such a duplicate when it is flagged `rests`: the
///    resting event was stored after the state read and already went out in the replay, so
///    waiting for another event would keep a resting scan's stream open; a `seq` that skips
///    ahead, or `Lagged`, goes back to 2, and so does [`REST_CHECK`] without an event;
/// 5. end after an event flagged `rests`, when the scan is gone (being deleted: `stream_state`
///    answers `NotFound`), when the channel closes, on a store error (the app
///    reconnects with `Last-Event-ID`), or on shutdown: `take_until` ends the stream when the
///    token fires, without which `axum::serve`'s graceful shutdown would wait on it forever.
///
/// Each event is signed as it is sent ([contracts 4]: URLs signed on send, live or replayed).
pub fn scan_stream(
    state: Arc<AppState>,
    member: Membership<Viewer>,
    scan: ScanId,
    after: EventSeq,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let shutdown = state.shutdown.clone();
    let events = async_stream::stream! {
        let mut live = state.hub.subscribe(scan);
        let mut last = after;
        'replay: loop {
            let at_rest = match state.ports.store.stream_state(scan).await {
                Ok(s) => s.at_rest(),
                Err(StoreError::NotFound) => {
                    tracing::debug!(%scan, "event stream ended: scan gone");
                    return;
                }
                Err(error) => {
                    tracing::warn!(%scan, error = %error, "event stream ended: state unreadable");
                    return;
                }
            };
            let stored = match state.ports.store.events_after(member, scan, last).await {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!(%scan, error = %error, "event stream ended: replay failed");
                    return;
                }
            };
            for event in stored {
                if event.seq <= last {
                    continue;
                }
                last = event.seq;
                yield Ok(to_sse(&state, event));
            }
            if at_rest {
                return;
            }
            loop {
                // `recv` is cancel-safe: a timed-out wait loses no message.
                let Ok(received) = tokio::time::timeout(REST_CHECK, live.recv()).await else {
                    continue 'replay;
                };
                match received {
                    Ok(published) => {
                        let seq = published.event.seq;
                        if seq <= last {
                            if published.rests {
                                return;
                            }
                            continue;
                        }
                        if seq > last.next() {
                            continue 'replay;
                        }
                        last = seq;
                        yield Ok(to_sse(&state, published.event.clone()));
                        if published.rests {
                            return;
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue 'replay,
                    Err(RecvError::Closed) => return,
                }
            }
        }
    };
    events.take_until(shutdown.cancelled_owned())
}

/// `id: {seq}`, `event: {type}`, `data: {json}`, in the contract's order. The `Signer` borrows
/// the state's signing ports for the length of this call only.
fn to_sse(state: &AppState, event: ScanEvent<Stored>) -> Event {
    let signed = event.sign((), &state.signer());
    Event::default()
        .id(signed.seq.to_string())
        .event(signed.event_name())
        .json_data(&signed)
        .unwrap_or_else(|_| Event::default().comment("unserialisable event"))
}
