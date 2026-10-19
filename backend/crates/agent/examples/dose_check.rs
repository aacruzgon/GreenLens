//! The dose check ([contracts 7]) for the knowledge loader: `ml/knowledge/validate.py` runs it
//! on every treatment text and soil condition, so the seed is checked by the same code as plans
//! and chat replies (contracts 10, checks).
//!
//! ```text
//! cd backend
//! echo '{"products": {"products": []}, "texts": [{"id": "t", "text": "Use 2 tbsp."}]}' \
//!   | cargo run -q -p agent --example dose_check
//! ```
//!
//! Reads `{"products": ProductsSeed, "texts": [{"id", "text"}]}` on stdin and writes one JSON
//! line per failing text, `{"id", "rule", "matched", "reason"}`. Exit code 0 whether or not a
//! text failed; 2 when the input can't be read.

use std::io::{Read, Write};

use agent::safety::{dose::dose_check, products::ProductCatalogue};
use domain::knowledge::ProductsSeed;
use serde::Deserialize;

#[derive(Deserialize)]
struct Input {
    products: ProductsSeed,
    texts: Vec<Text>,
}

#[derive(Deserialize)]
struct Text {
    id: String,
    text: String,
}

fn main() {
    let mut raw = String::new();
    let input: Input = match std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str(&raw).map_err(|e| e.to_string()))
    {
        Ok(input) => input,
        Err(e) => {
            #[allow(clippy::print_stderr, reason = "a command-line tool's error")]
            {
                eprintln!("dose_check: can't read the input: {e}");
            }
            std::process::exit(2);
        }
    };
    let products = ProductCatalogue::new(&input.products);
    let mut out = std::io::stdout().lock();
    for t in &input.texts {
        if let Some(m) = dose_check(&t.text, &products) {
            let line = serde_json::json!({
                "id": t.id,
                "rule": format!("{:?}", m.rule),
                "matched": m.matched,
                "reason": m.reason(),
            });
            // reason: a closed stdout means the caller stopped reading; nothing to report to.
            let _ = writeln!(out, "{line}");
        }
    }
}
