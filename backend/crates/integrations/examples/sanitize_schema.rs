//! The structured-output schema sanitiser for the knowledge scripts: `ml/knowledge/extract.py`
//! sends the seed's JSON Schemas to the hosted model as `output_config.format`, after the same
//! cleaning the fallback worker and the plan writer apply ([training 10], profile extraction).
//!
//! ```text
//! cd backend
//! echo '{"type": "object", "properties": {"s": {"type": "string", "maxLength": 3}}}' \
//!   | cargo run -q -p integrations --example sanitize_schema
//! ```
//!
//! Reads a JSON Schema on stdin and writes `{"schema", "stripped": [{"at", "keyword", "value"}],
//! "closed": [pointer]}`: the schema to send, the keywords removed (for the caller to enforce
//! after decoding) and the objects that got `additionalProperties: false`. Exit code 2 when the
//! input isn't JSON.

use std::io::Read;

use integrations::anthropic::sanitize_schema;
use serde_json::Value;

fn main() {
    let mut raw = String::new();
    let schema: Value = match std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str(&raw).map_err(|e| e.to_string()))
    {
        Ok(schema) => schema,
        Err(e) => {
            #[allow(clippy::print_stderr, reason = "a command-line tool's error")]
            {
                eprintln!("sanitize_schema: can't read the input: {e}");
            }
            std::process::exit(2);
        }
    };
    let s = sanitize_schema(&schema);
    let out = serde_json::json!({
        "schema": s.schema,
        "stripped": s.stripped.iter().map(|k| serde_json::json!({
            "at": k.at, "keyword": k.keyword, "value": k.value,
        })).collect::<Vec<_>>(),
        "closed": s.closed,
    });
    #[allow(clippy::print_stdout, reason = "a command-line tool's output")]
    {
        println!("{out}");
    }
}
