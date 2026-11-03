//! Compares scripted runs against the first one named (KHC-024).
//!
//! `run_compare [--window TICKS] FIRST.jsonl OTHER.jsonl...`
//! `run_compare --hash FIRST.csv OTHER.csv...`
//!
//! Exits 1 when a run really differs.

use std::process::ExitCode;
use validation::run_compare::{compare, compare_hashes, parse_hashes, parse_run};

fn read(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn run() -> Result<bool, String> {
    let mut window = 0;
    let mut hash = false;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--window" => {
                window = args
                    .next()
                    .ok_or("--window needs a number of ticks")?
                    .parse()
                    .map_err(|e| format!("--window: {e}"))?;
            }
            "--hash" => hash = true,
            _ => files.push(arg),
        }
    }
    let [first, others @ ..] = files.as_slice() else {
        return Err("usage: run_compare [--window TICKS] [--hash] FIRST OTHER...".into());
    };
    if others.is_empty() {
        return Err("name at least two runs".into());
    }
    let mut same = true;
    if hash {
        let a = parse_hashes(&read(first)?).map_err(|e| format!("{first}: {e}"))?;
        for other in others {
            let b = parse_hashes(&read(other)?).map_err(|e| format!("{other}: {e}"))?;
            let report = compare_hashes(&a, &b);
            println!("{other}: {}", report.text());
            same &= report.first.is_none();
        }
    } else {
        let a = parse_run(&read(first)?);
        for other in others {
            let report = compare(&a, &parse_run(&read(other)?), window);
            println!("{other}: {}", report.text());
            same &= report.passes();
        }
    }
    Ok(same)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("run_compare: {e}");
            ExitCode::from(2)
        }
    }
}
