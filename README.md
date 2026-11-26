# xmip-core-contract-sql

The SQL content contract, a technology of
[xmip-core-contract](https://github.com/IlleNilsson/xmip-core-contract).

Two claims. **Well-formedness is a given**: the Stream is UTF-8 text that
tokenizes and splits cleanly — balanced quotes, comments and parentheses,
statements separated by `;`, each beginning as SELECT, INSERT, UPDATE, DELETE,
MERGE, DDL, DCL or TCL. **Conformance is a given once the contract is named**:
a Location that names the statement kinds it allows — `select,insert` or
`read-only` — holds every script to them, and each statement outside the set
is named by its kind and position.

The claims are this crate's when it is called, and the System Tests call it.
A node holding a Location's Streams to it is
[decided, not built](../../../../../doc/architecture/estate-map.md#arrival-validation): a node refuses to start a
Location that names a contract until it does.

The text is ANSI SQL (ISO/IEC 9075) statement text, vendor-neutral: nothing
here knows a dialect, and nothing executes.

The text is read through `xmip-core-library-codec`'s character reader: any
Unicode whitespace separates tokens, a word may hold any letter, and a
token's column counts characters. A quoted string or identifier, its
doubled quote undone, is read by `xmip-core-library-sql`'s `Delimiter`, and
a statement's kind is the `Verb` it opens with there, the one SQL in the
estate (ADR-0044, amendment 2026-10-12).

## The declaration

What a Location naming this contract is configured with is two `const`s in
its `declaration` module, `src/declaration.rs`, beside the code, which its
`ContractFactory` reads (ADR-0064, amendment 2026-11-26):
`declaration::SETTINGS`, `reference`, the statement kinds allowed, and
`NAME`, `sql`, the name a Validate step's `format` names it by. Built with
its `declaration` feature alone (`--no-default-features --features
declaration`), the crate is those and nothing more: its code and every crate
the code is built on are behind the default feature. So the library the
surfaces load can carry it without the code, for the language server and the
desktop to suggest and check a Location's contract settings though no node
in their process loaded the technology, once the gathering crate of open
problem 48 links it. `test/Declaration.Test.ps1` in the estate holds that
the declaration builds nothing the contract capability does not.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
