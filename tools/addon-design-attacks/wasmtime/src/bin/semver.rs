//! Supported claims: how wasmtime 50.0.1 matches versioned export and import names.
//!
//! Exports: a component exports `deployarr:addon/resolver@<built>`, and the host looks up
//! `@<asked>` with `Component::get_export_index`, then calls the function inside.
//! Imports: a component imports `deployarr:sql/sql@<imported>` with one function, and the
//! host's `Linker` defines `@<served>` with that function and one more.

use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Result, Store};

fn export_case(engine: &Engine, built: &str, asked: &str) -> Result<()> {
    let wat = format!(
        r#"(component
            (core module $m (func (export "f") (result i32) (i32.const 7)))
            (core instance $i (instantiate $m))
            (func $f (result u32) (canon lift (core func $i "f")))
            (instance $r (export "resolve" (func $f)))
            (export "deployarr:addon/resolver@{built}" (instance $r)))"#
    );
    let component = Component::new(engine, wat)?;
    let name = format!("deployarr:addon/resolver@{asked}");
    let found = component.get_export_index(None, &name);
    let called = match &found {
        Some(iface) => {
            let linker: Linker<()> = Linker::new(engine);
            let mut store = Store::new(engine, ());
            let instance = linker.instantiate(&mut store, &component)?;
            let func = instance.get_export_index(&mut store, Some(iface), "resolve").expect("resolve is exported");
            let f = instance.get_typed_func::<(), (u32,)>(&mut store, &func)?;
            format!(", resolve() = {}", f.call(&mut store, ())?.0)
        }
        None => String::new(),
    };
    println!("export built @{built}, host looks up @{asked}: found={}{called}", found.is_some());
    Ok(())
}

fn import_case(engine: &Engine, imported: &str, served: &str) -> Result<()> {
    let wat = format!(
        r#"(component
            (import "deployarr:sql/sql@{imported}" (instance $sql (export "query" (func (result u32)))))
            (core func $q (canon lower (func $sql "query")))
            (core module $m
              (import "sql" "query" (func $q (result i32)))
              (func (export "run") (result i32) (call $q)))
            (core instance $qi (export "query" (func $q)))
            (core instance $i (instantiate $m (with "sql" (instance $qi))))
            (func (export "run") (result u32) (canon lift (core func $i "run"))))"#
    );
    let component = Component::new(engine, wat)?;
    let mut linker: Linker<()> = Linker::new(engine);
    {
        let mut sql = linker.instance(&format!("deployarr:sql/sql@{served}"))?;
        sql.func_wrap("query", |_store, (): ()| Ok((41u32,)))?;
        sql.func_wrap("query-extra", |_store, (): ()| Ok((0u32,)))?;
    }
    let mut store = Store::new(engine, ());
    let outcome = match linker.instantiate(&mut store, &component) {
        Ok(instance) => {
            let run = instance.get_typed_func::<(), (u32,)>(&mut store, "run")?;
            format!("links, run() = {}", run.call(&mut store, ())?.0)
        }
        Err(e) => format!("refused: {e}"),
    };
    println!("import of @{imported}, host serves @{served} with one extra function: {outcome}");
    Ok(())
}

fn main() -> Result<()> {
    let engine = Engine::default();
    export_case(&engine, "1.0.0", "1.1.0")?;
    export_case(&engine, "1.2.0", "1.0.0")?;
    export_case(&engine, "1.0.0", "2.0.0")?;
    import_case(&engine, "1.0.0", "1.3.0")?;
    import_case(&engine, "1.3.0", "1.0.0")?;
    import_case(&engine, "2.0.0", "1.3.0")?;
    Ok(())
}
