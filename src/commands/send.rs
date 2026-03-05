use crate::crypto::hash;
use crate::pairing::{alias, code::PairingCode};
use crate::transfer;
use crate::transport::{dht, quic::QuicListener};
use std::path::PathBuf;

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

    let dht_key = hash::derive_dht_key(&pairing_code.value);

    let listener = QuicListener::bind(0)?;
    let port = listener.port()?;

    dht::announce(&dht_key, port)?;

    println!("waiting to pair...");

    let mut stream = listener.accept().await?;
    let files = transfer::collect_files(&send_path)?;
    transfer::send_files(&mut stream, files, |_| {}).await?;

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
