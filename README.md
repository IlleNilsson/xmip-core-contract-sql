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

The text is ANSI SQL (ISO/IEC 9075) statement text, vendor-neutral: nothing
here knows a dialect, and nothing executes.

## Toolchain

`rust-toolchain.toml` pins the toolchain for the whole estate. Do not change it
here.

## Verification

The included workflow is manual-only and calls the versioned shared workflow at
`IlleNilsson/.github@v1`.
