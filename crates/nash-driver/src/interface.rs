//! In-memory summaries of public exports and module contracts.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Module interface for incremental compilation.
///
/// Contains the public exports of a module and a fingerprint
/// for change detection.
#[derive(Debug, Clone)]
pub struct Interface {
    /// Module name (e.g., "Json.Decode").
    pub module_name: String,

    /// Public exports from this module.
    pub exports: Vec<Export>,

    /// Hash of the interface content for change detection.
    pub fingerprint: u64,
}

/// An exported item from a module.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Export {
    /// A value export (function or constant).
    Value {
        name: String,
        // Type signature would go here in a full implementation
    },

    /// A type export (type alias or custom type).
    Type {
        name: String,
        /// Whether constructors are exposed.
        constructors_exposed: bool,
        /// Closed Haskell 98 constructor kind.
        kind: String,
    },
}

impl Interface {
    /// Create a new interface from exports.
    pub fn new(module_name: String, exports: Vec<Export>) -> Self {
        let fingerprint = compute_fingerprint(&exports);
        Interface {
            module_name,
            exports,
            fingerprint,
        }
    }

    /// Capture exported canonical types and their inferred kind contracts.
    pub fn from_canonical(interface: &nash_can::Interface<'_>) -> Self {
        let mut exports: Vec<Export> = interface
            .values
            .iter()
            .map(|value| Export::Value {
                name: value.name.into(),
            })
            .collect();
        exports.extend(
            interface
                .unions
                .iter()
                .filter_map(|union| union.to_public())
                .map(|union| Export::Type {
                    name: union.name.into(),
                    constructors_exposed: union.visibility == nash_can::UnionVisibility::Open,
                    kind: render_kind(union.kind),
                }),
        );
        exports.extend(
            interface
                .aliases
                .iter()
                .filter_map(|alias| alias.to_public())
                .map(|alias| Export::Type {
                    name: alias.name.into(),
                    constructors_exposed: false,
                    kind: render_kind(alias.kind),
                }),
        );
        exports.sort_by(|a, b| export_name(a).cmp(export_name(b)));
        let mut result = Self::new(interface.home.name.into(), exports);
        // Changes to hidden contexts, private constructor metadata, trait
        // signatures, aliases, or global impls also invalidate dependents.
        // This conservative fingerprint can change with source regions; it
        // never omits a contract because it is absent from the public exports.
        let mut hasher = DefaultHasher::new();
        result.exports.hash(&mut hasher);
        format!("{interface:?}").hash(&mut hasher);
        result.fingerprint = hasher.finish();
        result
    }

    /// Check if this interface differs from another.
    pub fn differs_from(&self, other: &Interface) -> bool {
        self.fingerprint != other.fingerprint
    }
}

/// Compute a fingerprint hash for a set of exports.
fn compute_fingerprint(exports: &[Export]) -> u64 {
    let mut hasher = DefaultHasher::new();
    exports.hash(&mut hasher);
    hasher.finish()
}

fn export_name(export: &Export) -> &str {
    match export {
        Export::Value { name } | Export::Type { name, .. } => name,
    }
}

fn render_kind(kind: &nash_ast::Kind<'_>) -> String {
    fn render(kind: &nash_ast::Kind<'_>, argument: bool) -> String {
        match kind {
            nash_ast::Kind::Type => "Type".into(),
            nash_ast::Kind::Arrow(from, to) => {
                let text = format!("{} -> {}", render(from, true), render(to, false));
                if argument { format!("({text})") } else { text }
            }
        }
    }
    render(kind, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interface_fingerprint() {
        let exports1 = vec![Export::Value {
            name: "foo".to_string(),
        }];
        let exports2 = vec![Export::Value {
            name: "bar".to_string(),
        }];

        let iface1 = Interface::new("Test".to_string(), exports1);
        let iface2 = Interface::new("Test".to_string(), exports2);

        assert!(iface1.differs_from(&iface2));
    }

    #[test]
    fn test_interface_same_fingerprint() {
        let exports1 = vec![Export::Value {
            name: "foo".to_string(),
        }];
        let exports2 = vec![Export::Value {
            name: "foo".to_string(),
        }];

        let iface1 = Interface::new("Test".to_string(), exports1);
        let iface2 = Interface::new("Test".to_string(), exports2);

        assert!(!iface1.differs_from(&iface2));
    }
}
