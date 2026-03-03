use std::path::PathBuf;

pub fn run(args: &[String]) {
    let send_path = validate_args(args);
    // proceed
}

fn validate_args(args: &[String]) -> PathBuf {
    if args.len() > 1 {
        eprintln!("error: unexpected arguments: {}", args[1..].join(", "));
        eprintln!("usage: rsend send <path>");
        std::process::exit(1);
    }

    return match args.get(0) {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            eprintln!("error: no path provided\nusage: rsend send <path>");
            std::process::exit(1);
        }
    };
}
