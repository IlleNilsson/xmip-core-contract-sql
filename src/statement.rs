//! Statements: splitting a token run at `;`, classifying each by the keyword
//! it begins with — its [`Verb`], `xmip-core-library-sql`'s — and the
//! classes the kinds fall into.
//!
//! A statement's kind is the first word of it, except `WITH`: a common table
//! expression introduces the statement that follows its definitions, so the
//! kind is the first `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `MERGE` or
//! `VALUES` at parenthesis depth zero after it. Every issue is placed by
//! `statement N`, counting the non-empty statements from 1.

use crate::lexer::{Token, tokenize};
use contract::ValidationIssue;
use sql::Verb;

/// A class of statement kinds a reference may name as one word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// `CREATE`, `ALTER`, `DROP`, `TRUNCATE`.
    Ddl,
    /// `INSERT`, `UPDATE`, `DELETE`, `MERGE`.
    Dml,
    /// `SELECT`, `WITH ... SELECT`, `VALUES`.
    ReadOnly,
    /// `BEGIN`, `START`, `COMMIT`, `ROLLBACK`.
    Tcl,
    /// `GRANT`, `REVOKE`.
    Dcl,
}

impl Class {
    /// Every class.
    pub const ALL: [Class; 5] = [
        Class::Ddl,
        Class::Dml,
        Class::ReadOnly,
        Class::Tcl,
        Class::Dcl,
    ];

    /// The class `name` names, case-insensitively.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Class> {
        Class::ALL
            .into_iter()
            .find(|class| class.name().eq_ignore_ascii_case(name))
    }

    /// The class's reference word.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Class::Ddl => "ddl",
            Class::Dml => "dml",
            Class::ReadOnly => "read-only",
            Class::Tcl => "tcl",
            Class::Dcl => "dcl",
        }
    }

    /// The class `verb` belongs to; `CALL` and `SET` belong to none.
    #[must_use]
    pub fn of_verb(verb: Verb) -> Option<Class> {
        Class::ALL
            .into_iter()
            .find(|class| class.members().contains(&verb))
    }

    /// The kinds in the class.
    #[must_use]
    pub fn members(self) -> &'static [Verb] {
        match self {
            Class::Ddl => &[Verb::Create, Verb::Alter, Verb::Drop, Verb::Truncate],
            Class::Dml => &[Verb::Insert, Verb::Update, Verb::Delete, Verb::Merge],
            Class::ReadOnly => &[Verb::Select, Verb::Values],
            Class::Tcl => &[Verb::Begin, Verb::Start, Verb::Commit, Verb::Rollback],
            Class::Dcl => &[Verb::Grant, Verb::Revoke],
        }
    }
}

/// One non-empty statement: where it began, what it began with, its kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// Counted from 1 over the non-empty statements.
    pub ordinal: usize,
    pub line: usize,
    /// The first token's text as written.
    pub keyword: String,
    /// `None` when the first word is not a recognized keyword.
    pub kind: Option<Verb>,
}

impl Statement {
    /// `statement N`, the path every issue in it carries.
    #[must_use]
    pub fn path(&self) -> String {
        format!("statement {}", self.ordinal)
    }
}

/// A script read into statements, with every well-formedness issue found.
#[derive(Debug, Default)]
pub struct Script {
    pub statements: Vec<Statement>,
    pub issues: Vec<ValidationIssue>,
}

impl Script {
    /// Reads `text`: tokens, then statements, then the issues.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let lexed = tokenize(text);
        let mut script = Script::default();
        let mut open: Vec<&Token> = Vec::new();
        for token in &lexed.tokens {
            if token.is_punct(';') {
                script.close(&open);
                open.clear();
            } else {
                open.push(token);
            }
        }
        let trailing = !open.is_empty();
        script.close(&open);
        if let Some(error) = lexed.error {
            // The construct that never closed is in the statement being read:
            // the trailing one, or the one after the last `;`.
            let ordinal = script.statements.len() + usize::from(!trailing);
            script
                .issues
                .push(issue(error.code, &error.message, ordinal));
        }
        script
    }

    /// Records the statement `tokens` form, if any, and its issues.
    fn close(&mut self, tokens: &[&Token]) {
        let Some(first) = tokens.first() else {
            return;
        };
        let ordinal = self.statements.len() + 1;
        if let Some(message) = imbalance(tokens) {
            self.issues
                .push(issue("unbalanced-parenthesis", &message, ordinal));
        }
        let (kind, unknown) = classify(tokens);
        if let Some(message) = unknown {
            self.issues
                .push(issue("unknown-statement", &message, ordinal));
        }
        self.statements.push(Statement {
            ordinal,
            line: first.line,
            keyword: first.text.clone(),
            kind,
        });
    }

    /// The first statement's kind, or whether it at least begins with `WITH`.
    #[must_use]
    pub fn opens_recognized(&self) -> bool {
        self.statements.first().is_some_and(|statement| {
            statement.kind.is_some() || statement.keyword.eq_ignore_ascii_case("WITH")
        })
    }
}

/// Why the parentheses do not balance, or `None` when they do.
fn imbalance(tokens: &[&Token]) -> Option<String> {
    let mut depth = 0usize;
    let mut opened = 0;
    for token in tokens {
        if token.is_punct('(') {
            if depth == 0 {
                opened = token.line;
            }
            depth += 1;
        } else if token.is_punct(')') {
            if depth == 0 {
                return Some(format!("a `)` at line {} closes nothing", token.line));
            }
            depth -= 1;
        }
    }
    (depth > 0).then(|| format!("the `(` at line {opened} never closes"))
}

/// The statement's kind, and the message when it has none.
fn classify(tokens: &[&Token]) -> (Option<Verb>, Option<String>) {
    let first = tokens[0];
    let Some(word) = first.word() else {
        let message = format!(
            "begins with {:?} at line {}, not a keyword",
            first.text, first.line
        );
        return (None, Some(message));
    };
    if word != "WITH" {
        let Some(kind) = Verb::of_name(&word) else {
            let message = format!(
                "begins with {word} at line {}, not a recognized statement",
                first.line
            );
            return (None, Some(message));
        };
        return (Some(kind), None);
    }
    let mut depth = 0usize;
    for token in &tokens[1..] {
        if token.is_punct('(') {
            depth += 1;
        } else if token.is_punct(')') {
            depth = depth.saturating_sub(1);
        } else if depth == 0
            && let Some(kind) = token.word().and_then(|w| Verb::of_name(&w))
            && WITH_BODIES.contains(&kind)
        {
            return (Some(kind), None);
        }
    }
    let message = format!(
        "WITH at line {} introduces no SELECT, INSERT, UPDATE, DELETE, MERGE or VALUES",
        first.line
    );
    (None, Some(message))
}

/// What a common table expression may introduce.
const WITH_BODIES: [Verb; 6] = [
    Verb::Select,
    Verb::Insert,
    Verb::Update,
    Verb::Delete,
    Verb::Merge,
    Verb::Values,
];

fn issue(code: &'static str, message: &str, ordinal: usize) -> ValidationIssue {
    ValidationIssue::at(code, message, format!("statement {ordinal}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Option<Verb>> {
        Script::parse(text)
            .statements
            .iter()
            .map(|s| s.kind)
            .collect()
    }

    #[test]
    fn a_script_splits_at_semicolons_and_classifies_each_statement() {
        let script = Script::parse(
            "-- setup\nCREATE TABLE t (id INT);\n\nINSERT INTO t VALUES (1);;\n\
             select * from t where n = 'a;b';\nCOMMIT",
        );
        assert!(script.issues.is_empty(), "{:?}", script.issues);
        let kinds: Vec<Verb> = script.statements.iter().filter_map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [Verb::Create, Verb::Insert, Verb::Select, Verb::Commit]
        );
        assert_eq!(script.statements[2].ordinal, 3);
        assert_eq!(script.statements[2].line, 5);
        assert_eq!(script.statements[2].keyword, "select");
        assert_eq!(script.statements[2].path(), "statement 3");
        assert!(script.opens_recognized());
        assert!(!Script::parse("").opens_recognized());
        assert!(!Script::parse("EXPLAIN SELECT 1").opens_recognized());
    }

    #[test]
    fn with_takes_the_kind_of_what_it_introduces() {
        assert_eq!(
            kinds("WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"),
            [Some(Verb::Insert)]
        );
        assert_eq!(
            kinds("WITH x AS (SELECT 1) SELECT * FROM x"),
            [Some(Verb::Select)]
        );
        let script = Script::parse("WITH x AS (SELECT 1)");
        assert_eq!(script.statements[0].kind, None);
        assert_eq!(script.issues[0].code, "unknown-statement");
        assert!(
            script.issues[0]
                .message
                .starts_with("WITH at line 1 introduces no")
        );
        assert!(script.opens_recognized());
    }

    #[test]
    fn every_departure_is_placed_by_statement() {
        let script = Script::parse("SELECT 1;\nEXPLAIN x;\nSELECT (1;\n) ;\n123;\nSELECT 'x");
        let placed: Vec<(&str, &str)> = script
            .issues
            .iter()
            .map(|i| (i.code.as_ref(), i.path.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(
            placed,
            [
                ("unknown-statement", "statement 2"),
                ("unbalanced-parenthesis", "statement 3"),
                ("unbalanced-parenthesis", "statement 4"),
                ("unknown-statement", "statement 4"),
                ("unknown-statement", "statement 5"),
                ("unterminated-string", "statement 6"),
            ]
        );
        assert_eq!(script.issues[1].message, "the `(` at line 3 never closes");
        assert_eq!(script.issues[2].message, "a `)` at line 4 closes nothing");

        let after = Script::parse("SELECT 1;\n/* open");
        assert_eq!(after.issues[0].path.as_deref(), Some("statement 2"));
        assert_eq!(after.statements.len(), 1);
    }

    #[test]
    fn classes_partition_the_kinds_they_name() {
        assert_eq!(Class::of_name("READ-ONLY"), Some(Class::ReadOnly));
        assert_eq!(Verb::of_name("Merge"), Some(Verb::Merge));
        assert_eq!(Class::of_verb(Verb::Merge), Some(Class::Dml));
        assert_eq!(Class::of_verb(Verb::Call), None);
        assert_eq!(Class::of_verb(Verb::Set), None);
        let classed: usize = Class::ALL.iter().map(|c| c.members().len()).sum();
        assert_eq!(classed, Verb::ALL.len() - 2);
        for kind in Verb::ALL {
            assert_eq!(Verb::of_name(kind.name()), Some(kind));
        }
    }
}
