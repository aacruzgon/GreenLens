//! Sentence splitting for the dose check: the "same sentence" conditions of contracts 7, and
//! the chat stream, where each sentence is checked before it is sent ([contracts 8], sending a
//! message).
//!
//! A sentence ends after `.`, `!`, `?` or `…` (and any closing quote or bracket) followed by
//! whitespace, or at a line break. A full stop does not end a sentence after a common
//! abbreviation ("e.g.") or after a list number at the start of a line ("1. Remove…"); a decimal
//! point ("0.5") is never followed by whitespace, so it never splits.

use std::ops::Range;

/// Words whose full stop doesn't end a sentence (compared lower-case, without the dot).
const ABBREVIATIONS: [&str; 13] = [
    "e.g", "i.e", "etc", "approx", "vs", "dr", "st", "no", "cf", "ca", "fig", "mr", "sq",
];

/// After this many bytes without a sentence end, the stream releases the buffer as one piece
/// rather than holding the reply back indefinitely. The whole piece is still checked.
pub const MAX_PENDING_BYTES: usize = 2_000;

/// Where sentences end in `text`: byte offsets just past each sentence's terminator and the
/// single whitespace character after it. When `complete` is false, a terminator at the very end
/// of `text` is undecided (the next delta may be a digit) and is not reported.
fn boundaries(text: &str, complete: bool) -> Vec<usize> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\n' {
            out.push(i + 1);
            continue;
        }
        if !matches!(c, '.' | '!' | '?' | '…') {
            continue;
        }
        let mut end = i + c.len_utf8();
        while let Some(&(j, q)) = chars.peek() {
            if matches!(q, '"' | '\'' | '”' | '’' | ')' | ']') {
                end = j + q.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        match chars.peek() {
            None if complete => out.push(end),
            Some(&(j, w)) if w.is_whitespace() && w != '\n' => {
                if c == '.' && !ends_sentence(&text[..i]) {
                    continue;
                }
                out.push(j + w.len_utf8());
                chars.next();
            }
            // Undecided at the end of a partial text, or followed by a non-space ("0.5").
            None | Some(_) => {}
        }
    }
    out
}

/// Whether a full stop after `before` ends a sentence.
fn ends_sentence(before: &str) -> bool {
    let line = before.rsplit('\n').next().unwrap_or(before);
    let word = line.rsplit(char::is_whitespace).next().unwrap_or(line);
    let lower = word.to_lowercase();
    if ABBREVIATIONS.contains(&lower.as_str()) {
        return false;
    }
    let is_list_number =
        !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) && line.trim_start() == word;
    !is_list_number
}

/// The sentences of a complete text, as trimmed byte ranges; whitespace-only pieces are
/// skipped. The ranges index `text`, so a caller can report where a sentence sits.
pub fn sentence_spans(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut cuts = boundaries(text, true);
    if cuts.last() != Some(&text.len()) {
        cuts.push(text.len());
    }
    let mut start = 0;
    cuts.into_iter().filter_map(move |end| {
        let piece = &text[start..end];
        let lead = piece.len() - piece.trim_start().len();
        let range = (start + lead)..(start + piece.trim_end().len());
        start = end;
        (!range.is_empty()).then_some(range)
    })
}

/// The sentences of a complete text, borrowed.
pub fn sentences(text: &str) -> impl Iterator<Item = &str> + '_ {
    sentence_spans(text).map(move |r| &text[r])
}

/// Buffers a reply's `text_delta`s and yields complete sentences, each with the whitespace that
/// followed it, so the pieces concatenate back into exactly the streamed text.
///
/// The API is owned and lifetime-free: `push` copies the delta into the buffer and returns owned
/// `String`s. The splitter lives in the turn's task across `.await` points (between model chunks)
/// and its sentences are moved into events sent over a channel, so nothing in it may borrow from
/// a chunk that is dropped at the next `.await`.
#[derive(Debug, Default, Clone)]
pub struct SentenceSplitter {
    buffer: String,
}

impl SentenceSplitter {
    /// An empty splitter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a delta and returns the sentences it completed, oldest first.
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.buffer.push_str(delta);
        let cut = boundaries(&self.buffer, false).last().copied();
        match cut {
            Some(end) => {
                let rest = self.buffer.split_off(end);
                let done = std::mem::replace(&mut self.buffer, rest);
                split_owned(&done)
            }
            None if self.buffer.len() >= MAX_PENDING_BYTES => {
                vec![std::mem::take(&mut self.buffer)]
            }
            None => Vec::new(),
        }
    }

    /// The text left when the reply ends: the last sentence, which may have no terminator.
    pub fn finish(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.buffer);
        (!rest.trim().is_empty()).then_some(rest)
    }

    /// Whether text is waiting for its sentence to end.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Splits completed text at every boundary, keeping each sentence's trailing whitespace.
fn split_owned(done: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for end in boundaries(done, true) {
        out.push(done[start..end].to_owned());
        start = end;
    }
    if start < done.len() {
        out.push(done[start..].to_owned());
    }
    out.retain(|s| !s.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_terminators_and_line_breaks() {
        let text = "Remove the leaves. Is it wet? Yes!\n- Mulch the soil\n- Water early";
        let got: Vec<_> = sentences(text).collect();
        assert_eq!(
            got,
            [
                "Remove the leaves.",
                "Is it wet?",
                "Yes!",
                "- Mulch the soil",
                "- Water early"
            ]
        );
    }

    #[test]
    fn decimals_abbreviations_and_list_numbers_do_not_split() {
        let text = "Use 0.5 of it, e.g. on the lower leaves.\n1. Remove spots. Then wait.";
        let got: Vec<_> = sentences(text).collect();
        assert_eq!(
            got,
            [
                "Use 0.5 of it, e.g. on the lower leaves.",
                "1. Remove spots.",
                "Then wait."
            ]
        );
        // `sq.` keeps an area together (EON gap G19).
        let got: Vec<_> = sentences("Cover 100 sq. m of bed. Then wait.").collect();
        assert_eq!(got, ["Cover 100 sq. m of bed.", "Then wait."]);
    }

    #[test]
    fn closing_quotes_stay_with_their_sentence() {
        let got: Vec<_> = sentences("He said \"stop.\" Then it rained.").collect();
        assert_eq!(got, ["He said \"stop.\"", "Then it rained."]);
    }

    #[test]
    fn streaming_waits_for_the_next_character_after_a_full_stop() {
        let mut s = SentenceSplitter::new();
        assert_eq!(s.push("Mix 0").len(), 0);
        assert_eq!(s.push(".").len(), 0, "0. might be 0.5");
        assert_eq!(s.push("5 of it").len(), 0);
        assert_eq!(s.push(". Then"), ["Mix 0.5 of it. "]);
        assert_eq!(
            s.push(" wait.").len(),
            0,
            "a full stop at the end is undecided"
        );
        assert_eq!(s.finish().as_deref(), Some("Then wait."));
        assert!(s.is_empty());
    }

    #[test]
    fn streamed_pieces_concatenate_to_the_text() {
        let text = "Remove spotted leaves. Bag them!\nWater at the base? Yes. Done";
        let mut s = SentenceSplitter::new();
        let mut out = String::new();
        for ch in text.chars() {
            for piece in s.push(&ch.to_string()) {
                out.push_str(&piece);
            }
        }
        out.push_str(&s.finish().unwrap_or_default());
        assert_eq!(out, text);
    }

    #[test]
    fn a_run_on_reply_is_released_at_the_cap() {
        let mut s = SentenceSplitter::new();
        let long = "word ".repeat(MAX_PENDING_BYTES / 5);
        assert_eq!(s.push(&long).len(), 1);
        assert!(s.is_empty());
    }
}
