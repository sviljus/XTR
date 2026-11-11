//! Optional XSD validation of raw-XML lane messages (sidecar `validate: true`).
//!
//! Needs the cargo feature `xsd-validate`. It links the system libxml2.
//! Without the feature, `validate: true` is refused at boot.
//!
//! Rules:
//! * Call `validate` only after `dom::parse` accepted the message. `dom::parse`
//!   refuses a DOCTYPE, so libxml2 never sees entities or a DTD.
//! * libxml2 parses with `NONET`. It does not substitute entities.
//! * The boot check refuses any schema file that names a remote `schemaLocation`.
//! * Validation is CPU work. Call it from `spawn_blocking`.

use std::path::{Path, PathBuf};

/// One validation error, shaped for the JSON answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XsdError {
    pub line: Option<i32>,
    pub message: String,
}

/// How many errors an answer carries.
pub const MAX_ERRORS: usize = 10;

#[derive(Debug)]
pub struct XsdValidator {
    #[cfg_attr(not(feature = "xsd-validate"), allow(dead_code))]
    entries: Vec<PathBuf>,
    #[cfg_attr(not(feature = "xsd-validate"), allow(dead_code))]
    id: u64,
}

#[cfg_attr(not(feature = "xsd-validate"), allow(dead_code))]
/// Refuse a schema directory that points to a remote `schemaLocation`.
fn refuse_remote_locations(dir: &Path) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    for f in rd.flatten() {
        let p = f.path();
        if p.extension().and_then(|e| e.to_str()) != Some("xsd") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let mut rest = text.as_str();
        while let Some(i) = rest.find("schemaLocation") {
            rest = &rest[i + "schemaLocation".len()..];
            let v = rest.trim_start().trim_start_matches('=').trim_start();
            let v = v.trim_start_matches(['"', '\'']);
            if v.contains("://")
                && v.split(['"', '\''])
                    .next()
                    .is_some_and(|u| u.contains("://"))
            {
                return Err(format!(
                    "{}: remote schemaLocation is not allowed with `validate: true`",
                    p.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "xsd-validate")]
mod imp {
    use super::*;
    use libxml::parser::{Parser, ParserOptions};
    use libxml::schemas::{SchemaParserContext, SchemaValidationContext};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    thread_local! {
        // A validation context is not `Send`. Each worker thread builds its own.
        static CONTEXTS: RefCell<HashMap<u64, Vec<SchemaValidationContext>>> =
            RefCell::new(HashMap::new());
    }

    fn load(entries: &[PathBuf]) -> Result<Vec<SchemaValidationContext>, String> {
        entries
            .iter()
            .map(|p| {
                let path = p.to_str().ok_or("schema path is not UTF-8")?;
                let mut pc = SchemaParserContext::from_file(path);
                SchemaValidationContext::from_parser(&mut pc).map_err(|errs| {
                    format!(
                        "{}: libxml2 cannot load the schema: {}",
                        p.display(),
                        errs.iter()
                            .filter_map(|e| e.message.as_deref())
                            .map(str::trim)
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                })
            })
            .collect()
    }

    pub fn new(dir: &Path, schemas: &[String]) -> Result<XsdValidator, String> {
        refuse_remote_locations(dir)?;
        let entries: Vec<PathBuf> = schemas.iter().map(|s| dir.join(s)).collect();
        load(&entries)?; // fail at boot, not on the first message
        Ok(XsdValidator {
            entries,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    pub fn validate(v: &XsdValidator, xml: &str) -> Result<(), Vec<XsdError>> {
        let internal = |m: &str| {
            vec![XsdError {
                line: None,
                message: m.to_string(),
            }]
        };
        let opts = ParserOptions {
            no_net: true,
            no_def_dtd: true,
            ..ParserOptions::default()
        };
        let doc = Parser::default()
            .parse_string_with_options(xml, opts)
            .map_err(|e| internal(&format!("libxml2 parse: {e}")))?;
        CONTEXTS.with(|c| {
            let mut map = c.borrow_mut();
            let ctxs = match map.entry(v.id) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(load(&v.entries).map_err(|e| internal(&e))?)
                }
            };
            let mut last = Vec::new();
            // The root element is declared in one entry XSD. Skip an entry that
            // does not declare it; report the first real verdict.
            for ctx in ctxs.iter_mut() {
                match ctx.validate_document(&doc) {
                    Ok(()) => return Ok(()),
                    Err(errs) => {
                        let undeclared = errs.iter().any(|e| {
                            e.message
                                .as_deref()
                                .is_some_and(|m| m.contains("No matching global declaration"))
                        });
                        let mapped: Vec<XsdError> = errs
                            .into_iter()
                            .take(MAX_ERRORS)
                            .map(|e| XsdError {
                                line: e.line,
                                message: e.message.unwrap_or_default().trim().to_string(),
                            })
                            .collect();
                        if !undeclared {
                            return Err(mapped);
                        }
                        last = mapped;
                    }
                }
            }
            Err(last)
        })
    }
}

#[cfg(not(feature = "xsd-validate"))]
mod imp {
    use super::*;
    pub fn new(_dir: &Path, _schemas: &[String]) -> Result<XsdValidator, String> {
        Err("`validate: true` needs a build with the cargo feature `xsd-validate`".into())
    }
    pub fn validate(_v: &XsdValidator, _xml: &str) -> Result<(), Vec<XsdError>> {
        Ok(())
    }
}

impl XsdValidator {
    pub fn new(dir: &Path, schemas: &[String]) -> Result<Self, String> {
        imp::new(dir, schemas)
    }

    /// Blocking. Call from `spawn_blocking`.
    pub fn validate(&self, xml: &str) -> Result<(), Vec<XsdError>> {
        imp::validate(self, xml)
    }
}
