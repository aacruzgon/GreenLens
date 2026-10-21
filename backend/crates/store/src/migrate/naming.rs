//! The names a file in `backend/migrations/` may have ([database 1], applying the migrations).
//! Shared by `build.rs`, which refuses a bad name or a duplicate number at build time, and by
//! the runner, which tells the kinds apart.
//!
//! [database 1]: https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md#1-version-and-setup

/// The schema, applied on every start and never recorded.
pub const SCHEMA_FILE: &str = "schema.surql";
/// The knowledge loader's file, applied last when its hash changes.
pub const KNOWLEDGE_FILE: &str = "knowledge.surql";

/// What a migration file is, from its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// `schema.surql`.
    Schema,
    /// `NNNN_<slug>.surql`, with its number (1 or more).
    Versioned(u16),
    /// `knowledge.surql`.
    Knowledge,
}

/// The kind of a `*.surql` file name, or why the name is refused: `schema.surql`,
/// `knowledge.surql`, or 4 digits from `0001`, `_`, then lower-case words (letters and digits)
/// joined by `_`.
pub fn file_kind(name: &str) -> Result<FileKind, String> {
    if name == SCHEMA_FILE {
        return Ok(FileKind::Schema);
    }
    if name == KNOWLEDGE_FILE {
        return Ok(FileKind::Knowledge);
    }
    let refuse = || {
        format!(
            "{name}: a migration is named NNNN_<slug>.surql (4 digits from 0001, then lower-case \
             words joined by _), schema.surql or knowledge.surql"
        )
    };
    let stem = name.strip_suffix(".surql").ok_or_else(refuse)?;
    let (digits, slug) = stem.split_at_checked(4).ok_or_else(refuse)?;
    let slug = slug.strip_prefix('_').ok_or_else(refuse)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refuse());
    }
    let words_ok = slug.split('_').all(|word| {
        !word.is_empty()
            && word
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    });
    if !words_ok {
        return Err(refuse());
    }
    match digits.parse::<u16>() {
        Ok(number) if number > 0 => Ok(FileKind::Versioned(number)),
        _ => Err(refuse()),
    }
}
