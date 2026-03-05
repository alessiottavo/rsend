mod commands;
mod crypto;
mod pairing;
mod transfer;
mod transport;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("send") => commands::send::run(&args[2..]).await,
        Some("get") => commands::get::run(&args[2..]).await,
        _ => print_usage(),
    }
}

fn print_usage() {
    eprintln!(
        "rsend — the security conscious, CLI file-sharing tool.

USAGE:
    rsend <COMMAND>

COMMANDS:
    send <path>             send a file or directory
    get  [directory]        receive a file or directory

FLOW:
    SENDER                          RECEIVER
    ──────────────────────────────────────────
    rsend send <path>               rsend get [directory]
      → generates alias + code        → prompts for pairing code
      → displays pairing code         → enter code from sender
      → waits for receiver            → aliases displayed on both sides
      → aliases displayed             → verify aliases match (voice/call)
      → waits for consent             → displays incoming file tree
      → streams file                  → accept? [y/n]
      → done!                         → done!
    "
    );
}
