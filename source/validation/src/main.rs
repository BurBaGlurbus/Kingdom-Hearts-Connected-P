use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::PathBuf,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: validation <selected-Steam-install-root> <new-private-manifest.json>".into(),
        );
    }
    let root = PathBuf::from(&args[0]);
    let output = validation::safe_output(&root, &PathBuf::from(&args[1]))?;
    let manifest = validation::scan(&root)?;
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    writeln!(
        io::stdout(),
        "Scanned {} KH2 candidate files. Qualification: {}. Private manifest written outside validation.",
        manifest.files.len(),
        manifest.qualification
    )?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Source inspection failed: {error}");
        std::process::exit(1);
    }
}
