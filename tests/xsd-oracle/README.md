# Xerces oracle for the XSD validator

This directory holds a test oracle. The oracle validates XML messages against
XSD files with Apache Xerces (Java). The Rust test `tests/xsd_oracle.rs`
compares the Rust validator (`src/xsdval/`) with the oracle.

Xerces runs only in tests and in CI. It never runs in the XTR process.

## What the oracle does

* It reads a schema directory and a message directory.
* It prints one JSON document with a verdict for each message.
* The verdict is `valid`, `invalid`, `not_wellformed`, `refused` or `no_schema`.
* Each error has a rule code (`cvc-...`), an element path and a line.
* The output has no message value and no Xerces message text.
* The oracle ignores `xsi:schemaLocation` and `xsi:noNamespaceSchemaLocation`.

## Safety rules

* Java runs in a container with `--network none`, `--read-only` and no capabilities.
* The directories are mounted read-only.
* The image is pinned by digest. The jar is pinned by SHA-256 (`pins.env`).
* A wrong SHA-256 stops the script with exit code 1.
* The schema parser reads only regular files inside the schema directory.
* The resolver refuses `..`, absolute paths, every URI scheme, `%` and `\`,
  and symlinks that leave the directory. A `DOCTYPE` is refused in schemas and messages.

## Run it

1. Install Docker.
2. Run `sh tests/xsd-oracle/fetch-xerces.sh`. It downloads the jar into `.cache/`.
3. Run `cargo test --test xsd_oracle -- --nocapture`.

Without Docker or the jar, the tests print `xsd-oracle skipped` and pass.
Set `XTR_XSD_ORACLE_REQUIRED=1` to make a missing part an error.

## Gate test

The ADR requires zero differences before `enforce` is the default.

1. Set `XTR_ERRU_XSD_DIR` to the directory with the ERRU XSD files.
2. Set `XTR_ORACLE_SAMPLES_DIR` to a directory with valid sample messages.
3. Run `XTR_XSD_ORACLE_REQUIRED=1 cargo test --test xsd_oracle -- --ignored --nocapture`.

The test writes generated messages only to a temporary directory.
Do not commit sample or generated messages.

To use the oracle from another repository, set `XTR_ORACLE_DIR` to the directory
of this oracle and `XTR_ORACLE_CACHE` to the jar directory.

## Files

* `Oracle.java`: the oracle.
* `run-oracle.sh`, `fetch-xerces.sh`, `pins.env`: container start and pins.
* `rule-classes.json`: map from Xerces rule codes to Rust rule codes. A code that is not here is an error.
* `expected-differences`: the closed list of allowed differences.
  A difference is allowed only when XTR refuses and Xerces accepts.
* `selftest/`: synthetic schema and messages.
