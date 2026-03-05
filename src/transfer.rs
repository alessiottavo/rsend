use std::path::{Path, PathBuf};

use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::transport::quic::QuicStream;

const CHUNK: usize = 64 * 1024; // 64 KB

pub struct Progress {
    pub filename: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Sender: streams all files then sends terminator
pub async fn send_files(
    stream: &mut QuicStream,
    files: Vec<PathBuf>,
    on_progress: impl Fn(Progress),
) -> Result<(), String> {
    for path in &files {
        let filename = path
            .file_name()
            .ok_or("path has no filename")?
            .to_string_lossy()
            .to_string();

        let mut file = File::open(path)
            .await
            .map_err(|e| format!("open {filename}: {e}"))?;

        let file_size = file
            .metadata()
            .await
            .map_err(|e| format!("stat {filename}: {e}"))?
            .len();

        // header
        let name_bytes = filename.as_bytes();
        let name_len =
            u16::try_from(name_bytes.len()).map_err(|_| format!("filename too long: {filename}"))?;
        stream
            .send
            .write_all(&name_len.to_be_bytes())
            .await
            .map_err(|e| format!("write name_len: {e}"))?;
        stream
            .send
            .write_all(name_bytes)
            .await
            .map_err(|e| format!("write name: {e}"))?;
        stream
            .send
            .write_all(&file_size.to_be_bytes())
            .await
            .map_err(|e| format!("write file_size: {e}"))?;

        // file data
        let mut buf = vec![0u8; CHUNK];
        let mut bytes_done = 0u64;

        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| format!("read {filename}: {e}"))?;
            if n == 0 {
                break;
            }

            stream
                .send
                .write_all(&buf[..n])
                .await
                .map_err(|e| format!("write chunk: {e}"))?;

            bytes_done += n as u64;
            on_progress(Progress {
                filename: filename.clone(),
                bytes_done,
                bytes_total: file_size,
            });
        }
    }

    // terminator — name_len = 0 signals no more files
    stream
        .send
        .write_all(&0u16.to_be_bytes())
        .await
        .map_err(|e| format!("write terminator: {e}"))?;

    stream
        .send
        .finish()
        .map_err(|e| format!("finish stream: {e}"))?;

    Ok(())
}

/// Walk a path and return all files. Single file → `vec![path]`, directory → recurse.
pub fn collect_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Err(format!("'{}' does not exist", path.display()));
    }
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if path.is_dir() {
        let mut files = Vec::new();
        collect_dir(path, &mut files)?;
        if files.is_empty() {
            return Err(format!("'{}' contains no files", path.display()));
        }
        return Ok(files);
    }
    Err(format!(
        "'{}' is not a regular file or directory",
        path.display()
    ))
}

fn collect_dir(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("failed to read directory '{}': {e}", dir.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| format!("failed to read entry in '{}': {e}", dir.display()))?;
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            collect_dir(&path, files)?;
        }
    }
    Ok(())
}

/// Receiver: reads files until terminator, writes to `dest_dir`
pub async fn recv_files(
    stream: &mut QuicStream,
    dest_dir: PathBuf,
    on_progress: impl Fn(Progress),
) -> Result<(), String> {
    loop {
        // read name_len — 0 means done
        let mut name_len_buf = [0u8; 2];
        stream
            .recv
            .read_exact(&mut name_len_buf)
            .await
            .map_err(|e| format!("read name_len: {e}"))?;

        let name_len = u16::from_be_bytes(name_len_buf) as usize;
        if name_len == 0 {
            break;
        }

        // read filename
        let mut name_buf = vec![0u8; name_len];
        stream
            .recv
            .read_exact(&mut name_buf)
            .await
            .map_err(|e| format!("read filename: {e}"))?;
        let filename =
            String::from_utf8(name_buf).map_err(|_| "filename is not valid UTF-8".to_string())?;

        // read file_size
        let mut size_buf = [0u8; 8];
        stream
            .recv
            .read_exact(&mut size_buf)
            .await
            .map_err(|e| format!("read file_size: {e}"))?;
        let file_size = u64::from_be_bytes(size_buf);

        // create output file — reject path traversal attempts
        let safe_name = PathBuf::from(&filename)
            .file_name()
            .ok_or("filename contains path separators")?
            .to_owned();
        let dest_path = dest_dir.join(safe_name);

        let mut file = File::create(&dest_path)
            .await
            .map_err(|e| format!("create {filename}: {e}"))?;

        // read file data in chunks
        let mut buf = vec![0u8; CHUNK];
        let mut bytes_done = 0u64;

        while bytes_done < file_size {
            #[allow(clippy::cast_possible_truncation)] // CHUNK is 64 KB, min clamps
            let to_read = ((file_size - bytes_done) as usize).min(CHUNK);

            stream
                .recv
                .read_exact(&mut buf[..to_read])
                .await
                .map_err(|e| format!("read chunk for {filename}: {e}"))?;

            file.write_all(&buf[..to_read])
                .await
                .map_err(|e| format!("write chunk to {filename}: {e}"))?;

            bytes_done += to_read as u64;
            on_progress(Progress {
                filename: filename.clone(),
                bytes_done,
                bytes_total: file_size,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_collect_single_file() {
        let dir = tempdir("single_file");
        let file = dir.join("hello.txt");
        fs::write(&file, "content").unwrap();

        let result = collect_files(&file).unwrap();
        assert_eq!(result, vec![file]);
    }

    #[test]
    fn test_collect_directory_with_files() {
        let dir = tempdir("dir_files");
        fs::write(dir.join("a.txt"), "aaa").unwrap();
        fs::write(dir.join("b.txt"), "bbb").unwrap();

        let mut result = collect_files(&dir).unwrap();
        result.sort();
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|p| p.is_file()));
    }

    #[test]
    fn test_collect_nested_directory() {
        let dir = tempdir("nested");
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(dir.join("top.txt"), "top").unwrap();
        fs::write(sub.join("nested.txt"), "nested").unwrap();

        let result = collect_files(&dir).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_collect_nonexistent_path() {
        let err = collect_files(Path::new("/tmp/rsend_does_not_exist_12345")).unwrap_err();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn test_collect_empty_directory() {
        let dir = tempdir("empty");
        let err = collect_files(&dir).unwrap_err();
        assert!(err.contains("contains no files"));
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rsend_test_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
