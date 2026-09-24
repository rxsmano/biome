//! Salsa-backed queries over module graph data.
//!
//! Tracked queries cache their results and record the inputs they inspect so
//! Salsa can rerun affected queries after a change. Queries with compound input
//! values may use interned input structs, which provide stable identities for
//! equal values but do not cache results by themselves.
//!
//! Public query entry points live in this module. Helpers stay near the query
//! whose behavior they implement.

mod css;
mod js_scc;
mod type_inference;

use crate::{JsExport, JsExportedSymbolLookup, JsOwnExport, ModuleDb, ModuleInfo, ModuleInfoKind};
use biome_js_semantic::{JsDeclarationKind, SemanticModel};
use biome_js_syntax::binding_ext::AnyJsBindingDeclaration;
use biome_js_type_info::ImportSymbol;
use biome_jsdoc_comment::JsdocComment;
use biome_rowan::TextRange;

pub use crate::db::type_inference::InferredModuleTypes;
pub use css::*;
pub use js_scc::*;
pub use type_inference::*;

// #region EXPORTED TRACKED QUERIES

/// Finds the exported symbol with the given name, following re-exports.
#[salsa::tracked]
pub fn find_js_exported_symbol<'db>(
    db: &'db dyn ModuleDb,
    symbol: SymbolFromModuleInfo<'db>,
) -> JsExportedSymbolLookup {
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut stack = vec![symbol];
    let mut saw_unresolved_target = false;

    while let Some(symbol) = stack.pop() {
        let ModuleInfoKind::Js(module) = symbol.module(db).kind(db) else {
            continue;
        };
        match &module.exports.get(symbol.name(db).as_str()) {
            Some(JsExport::Own(own_export) | JsExport::OwnType(own_export)) => {
                return JsExportedSymbolLookup::Found(own_export.clone());
            }
            Some(JsExport::Reexport(reexport) | JsExport::ReexportType(reexport)) => {
                match &reexport.import.symbol {
                    ImportSymbol::All => break,
                    ImportSymbol::Named(source_name) => {
                        let lookup = source_name.text().to_string();
                        match reexport.import.resolved_path.as_deref() {
                            Ok(path) if seen_paths.insert(path.to_path_buf()) => {
                                if let Some(module) = db.module_for_path(path) {
                                    stack.push(SymbolFromModuleInfo::new(
                                        db,
                                        lookup.clone(),
                                        module,
                                    ));
                                }
                            }
                            Ok(_) => break,
                            Err(_) => {
                                saw_unresolved_target = true;
                                break;
                            }
                        }
                    }
                    ImportSymbol::Default => {
                        if let Ok(path) = reexport.import.resolved_path.as_deref()
                            && seen_paths.insert(path.to_path_buf())
                            && let Some(module) = db.module_for_path(path)
                        {
                            stack.push(SymbolFromModuleInfo::new(db, symbol.name(db), module));
                        }
                    }
                }
            }
            None => {
                for reexport in module.blanket_reexports.iter() {
                    match reexport.import.resolved_path.as_deref() {
                        Ok(path) => {
                            if seen_paths.insert(path.to_path_buf())
                                && let Some(module) = db.module_for_path(path)
                            {
                                stack.push(SymbolFromModuleInfo::new(db, symbol.name(db), module));
                            }
                        }
                        Err(_) => saw_unresolved_target = true,
                    }
                }
            }
        }
    }

    if saw_unresolved_target {
        JsExportedSymbolLookup::Unknown
    } else {
        JsExportedSymbolLookup::Missing
    }
}

/// Finds JSDoc for an exported symbol and its public overload signatures,
/// following re-exports through the db.
#[salsa::tracked(returns(ref))]
pub fn find_jsdoc_for_exported_symbol<'db>(
    db: &'db dyn ModuleDb,
    symbol: SymbolFromModuleInfo<'db>,
) -> Option<JsExportedSymbolJsdoc> {
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut stack = vec![symbol];

    while let Some(symbol) = stack.pop() {
        let ModuleInfoKind::Js(module) = symbol.module(db).kind(db) else {
            continue;
        };
        match &module.exports.get(symbol.name(db).as_str()) {
            Some(JsExport::Own(own_export) | JsExport::OwnType(own_export)) => {
                return match own_export {
                    JsOwnExport::Binding(binding_range) => {
                        jsdoc_for_binding(&module.semantic_model, *binding_range)
                    }
                    JsOwnExport::Type(_) => None,
                    JsOwnExport::Namespace(reexport) => reexport
                        .export_range
                        .and_then(|range| module.semantic_model.export_jsdoc(range).cloned())
                        .map(|declaration| JsExportedSymbolJsdoc {
                            declaration: Some(declaration),
                            overloads: Box::default(),
                        }),
                };
            }
            Some(JsExport::Reexport(reexport) | JsExport::ReexportType(reexport)) => {
                match &reexport.import.symbol {
                    ImportSymbol::All => break,
                    ImportSymbol::Named(source_name) => {
                        let lookup = source_name.text().to_string();
                        match reexport.import.resolved_path.as_deref() {
                            Ok(path) if seen_paths.insert(path.to_path_buf()) => {
                                if let Some(module) = db.module_for_path(path) {
                                    stack.push(SymbolFromModuleInfo::new(
                                        db,
                                        lookup.clone(),
                                        module,
                                    ));
                                }
                            }
                            _ => break,
                        }
                    }
                    ImportSymbol::Default => {
                        if let Ok(path) = reexport.import.resolved_path.as_deref()
                            && let Some(module) = db.module_for_path(path)
                        {
                            stack.push(SymbolFromModuleInfo::new(db, symbol.name(db), module));
                        }
                    }
                }
            }
            None => {
                for reexport in module.blanket_reexports.iter() {
                    if let Ok(path) = reexport.import.resolved_path.as_deref()
                        && seen_paths.insert(path.to_path_buf())
                        && let Some(module) = db.module_for_path(path)
                    {
                        stack.push(SymbolFromModuleInfo::new(db, symbol.name(db), module));
                    }
                }
            }
        }
    }

    None
}

// #endregion

// #region QUERY HELPER FUNCTIONS

fn jsdoc_for_binding(model: &SemanticModel, range: TextRange) -> Option<JsExportedSymbolJsdoc> {
    let binding = model.as_binding_by_range(range)?;
    let mut result = JsExportedSymbolJsdoc {
        declaration: binding.jsdoc().cloned(),
        overloads: Box::default(),
    };
    if binding.declaration_kind() != JsDeclarationKind::Function {
        return Some(result);
    }

    let scope = model
        .scope_hoisted_to(&binding.syntax())
        .unwrap_or_else(|| binding.scope());
    let overloads = scope.overload_sets().into_iter().find(|set| {
        set.last()
            .and_then(|id| model.binding_by_id(*id))
            .is_some_and(|last| last == binding)
    });
    let mut comments = Vec::new();
    for id in overloads.into_iter().flatten() {
        let overload = model.binding_by_id(id)?;
        if matches!(
            overload.tree().declaration(),
            Some(
                AnyJsBindingDeclaration::TsDeclareFunctionDeclaration(_)
                    | AnyJsBindingDeclaration::TsDeclareFunctionExportDefaultDeclaration(_)
            )
        ) {
            comments.push(overload.jsdoc().cloned());
        }
    }
    result.overloads = comments.into_boxed_slice();
    Some(result)
}

// #endregion

/// Documentation of an exported declaration and its overload signatures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsExportedSymbolJsdoc {
    /// JSDoc of the declaration selected by name resolution.
    pub declaration: Option<JsdocComment>,

    /// Public overload signatures in source order, excluding the implementation.
    /// Empty for symbols without overloads; undocumented signatures remain `None`.
    pub overloads: Box<[Option<JsdocComment>]>,
}

// #region INTERNED TYPES

#[salsa::interned]
/// Generic symbol used by queries to track a generic "symbol", which can represent everything (variable name, class name, etc.)
pub struct SymbolFromModuleInfo {
    #[returns(clone)]
    pub(crate) name: String,

    #[returns(ref)]
    pub(crate) module: ModuleInfo,
}

// #endregion
