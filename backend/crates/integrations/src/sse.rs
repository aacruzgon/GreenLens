//! Server-sent events from an upstream: the Anthropic Messages stream and OpenAI-compatible
//! streams. A small hand-written parser, so it is easy to test with split chunks.

use bytes::Bytes;
use futures::{Stream, StreamExt, stream};

/// One dispatched event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseEvent {
    /// The `event:` field; `"message"` when absent.
    pub event: String,
    /// The `data:` lines joined with `\n`.
    pub data: String,
}

/// An incremental parser for the subset of the WHATWG rules both upstreams use: `event:`,
/// `data:` (several lines joined with `\n`), `:` comments, and a blank line to dispatch. CRLF and
/// LF both end a line; a chunk may end anywhere, even inside a UTF-8 sequence, because bytes
/// are buffered until a whole line is in.
#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    current: SseEvent,
    has_data: bool,
}

impl SseParser {
    /// Feeds a chunk and returns the events it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line);
            if line.is_empty() {
                if self.has_data {
                    let mut event = std::mem::take(&mut self.current);
                    if event.event.is_empty() {
                        "message".clone_into(&mut event.event);
                    }
                    out.push(event);
                }
                self.current = SseEvent::default();
                self.has_data = false;
            } else if !line.starts_with(':') {
                let (field, value) = line.split_once(':').unwrap_or((&line, ""));
                let value = value.strip_prefix(' ').unwrap_or(value);
                match field {
                    "event" => value.clone_into(&mut self.current.event),
                    "data" => {
                        if self.has_data {
                            self.current.data.push('\n');
                        }
                        self.current.data.push_str(value);
                        self.has_data = true;
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

/// Turns a byte stream into an event stream. Generic over the chunk stream `S` and its error
/// `E` so tests feed it hand-made chunks; the output owns everything it needs, so it is
/// `'static` whenever `S` is (an upstream `reqwest::Response::bytes_stream()` is).
pub fn events<S, E>(bytes: S) -> impl Stream<Item = Result<SseEvent, E>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + Unpin,
    E: Send,
{
    stream::unfold(
        (
            bytes,
            SseParser::default(),
            std::collections::VecDeque::new(),
        ),
        |(mut bytes, mut parser, mut pending)| async move {
            loop {
                if let Some(event) = pending.pop_front() {
                    return Some((Ok(event), (bytes, parser, pending)));
                }
                match bytes.next().await {
                    Some(Ok(chunk)) => pending.extend(parser.push(&chunk)),
                    Some(Err(e)) => return Some((Err(e), (bytes, parser, pending))),
                    None => return None,
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_split_crlf_and_multiline() {
        let raw =
            "event: ping\r\ndata: {\"type\":\"ping\"}\r\n\r\n: keep-alive\n\ndata: a\ndata: é\n\n";
        let mut p = SseParser::default();
        let mut all = Vec::new();
        for b in raw.as_bytes() {
            all.extend(p.push(std::slice::from_ref(b)));
        }
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].event, "ping");
        assert_eq!(all[1].event, "message");
        assert_eq!(all[1].data, "a\né");
    }

    #[tokio::test]
    async fn stream_adapter() {
        let chunks: Vec<Result<Bytes, ()>> = vec![
            Ok(Bytes::from_static(b"event: a\nda")),
            Ok(Bytes::from_static(b"ta: 1\n\nevent: b\ndata: 2\n\n")),
        ];
        let out: Vec<_> = events(stream::iter(chunks)).collect().await;
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].as_ref().unwrap().data, "2");
    }
}
