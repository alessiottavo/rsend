pub fn run(args: &[String]) {
    if args.len() > 1 {
        eprintln!("error: unexpected arguments: {}", args[1..].join(", "));
        eprintln!("usage: rsend get [directory]");
        std::process::exit(1);
    }

    let dir = match args.get(0) {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().expect("failed to get current directory"),
    };

    if let Err(e) = validate(&dir) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }

    println!("receiving to: {}", dir.display());
    // proceed
}

fn validate(dir: &std::path::PathBuf) -> Result<(), String> {
    if !dir.exists() {
        return Err(format!("'{}' does not exist", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("'{}' is not a directory", dir.display()));
    }

    let test_file = dir.join(".rsend_write_test");
    match std::fs::File::create(&test_file) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
        }
        Err(_) => return Err(format!("'{}' is not writable", dir.display())),
    }

    Ok(())
}
