use crate::crypto::hash;
use crate::pairing::{alias, code::PairingCode};
use crate::protocol::{self, FileInfo};
use crate::transfer;
use crate::transport::{dht, nat, quic::QuicListener};
use std::path::PathBuf;
use std::time::Duration;

const RECEIVER_LOOKUP_ATTEMPTS: u32 = 60;
const RECEIVER_LOOKUP_DELAY: Duration = Duration::from_secs(2);
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(60);

pub async fn run(args: &[String]) {
    if let Err(e) = run_inner(args).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run_inner(args: &[String]) -> Result<(), String> {
    let send_path = validate_args(args)?;

    if !send_path.exists() {
        return Err(format!("'{}' does not exist", send_path.display()));
    }

    let sender_alias = alias::generate();
    let pairing_code = PairingCode::generate()?;

    println!("your alias:   {sender_alias}");
    println!("pairing code: {}", pairing_code.value);

    let sender_key = hash::derive_sender_key(&pairing_code.value);
    let receiver_key = hash::derive_receiver_key(&pairing_code.value);

    // Bind UDP + STUN discovery
    println!("discovering public address...");
    let (udp, public_addr, nat_type) = nat::bind_and_discover().await?;
    println!("public address: {public_addr}");

    if nat_type == nat::NatType::Symmetric {
        eprintln!("warning: symmetric NAT detected — direct connection may fail");
    }

    // Announce sender on DHT with STUN-discovered port
    dht::announce(&sender_key, public_addr.port()).await?;

    // Wait for receiver to announce
    println!("waiting to pair...");
    let receiver_addr =
        dht::lookup_with_retry(&receiver_key, RECEIVER_LOOKUP_ATTEMPTS, RECEIVER_LOOKUP_DELAY)
            .await?;

    // Convert to std socket, create listener FIRST, then start background punch
    let std_socket = udp
        .into_std()
        .map_err(|e| format!("convert to std socket: {e}"))?;
    std_socket
        .set_nonblocking(true)
        .map_err(|e| format!("set nonblocking: {e}"))?;

    let punch_clone = std_socket
        .try_clone()
        .map_err(|e| format!("clone socket for punch: {e}"))?;

    let listener = QuicListener::from_socket(std_socket)?;

    println!("punching through NAT...");
    let punch_task = nat::punch_background(punch_clone, receiver_addr)?;

    let mut stream = tokio::time::timeout(ACCEPT_TIMEOUT, listener.accept())
        .await
        .map_err(|_| format!("accept timed out after {ACCEPT_TIMEOUT:?}"))?
        .map_err(|e| format!("{e}"))?;
    punch_task.abort();

    // Handshake: exchange aliases
    protocol::send_alias(&mut stream.send, &sender_alias).await?;
    let receiver_alias = protocol::recv_alias(&mut stream.recv).await?;
    println!("receiver:     {receiver_alias}");

    // Collect files and send manifest
    let (base, files) = transfer::collect_files(&send_path)?;
    let manifest: Vec<FileInfo> = files
        .iter()
        .map(|rel| {
            let abs = base.join(rel);
            let name = rel.to_string_lossy().to_string();
            let size = abs
                .metadata()
                .map_err(|e| format!("stat {name}: {e}"))?
                .len();
            Ok(FileInfo { name, size })
        })
        .collect::<Result<Vec<_>, String>>()?;

    protocol::send_manifest(&mut stream.send, &manifest).await?;

    // Wait for consent
    println!("waiting for consent...");
    let accepted = protocol::recv_consent(&mut stream.recv).await?;
    if !accepted {
        println!("receiver declined the transfer.");
        return Ok(());
    }

    // Transfer files
    transfer::send_files(&mut stream.send, &base, &files, |p| {
        println!(
            "  {} {}/{}",
            p.filename,
            protocol::format_size(p.bytes_done),
            protocol::format_size(p.bytes_total),
        );
    })
    .await?;

    stream
        .send
        .finish()
        .map_err(|e| format!("finish stream: {e}"))?;

    println!("done!");
    Ok(())
}

fn validate_args(args: &[String]) -> Result<PathBuf, String> {
    if args.len() > 1 {
        return Err(format!(
            "unexpected arguments: {}\nusage: rsend send <path>",
            args[1..].join(", ")
        ));
    }

    match args.first() {
        Some(p) => Ok(PathBuf::from(p)),
        None => Err("no path provided\nusage: rsend send <path>".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_args() {
        let args: Vec<String> = vec![];
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn test_valid_path() {
        let args = vec!["/tmp".to_string()];
        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn test_unexpected_args() {
        let args = vec!["/tmp".to_string(), "extra".to_string()];
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn test_error_message_no_args() {
        let args: Vec<String> = vec![];
        let err = validate_args(&args).unwrap_err();
        assert!(err.contains("no path provided"));
    }

    #[test]
    fn test_error_message_unexpected_args() {
        let args = vec!["/tmp".to_string(), "extra".to_string()];
        let err = validate_args(&args).unwrap_err();
        assert!(err.contains("unexpected arguments"));
    }
}
