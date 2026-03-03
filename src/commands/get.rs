use crate::pairing::code;
use std::io::{self, Write};
use std::path::PathBuf;

pub fn run(args: &[String]) {
    match parse_args(args) {
        Ok(dir) => {
            if let Err(e) = validate(&dir) {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
            println!("receiving to: {}", dir.display());

            let pairing_code = match prompt_code() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            };

            println!("looking up sender...");
            // proceed to DHT lookup with pairing_code
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn parse_args(args: &[String]) -> Result<PathBuf, String> {
    if args.len() > 1 {
        return Err(format!(
            "unexpected arguments: {}\nusage: rsend get [directory]",
            args[1..].join(", ")
        ));
    }

    match args.get(0) {
        Some(p) => Ok(PathBuf::from(p)),
        None => {
            std::env::current_dir().map_err(|e| format!("failed to get current directory: {}", e))
        }
    }
}

fn validate(dir: &PathBuf) -> Result<(), String> {
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
        .map_err(|e| format!("failed to flush stdout: {}", e))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("failed to read input: {}", e))?;

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
}
