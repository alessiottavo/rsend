use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("init") => return,
        Some("pin") => set_pin(args.get(2).map(|p| p.as_str())),
        _ => print_usage(),
    }
}

fn set_pin(pin_str: Option<&str>) {
    let pin_path = match pin_str {
        Some(s) => Path::new(s),
        None => {
            eprintln!("Please provide a valid path.");
            std::process::exit(1)
        }
    };

    if pin_path.exists() {
        println!("{}", pin_path.display());
    } else {
        eprintln!("Path '{}' does not exist.", pin_path.display());
        std::process::exit(1);
    }
}

fn print_usage() {
    println!(
        "rsend — the security conscious, CLI file-sharing tool.

USAGE:
    rsend <COMMAND>

COMMANDS:
    init              generate alias and pairing code
    pin <directory>   set receive directory for next session
    pair <code>       pair with a sender
    send <path>       send file or directory to paired peer
    
TO SEND:                     TO RECEIVE:
    rsend init             |    rsend pin
    rsend send <path>      |    rsend pair <code>
    
    "
    );
}
