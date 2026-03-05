use std::path::{Path, PathBuf};

use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
};

const CHUNK: usize = 64 * 1024; // 64 KB

pub struct Progress {
    pub filename: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Sender: streams all files then sends terminator.
///
/// `base` is the root directory; each entry in `files` is a relative path
/// from `base`. The relative path is sent over the wire so the receiver
/// can recreate subdirectories.
pub async fn send_files(
    writer: &mut (impl AsyncWrite + Unpin),
    base: &Path,
    files: &[PathBuf],
    on_progress: impl Fn(Progress),
) -> Result<(), String> {
    for rel_path in files {
        let abs_path = base.join(rel_path);
        let filename = rel_path.to_string_lossy().to_string();

        let mut file = File::open(&abs_path)
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
        writer
            .write_all(&name_len.to_be_bytes())
            .await
            .map_err(|e| format!("write name_len: {e}"))?;
        writer
            .write_all(name_bytes)
            .await
            .map_err(|e| format!("write name: {e}"))?;
        writer
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

            writer
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
    writer
        .write_all(&0u16.to_be_bytes())
        .await
        .map_err(|e| format!("write terminator: {e}"))?;

    Ok(())
}

/// Walk a path and return `(base_dir, relative_paths)`.
///
/// - Single file `/tmp/test.txt` → `(/tmp, [test.txt])`
/// - Directory `/tmp/secrets/`   → `(/tmp/secrets, [.env, sub/keys.json])`
pub fn collect_files(path: &Path) -> Result<(PathBuf, Vec<PathBuf>), String> {
    if !path.exists() {
        return Err(format!("'{}' does not exist", path.display()));
    }
    if path.is_file() {
        let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = path
            .file_name()
            .ok_or_else(|| format!("'{}' has no filename", path.display()))?;
        return Ok((base, vec![PathBuf::from(name)]));
    }
    if path.is_dir() {
        let mut files = Vec::new();
        collect_dir(path, path, &mut files)?;
        if files.is_empty() {
            return Err(format!("'{}' contains no files", path.display()));
        }
        return Ok((path.to_path_buf(), files));
    }
    Err(format!(
        "'{}' is not a regular file or directory",
        path.display()
    ))
}

fn collect_dir(base: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("failed to read directory '{}': {e}", dir.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| format!("failed to read entry in '{}': {e}", dir.display()))?;
        let path = entry.path();
        if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| format!("strip_prefix failed for '{}': {e}", path.display()))?
                .to_path_buf();
            files.push(rel);
        } else if path.is_dir() {
            collect_dir(base, &path, files)?;
        }
    }
    Ok(())
}

/// Receiver: reads files until terminator, writes to `dest_dir`.
///
/// Relative paths from the sender are preserved — subdirectories are
/// created automatically.  Paths containing `..` or starting with `/`
/// are rejected to prevent path-traversal attacks.
pub async fn recv_files(
    reader: &mut (impl AsyncRead + Unpin),
    dest_dir: &Path,
    on_progress: impl Fn(Progress),
) -> Result<(), String> {
    loop {
        // read name_len — 0 means done
        let mut name_len_buf = [0u8; 2];
        reader
            .read_exact(&mut name_len_buf)
            .await
            .map_err(|e| format!("read name_len: {e}"))?;

        let name_len = u16::from_be_bytes(name_len_buf) as usize;
        if name_len == 0 {
            break;
        }

        // read filename
        let mut name_buf = vec![0u8; name_len];
        reader
            .read_exact(&mut name_buf)
            .await
            .map_err(|e| format!("read filename: {e}"))?;
        let filename =
            String::from_utf8(name_buf).map_err(|_| "filename is not valid UTF-8".to_string())?;

        // read file_size
        let mut size_buf = [0u8; 8];
        reader
            .read_exact(&mut size_buf)
            .await
            .map_err(|e| format!("read file_size: {e}"))?;
        let file_size = u64::from_be_bytes(size_buf);

        // validate path — reject traversal attempts
        let rel = PathBuf::from(&filename);
        if rel.has_root() {
            return Err(format!("rejected absolute path: {filename}"));
        }
        for component in rel.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(format!("rejected path with '..': {filename}"));
            }
        }

        let dest_path = dest_dir.join(&rel);

        // create parent directories for nested files
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("create dirs for {filename}: {e}"))?;
        }

        let mut file = File::create(&dest_path)
            .await
            .map_err(|e| format!("create {filename}: {e}"))?;

        // read file data in chunks
        let mut buf = vec![0u8; CHUNK];
        let mut bytes_done = 0u64;

        while bytes_done < file_size {
            #[allow(clippy::cast_possible_truncation)] // CHUNK is 64 KB, min clamps
            let to_read = ((file_size - bytes_done) as usize).min(CHUNK);

            reader
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

        let (base, files) = collect_files(&file).unwrap();
        assert_eq!(base, dir);
        assert_eq!(files, vec![PathBuf::from("hello.txt")]);
    }

    #[test]
    fn test_collect_directory_with_files() {
        let dir = tempdir("dir_files");
        fs::write(dir.join("a.txt"), "aaa").unwrap();
        fs::write(dir.join("b.txt"), "bbb").unwrap();

        let (base, mut files) = collect_files(&dir).unwrap();
        files.sort();
        assert_eq!(base, dir);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], PathBuf::from("a.txt"));
        assert_eq!(files[1], PathBuf::from("b.txt"));
    }

    #[test]
    fn test_collect_nested_directory() {
        let dir = tempdir("nested");
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(dir.join("top.txt"), "top").unwrap();
        fs::write(sub.join("nested.txt"), "nested").unwrap();

        let (base, mut files) = collect_files(&dir).unwrap();
        files.sort();
        assert_eq!(base, dir);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], PathBuf::from("sub/nested.txt"));
        assert_eq!(files[1], PathBuf::from("top.txt"));
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

    /// Full round-trip: collect → send → recv over an in-memory duplex,
    /// then verify received files match the originals.
    #[tokio::test]
    async fn test_send_recv_round_trip_nested() {
        // build a source tree with nested files
        let src = tempdir("rt_src");
        let sub = src.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(src.join("top.txt"), "hello from top").unwrap();
        fs::write(sub.join("deep.txt"), "hello from sub").unwrap();

        let (base, mut files) = collect_files(&src).unwrap();
        files.sort(); // deterministic order

        // wire: duplex acts as our "QUIC stream"
        let (mut writer, mut reader) = tokio::io::duplex(128 * 1024);

        let send_base = base.clone();
        let send_files = files.clone();
        let send_handle = tokio::spawn(async move {
            send_files_fn(&mut writer, &send_base, &send_files, |_| {}).await
        });

        let dest = tempdir("rt_dest");
        let recv_dest = dest.clone();
        let recv_handle = tokio::spawn(async move {
            recv_files(&mut reader, &recv_dest, |_| {}).await
        });

        send_handle.await.unwrap().unwrap();
        recv_handle.await.unwrap().unwrap();

        // verify every file exists with correct contents
        for rel in &files {
            let original = fs::read(base.join(rel)).unwrap();
            let received = fs::read(dest.join(rel)).unwrap();
            assert_eq!(
                original, received,
                "content mismatch for {}",
                rel.display()
            );
        }

        // verify no extra files were created
        let (_, received_files) = collect_files(&dest).unwrap();
        assert_eq!(received_files.len(), files.len());
    }

    /// Verify recv_files rejects paths containing `..`.
    #[tokio::test]
    async fn test_recv_rejects_path_traversal() {
        let (mut writer, mut reader) = tokio::io::duplex(1024);

        // manually craft a malicious header: filename = "../etc/passwd", size = 0
        let malicious = "../etc/passwd";
        let name_bytes = malicious.as_bytes();
        let send_handle = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let name_len = name_bytes.len() as u16;
            writer.write_all(&name_len.to_be_bytes()).await.unwrap();
            writer.write_all(name_bytes).await.unwrap();
            writer.write_all(&0u64.to_be_bytes()).await.unwrap();
        });

        let dest = tempdir("rt_traversal");
        let result = recv_files(&mut reader, &dest, |_| {}).await;

        send_handle.await.unwrap();
        let err = result.unwrap_err();
        assert!(err.contains(".."), "expected traversal rejection, got: {err}");
    }

    /// Verify recv_files rejects absolute paths.
    #[tokio::test]
    async fn test_recv_rejects_absolute_path() {
        let (mut writer, mut reader) = tokio::io::duplex(1024);

        let malicious = "/etc/passwd";
        let name_bytes = malicious.as_bytes();
        let send_handle = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let name_len = name_bytes.len() as u16;
            writer.write_all(&name_len.to_be_bytes()).await.unwrap();
            writer.write_all(name_bytes).await.unwrap();
            writer.write_all(&0u64.to_be_bytes()).await.unwrap();
        });

        let dest = tempdir("rt_absolute");
        let result = recv_files(&mut reader, &dest, |_| {}).await;

        send_handle.await.unwrap();
        let err = result.unwrap_err();
        assert!(
            err.contains("absolute"),
            "expected absolute path rejection, got: {err}"
        );
    }

    // Alias so tests can call `send_files` without conflicting with
    // the `send_files` variable binding in the round-trip test.
    use super::send_files as send_files_fn;
}
