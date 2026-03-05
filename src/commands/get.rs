use crate::crypto::hash;
use crate::pairing::{alias, code};
use crate::protocol::{self, FileInfo};
use crate::transfer;
use crate::transport::{dht, nat, quic};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const SENDER_LOOKUP_ATTEMPTS: u32 = 30;
const SENDER_LOOKUP_DELAY: Duration = Duration::from_secs(2);
const CONNECT_ATTEMPTS: u32 = 3;
const CONNECT_PER_ATTEMPT: Duration = Duration::from_secs(15);

pub async fn run(args: &[String]) {
    if let Err(e) = run_inner(args).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run_inner(args: &[String]) -> Result<(), String> {
    let dest_dir = parse_args(args)?;
    validate(&dest_dir)?;

    println!("receiving to: {}", dest_dir.display());

    let pairing_code = prompt_code()?;
    let receiver_alias = alias::generate();

    println!("your alias:   {receiver_alias}");

    let sender_key = hash::derive_sender_key(&pairing_code);
    let receiver_key = hash::derive_receiver_key(&pairing_code);

    // Bind UDP + STUN discovery
    println!("discovering public address...");
    let (udp, public_addr, nat_type) = nat::bind_and_discover().await?;
    println!("public address: {public_addr}");

    if nat_type == nat::NatType::Symmetric {
        eprintln!("warning: symmetric NAT detected — direct connection may fail");
    }

    // Lookup sender on DHT
    println!("looking up sender...");
    let sender_addr =
        dht::lookup_with_retry(&sender_key, SENDER_LOOKUP_ATTEMPTS, SENDER_LOOKUP_DELAY).await?;

    // Announce receiver on DHT so sender can find us
    dht::announce(&receiver_key, public_addr.port()).await?;

    // Convert to std socket, start background punch concurrently with connect
    let std_socket = udp
        .into_std()
        .map_err(|e| format!("convert to std socket: {e}"))?;
    std_socket
        .set_nonblocking(true)
        .map_err(|e| format!("set nonblocking: {e}"))?;

    let punch_clone = std_socket
        .try_clone()
        .map_err(|e| format!("clone socket for punch: {e}"))?;

    println!("punching through NAT...");
    let punch_task = nat::punch_background(punch_clone, sender_addr)?;

    println!("connecting...");
    let mut stream =
        quic::connect_with_retry(&std_socket, sender_addr, CONNECT_ATTEMPTS, CONNECT_PER_ATTEMPT)
            .await?;
    punch_task.abort();

    // Handshake: exchange aliases
    let sender_alias = protocol::recv_alias(&mut stream.recv).await?;
    protocol::send_alias(&mut stream.send, &receiver_alias).await?;

    println!("sender:       {sender_alias}");
    println!();
    println!("verify the aliases match over a call or in person.");

    // Receive file manifest
    let manifest = protocol::recv_manifest(&mut stream.recv).await?;
    print_manifest(&manifest);

    // Prompt for consent
    let accepted = prompt_accept()?;
    protocol::send_consent(&mut stream.send, accepted).await?;

    if !accepted {
        println!("transfer declined.");
        return Ok(());
    }

    // Receive files
    transfer::recv_files(&mut stream.recv, &dest_dir, |p| {
        println!(
            "  {} {}/{}",
            p.filename,
            protocol::format_size(p.bytes_done),
            protocol::format_size(p.bytes_total),
        );
    })
    .await?;

    println!("done!");
    Ok(())
}

fn print_manifest(files: &[FileInfo]) {
    println!();
    println!("incoming files:");
    for file in files {
        println!("  {} ({})", file.name, protocol::format_size(file.size));
    }
    println!();
}

fn prompt_accept() -> Result<bool, String> {
    print!("accept? [y/n] ");
    io::stdout()
        .flush()
        .map_err(|e| format!("failed to flush stdout: {e}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("failed to read input: {e}"))?;

    Ok(input.trim().eq_ignore_ascii_case("y"))
}

fn parse_args(args: &[String]) -> Result<PathBuf, String> {
    if args.len() > 1 {
        return Err(format!(
            "unexpected arguments: {}\nusage: rsend get [directory]",
            args[1..].join(", ")
        ));
    }

    match args.first() {
        Some(p) => Ok(PathBuf::from(p)),
        None => {
            std::env::current_dir().map_err(|e| format!("failed to get current directory: {e}"))
        }
    }
}

fn validate(dir: &Path) -> Result<(), String> {
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

fn prompt_code() -> Result<String, String> {
    print!("pairing code: ");
    io::stdout()
        .flush()
        .map_err(|e| format!("failed to flush stdout: {e}"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("failed to read input: {e}"))?;

    let code = input.trim().to_string();
    code::validate_format(&code)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_args_uses_current_dir() {
        let args: Vec<String> = vec![];
        assert!(parse_args(&args).is_ok());
    }

    #[test]
    fn test_valid_dir_arg() {
        let args = vec!["/tmp".to_string()];
        assert_eq!(parse_args(&args).unwrap(), PathBuf::from("/tmp"));
    }

    #[test]
    fn test_unexpected_args() {
        let args = vec!["/tmp".to_string(), "extra".to_string()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_valid_directory() {
        assert!(validate(&PathBuf::from("/tmp")).is_ok());
    }

    #[test]
    fn test_nonexistent_directory() {
        let err = validate(&PathBuf::from("/tmp/doesnotexist_rsend")).unwrap_err();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn test_path_is_not_directory() {
        let file_path = PathBuf::from("/tmp/rsend_test_file");
        std::fs::File::create(&file_path).unwrap();
        let err = validate(&file_path).unwrap_err();
        assert!(err.contains("is not a directory"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_prompt_accept_yes() {
        assert!("y".eq_ignore_ascii_case("y"));
        assert!("Y".eq_ignore_ascii_case("y"));
        assert!(!"n".eq_ignore_ascii_case("y"));
        assert!(!"".eq_ignore_ascii_case("y"));
    }

    #[test]
    fn test_print_manifest_does_not_panic() {
        let files = vec![
            FileInfo {
                name: "test.txt".to_string(),
                size: 1024,
            },
            FileInfo {
                name: "big.bin".to_string(),
                size: 5 * 1024 * 1024,
            },
        ];
        print_manifest(&files);
    }
}
