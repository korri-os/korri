//! Run a plugin file and print its declaration.
//!
//! Run this probe on Linux to inspect runtime transpilation and evaluation.

use std::time::Instant;

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: script_probe <plugin.ts>");
            std::process::exit(2);
        }
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            std::process::exit(2);
        }
    };

    let started = Instant::now();
    let transpiled = match korrid::script::transpile_ts(&source) {
        Ok(javascript) => javascript,
        Err(error) => {
            eprintln!("TRANSPILE FAILED: {error}");
            std::process::exit(1);
        }
    };
    let transpile_ms = started.elapsed().as_secs_f64() * 1000.0;

    let evaluated = Instant::now();
    match korrid::script::eval_plugin(&transpiled) {
        Ok(json) => {
            let eval_ms = evaluated.elapsed().as_secs_f64() * 1000.0;
            println!("declaration: {json}");
            println!("transpile: {transpile_ms:.2}ms");
            println!("evaluate:  {eval_ms:.2}ms");
        }
        Err(error) => {
            eprintln!("EVAL FAILED: {error}");
            std::process::exit(1);
        }
    }
}
