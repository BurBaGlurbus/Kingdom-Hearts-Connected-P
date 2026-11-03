//! The short commit this build came from, for the version two players compare
//! (KHC-029). Built outside a Git checkout it reads "nogit".

use std::path::Path;
use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "nogit".to_owned());
    println!("cargo:rustc-env=KHC_COMMIT={commit}");
    // A commit moves this file. Named only when it is there: a missing file
    // would run this script on every build.
    let log = Path::new("../../.git/logs/HEAD");
    if log.is_file() {
        println!("cargo:rerun-if-changed={}", log.display());
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }
}
