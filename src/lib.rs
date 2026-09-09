#![forbid(unsafe_code)]

//! The SQL content contract — a technology of `xmip-core-contract`.
//!
//! Two claims, decided 2026-09-07 (ADR-0042): **well-formedness is a given**
//! and **conformance is a given once a contract is named**.
//!
//! Well-formed here is *statement text that tokenizes and splits cleanly*:
//! UTF-8; every string literal, quoted identifier and block comment closed;
//! parentheses balanced within each statement; statements separated by `;`,
//! and each non-empty one beginning with a keyword this contract knows —
//! `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `MERGE`, `CREATE`, `ALTER`, `DROP`,
//! `TRUNCATE`, `GRANT`, `REVOKE`, `BEGIN`, `START`, `COMMIT`, `ROLLBACK`,
//! `WITH`, `CALL`, `VALUES` or `SET`. The text is ANSI SQL (ISO/IEC 9075),
//! vendor-neutral: nothing here knows a dialect's syntax past its first word,
//! and nothing executes.
//!
//! Conformance is the *allowed set of statement kinds*: a Location that names
//! this contract with `select,insert` or `read-only` bound has every statement
//! in every script held to that set, and one outside it is named by its kind
//! and position. The reference is a comma-separated list of kind words
//! (`select`, `insert`, `update`, `delete`, `merge`, `call`, and the rest of
//! the keywords above in lower case) and class words: `ddl` for `CREATE`,
//! `ALTER`, `DROP` and `TRUNCATE`; `dml` for `INSERT`, `UPDATE`, `DELETE` and
//! `MERGE`; `read-only` for `SELECT`, `WITH ... SELECT` and `VALUES`; `tcl`
//! for `BEGIN`, `START`, `COMMIT` and `ROLLBACK`; `dcl` for `GRANT` and
//! `REVOKE`. Every issue carries a `path` of the form `statement N`.

pub mod lexer;
pub mod statement;

use contract::{
    Contract, ContractDescriptor, ContractError, ContractFactory, ContractId, ValidationIssue,
    ValidationResult,
};
use statement::{Class, Kind, Script};
use std::collections::BTreeSet;
use stream::Stream;

/// The statement kinds a reference allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Allowed {
    kinds: BTreeSet<Kind>,
    reference: String,
}

impl Allowed {
    /// `select`, `read-only,call`, `dml, ddl`: kind and class words, comma
    /// separated, in any case.
    ///
    /// # Errors
    /// A word that is neither a kind nor a class, or an empty word.
    pub fn parse(reference: &str) -> Result<Self, ContractError> {
        let mut kinds = BTreeSet::new();
        let mut words = Vec::new();
        for word in reference.split(',').map(str::trim) {
            if let Some(class) = Class::of_name(word) {
                kinds.extend(class.members().iter().copied());
            } else if let Some(kind) = Kind::of_name(word) {
                kinds.insert(kind);
            } else {
                return Err(ContractError {
                    message: format!(
                        "{word:?} is not a statement kind or class; the kinds are {} and the \
                         classes {}",
                        names(Kind::ALL.iter().map(|k| k.name())),
                        names(Class::ALL.iter().map(|c| c.name())),
                    ),
                });
            }
            words.push(word.to_ascii_lowercase());
        }
        Ok(Self {
            kinds,
            reference: words.join(","),
        })
    }

    /// The reference as written, lower-cased and trimmed.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Whether `kind` is in the set.
    #[must_use]
    pub fn permits(&self, kind: Kind) -> bool {
        self.kinds.contains(&kind)
    }
}

fn names<'a>(words: impl Iterator<Item = &'a str>) -> String {
    words.collect::<Vec<_>>().join(", ")
}

/// The SQL contract, bare or bound to an allowed set of statement kinds.
pub struct SqlContract {
    descriptor: ContractDescriptor,
    allowed: Option<Allowed>,
}

impl SqlContract {
    /// Well-formed statement text, of any kinds.
    #[must_use]
    pub fn new() -> Self {
        Self {
            descriptor: descriptor("sql"),
            allowed: None,
        }
    }

    /// Well-formed statement text whose every statement is in `allowed`.
    #[must_use]
    pub fn allowing(allowed: Allowed) -> Self {
        Self {
            descriptor: descriptor(&format!("sql:{}", allowed.reference())),
            allowed: Some(allowed),
        }
    }

    /// Whether an allowed set is bound.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.allowed.is_some()
    }
}

impl Default for SqlContract {
    fn default() -> Self {
        Self::new()
    }
}

fn descriptor(id: &str) -> ContractDescriptor {
    ContractDescriptor {
        id: ContractId(id.to_string()),
        version: "1".to_string(),
        representation: "application/sql".to_string(),
    }
}

impl Contract for SqlContract {
    fn descriptor(&self) -> &ContractDescriptor {
        &self.descriptor
    }

    fn identify(&self, stream: &Stream) -> Result<bool, ContractError> {
        if stream.media_type().is_some_and(|m| {
            m.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("application/sql")
        }) {
            return Ok(true);
        }
        // Text whose first statement begins with a keyword is SQL enough to
        // claim; a bound set does not narrow identify — that is validate's
        // job, and an operator wants "not allowed" reported, not unclaimed.
        Ok(std::str::from_utf8(stream.bytes())
            .is_ok_and(|text| Script::parse(text).opens_recognised()))
    }

    fn validate(&self, stream: &Stream) -> Result<ValidationResult, ContractError> {
        let text = match std::str::from_utf8(stream.bytes()) {
            Ok(text) => text,
            Err(error) => {
                return Ok(result(vec![ValidationIssue {
                    code: "not-text".to_string(),
                    message: format!("not UTF-8 text: {error}"),
                    path: Some(format!("byte {}", error.valid_up_to())),
                }]));
            }
        };
        let script = Script::parse(text);
        let mut issues = script.issues;
        if let Some(allowed) = &self.allowed {
            for statement in &script.statements {
                let Some(kind) = statement.kind else {
                    continue;
                };
                if !allowed.permits(kind) {
                    issues.push(ValidationIssue {
                        code: "statement-not-allowed".to_string(),
                        message: format!(
                            "a {} statement at line {}; the contract allows {}",
                            kind.name().to_ascii_uppercase(),
                            statement.line,
                            allowed.reference()
                        ),
                        path: Some(statement.path()),
                    });
                }
            }
        }
        issues.sort_by_key(|issue| ordinal(issue.path.as_deref()));
        Ok(result(issues))
    }
}

/// The statement number a path names, for ordering issues by position.
fn ordinal(path: Option<&str>) -> usize {
    path.and_then(|p| p.strip_prefix("statement "))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn result(issues: Vec<ValidationIssue>) -> ValidationResult {
    ValidationResult {
        valid: issues.is_empty(),
        issues,
    }
}

/// Loads the contract a Location names: an empty reference is the bare
/// contract, anything else the allowed kinds, `select,insert` or `read-only`.
pub struct SqlFactory;

impl ContractFactory for SqlFactory {
    fn technology(&self) -> &'static str {
        "sql"
    }

    fn load(&self, reference: &str) -> Result<Box<dyn Contract>, ContractError> {
        if reference.trim().is_empty() {
            return Ok(Box::new(SqlContract::new()));
        }
        Ok(Box::new(SqlContract::allowing(Allowed::parse(reference)?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xcore::StreamId;

    const SCRIPT: &str = "-- a small migration\n\
        CREATE TABLE person (id INT PRIMARY KEY, name VARCHAR(40));\n\
        INSERT INTO person VALUES (1, 'O''Brien; -- not a comment');\n\
        /* the read\n   back */\n\
        WITH named AS (SELECT * FROM person WHERE name <> '')\n\
        SELECT count(*) FROM named;\n\
        COMMIT;\n";

    fn stream(text: &str, media_type: Option<&str>) -> Stream {
        Stream::new(
            StreamId::new(1),
            text.as_bytes().to_vec(),
            media_type.map(str::to_string),
        )
    }

    fn codes(result: &ValidationResult) -> Vec<(&str, &str)> {
        result
            .issues
            .iter()
            .map(|i| (i.code.as_str(), i.path.as_deref().unwrap_or("")))
            .collect()
    }

    #[test]
    fn a_clean_script_holds_bare_and_is_identified() {
        let bare = SqlContract::new();
        assert_eq!(bare.descriptor().representation, "application/sql");
        assert!(!bare.is_bound());
        assert!(bare.identify(&stream(SCRIPT, None)).expect("identify"));
        assert!(
            bare.identify(&stream("x", Some("application/sql; charset=utf-8")))
                .expect("identify")
        );
        assert!(!bare.identify(&stream("<xml/>", None)).expect("identify"));
        assert!(!bare.identify(&stream("", None)).expect("identify"));
        let held = bare.validate(&stream(SCRIPT, None)).expect("validate");
        assert!(held.valid, "{:?}", held.issues);
    }

    #[test]
    fn a_string_hides_its_separator_and_comment_marker() {
        let held = SqlContract::new()
            .validate(&stream(
                "SELECT 'a;b -- c' FROM t; SELECT \"x;y\" FROM t",
                None,
            ))
            .expect("validate");
        assert!(held.valid, "{:?}", held.issues);
    }

    #[test]
    fn what_does_not_tokenize_or_begin_as_a_statement_is_named() {
        let bare = SqlContract::new();
        let held = bare
            .validate(&stream("SELECT 1;\nSELECT 2 /* never closed\n", None))
            .expect("validate");
        assert_eq!(codes(&held), [("unterminated-comment", "statement 2")]);
        assert_eq!(
            held.issues[0].message,
            "the comment opened at line 2 never closes"
        );
        let held = bare
            .validate(&stream("SELECT (1;\nEXPLAIN SELECT 1", None))
            .expect("validate");
        assert_eq!(
            codes(&held),
            [
                ("unbalanced-parenthesis", "statement 1"),
                ("unknown-statement", "statement 2")
            ]
        );
        let held = bare
            .validate(&Stream::new(StreamId::new(1), vec![0x53, 0xff], None))
            .expect("validate");
        assert_eq!(codes(&held), [("not-text", "byte 1")]);
    }

    #[test]
    fn a_bound_set_refuses_what_is_outside_it() {
        let bound = SqlFactory.load("dml, Read-Only").expect("load");
        assert_eq!(bound.descriptor().id.0, "sql:dml,read-only");
        let held = bound.validate(&stream(SCRIPT, None)).expect("validate");
        assert!(!held.valid);
        assert_eq!(
            codes(&held),
            [
                ("statement-not-allowed", "statement 1"),
                ("statement-not-allowed", "statement 4")
            ]
        );
        assert_eq!(
            held.issues[0].message,
            "a CREATE statement at line 2; the contract allows dml,read-only"
        );
        assert!(bound.identify(&stream(SCRIPT, None)).expect("identify"));
        let read_only = SqlFactory.load("read-only").expect("load");
        let held = read_only.validate(&stream(SCRIPT, None)).expect("validate");
        assert_eq!(held.issues.len(), 3);
        let all = SqlFactory
            .load("ddl,dml,read-only,tcl,dcl,call,set")
            .expect("load");
        assert!(all.validate(&stream(SCRIPT, None)).expect("validate").valid);
    }

    #[test]
    fn issues_from_both_claims_come_in_statement_order() {
        let bound = SqlContract::allowing(Allowed::parse("select").expect("parse"));
        let held = bound
            .validate(&stream("DROP TABLE t;\nSELECT (1;\nSELECT 'x", None))
            .expect("validate");
        assert_eq!(
            codes(&held),
            [
                ("statement-not-allowed", "statement 1"),
                ("unbalanced-parenthesis", "statement 2"),
                ("unterminated-string", "statement 3")
            ]
        );
    }

    #[test]
    fn the_factory_refuses_a_word_it_does_not_know() {
        assert_eq!(SqlFactory.technology(), "sql");
        assert_eq!(SqlFactory.load(" ").expect("bare").descriptor().id.0, "sql");
        let error = SqlFactory.load("select,explain").err().expect("refused");
        assert!(
            error
                .message
                .starts_with("\"explain\" is not a statement kind or class")
        );
        assert!(SqlFactory.load("select,").is_err());
        assert!(SqlContract::allowing(Allowed::parse("call").expect("parse")).is_bound());
        assert!(Allowed::parse("tcl").expect("parse").permits(Kind::Start));
        assert!(!Allowed::parse("tcl").expect("parse").permits(Kind::Set));
    }
}
