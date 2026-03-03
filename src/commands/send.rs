use std::path::PathBuf;

pub fn run(args: &[String]) {
    match validate_args(args) {
        Ok(send_path) => {
            // proceed
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn validate_args(args: &[String]) -> Result<PathBuf, String> {
    if args.len() > 1 {
        return Err(format!(
            "unexpected arguments: {}\nusage: rsend send <path>",
            args[1..].join(", ")
        ));
    }

    match args.get(0) {
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
