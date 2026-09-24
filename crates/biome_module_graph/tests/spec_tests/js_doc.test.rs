use super::*;
use biome_module_graph::{SymbolFromModuleInfo, find_jsdoc_for_exported_symbol};

#[test]
fn finds_jsdoc_for_separately_exported_ambient_declarations() {
    let fs = MemoryFileSystem::default();
    fs.insert(
        "/src/index.ts".into(),
        r#"
            /** @deprecated Use Grid2. */
            declare const Grid: unknown;
            export { Grid };

            /** @deprecated Use generateText. */
            declare function generateObject(): void;
            export { generateObject };
        "#,
    );

    let db = build_js_test_module_db(&fs, &["/src/index.ts"], false);
    let module = db
        .module_for_path(Utf8Path::new("/src/index.ts"))
        .expect("module must exist");

    for (name, expected) in [
        ("Grid", "@deprecated Use Grid2."),
        ("generateObject", "@deprecated Use generateText."),
    ] {
        let symbol = SymbolFromModuleInfo::new(&db, name, module);
        let jsdoc = find_jsdoc_for_exported_symbol(&db, symbol)
            .as_ref()
            .and_then(|jsdoc| jsdoc.declaration.as_ref())
            .unwrap_or_else(|| panic!("{name} must have JSDoc"));
        assert_eq!(jsdoc.as_ref(), expected);
    }
}

#[test]
fn finds_overload_jsdoc_without_losing_undocumented_signatures() {
    let fs = MemoryFileSystem::default();
    fs.insert(
        "/src/index.ts".into(),
        r#"
            declare function mixed(value: string): string;
            /** @deprecated Pass a string. */
            declare function mixed(value: number): string;
            export { mixed as renamed };

            /** @deprecated Use another function. */
            export function implemented(value: string): string;
            /** @deprecated Use another function. */
            export function implemented(value: number): string;
            /** @private */
            export function implemented(value: string | number): string {
                return String(value);
            }

            /** @deprecated Use another function. */
            export function single(): void {}

            export default function mixedDefault(value: string): string;
            /** @deprecated Pass a string. */
            export default function mixedDefault(value: number): string;
            /** @deprecated Use another function. */
            export default function mixedDefault(value: string | number): string {
                return String(value);
            }
        "#,
    );
    let db = build_js_test_module_db(&fs, &["/src/index.ts"], false);
    let module = db
        .module_for_path(Utf8Path::new("/src/index.ts"))
        .expect("module must exist");

    for (name, declaration, overloads) in [
        (
            "renamed",
            "@deprecated Pass a string.",
            vec![None, Some("@deprecated Pass a string.")],
        ),
        (
            "implemented",
            "@private",
            vec![
                Some("@deprecated Use another function."),
                Some("@deprecated Use another function."),
            ],
        ),
        ("single", "@deprecated Use another function.", vec![]),
        (
            "default",
            "@deprecated Use another function.",
            vec![None, Some("@deprecated Pass a string.")],
        ),
    ] {
        let symbol = SymbolFromModuleInfo::new(&db, name, module);
        let jsdoc = find_jsdoc_for_exported_symbol(&db, symbol)
            .as_ref()
            .unwrap_or_else(|| panic!("{name} must resolve"));
        assert_eq!(jsdoc.declaration.as_deref(), Some(declaration));
        assert_eq!(
            jsdoc
                .overloads
                .iter()
                .map(Option::as_deref)
                .collect::<Vec<_>>(),
            overloads,
        );
    }
}

#[test]
fn overload_jsdoc_tracks_reexport_dependencies() {
    let fs = MemoryFileSystem::default();
    fs.insert(
        "/src/library.ts".into(),
        r#"
            export declare function make(value: string): string;
            /** @deprecated Pass a string. */
            export declare function make(value: number): string;
        "#,
    );
    fs.insert(
        "/src/index.ts".into(),
        "export { make as renamed } from './library';",
    );
    fs.insert("/src/unrelated.ts".into(), "export const value = 1;");
    let mut db = build_js_test_module_db(
        &fs,
        &["/src/index.ts", "/src/library.ts", "/src/unrelated.ts"],
        false,
    );
    let module = db
        .module_for_path(Utf8Path::new("/src/index.ts"))
        .expect("module must exist");
    let symbol = SymbolFromModuleInfo::new(&db, "renamed", module);
    let jsdoc = find_jsdoc_for_exported_symbol(&db, symbol)
        .as_ref()
        .expect("reexport must resolve");
    assert_eq!(jsdoc.overloads.len(), 2);
    assert_eq!(jsdoc.overloads.first(), Some(&None));

    db.clear_salsa_events();
    let _ = find_jsdoc_for_exported_symbol(&db, symbol);
    assert!(
        function_query_will_execute_position(
            &db,
            find_jsdoc_for_exported_symbol,
            symbol,
            &db.take_salsa_events(),
        )
        .is_none()
    );

    fs.insert("/src/unrelated.ts".into(), "export const value = 2;");
    let unrelated = db
        .module_for_path(Utf8Path::new("/src/unrelated.ts"))
        .expect("module must exist");
    let kind = resolve_js_module_kind_for_test(&fs, "/src/unrelated.ts", false);
    salsa::Setter::to(unrelated.set_kind(&mut db), kind);
    db.clear_salsa_events();
    let symbol = SymbolFromModuleInfo::new(&db, "renamed", module);
    let _ = find_jsdoc_for_exported_symbol(&db, symbol);
    assert!(
        function_query_will_execute_position(
            &db,
            find_jsdoc_for_exported_symbol,
            symbol,
            &db.take_salsa_events(),
        )
        .is_none()
    );

    fs.insert(
        "/src/library.ts".into(),
        r#"
            /** @deprecated Use another function. */
            export declare function make(value: string): string;
            /** @deprecated Pass a string. */
            export declare function make(value: number): string;
        "#,
    );
    let library = db
        .module_for_path(Utf8Path::new("/src/library.ts"))
        .expect("module must exist");
    let kind = resolve_js_module_kind_for_test(&fs, "/src/library.ts", false);
    salsa::Setter::to(library.set_kind(&mut db), kind);
    db.clear_salsa_events();
    let symbol = SymbolFromModuleInfo::new(&db, "renamed", module);
    let jsdoc = find_jsdoc_for_exported_symbol(&db, symbol)
        .as_ref()
        .expect("reexport must resolve");
    assert_eq!(
        jsdoc.overloads.first().and_then(Option::as_deref),
        Some("@deprecated Use another function."),
    );
    let events = db.take_salsa_events();
    assert!(
        function_query_will_execute_position(&db, find_jsdoc_for_exported_symbol, symbol, &events,)
            .is_some()
    );
    assert_function_query_was_not_run(&db, infer_module_types, library, &events);
}
