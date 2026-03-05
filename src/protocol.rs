use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct FileInfo {
    pub name: String,
    pub size: u64,
}

/// Send alias over the wire: `u16 len + UTF-8 bytes`
pub async fn send_alias(
    writer: &mut (impl AsyncWrite + Unpin),
    alias: &str,
) -> Result<(), String> {
    let bytes = alias.as_bytes();
    let len =
        u16::try_from(bytes.len()).map_err(|_| format!("alias too long: {}", bytes.len()))?;

    writer
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| format!("write alias len: {e}"))?;
    writer
        .write_all(bytes)
        .await
        .map_err(|e| format!("write alias: {e}"))?;

    Ok(())
}

/// Receive alias from the wire: `u16 len + UTF-8 bytes`
pub async fn recv_alias(reader: &mut (impl AsyncRead + Unpin)) -> Result<String, String> {
    let mut len_buf = [0u8; 2];
    reader
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("read alias len: {e}"))?;

    let len = u16::from_be_bytes(len_buf) as usize;
    if len == 0 {
        return Err("received empty alias".to_string());
    }

    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("read alias: {e}"))?;

    String::from_utf8(buf).map_err(|_| "alias is not valid UTF-8".to_string())
}

/// Send file manifest: `u32 count + [u16 name_len + name + u64 size]*`
pub async fn send_manifest(
    writer: &mut (impl AsyncWrite + Unpin),
    files: &[FileInfo],
) -> Result<(), String> {
    let count =
        u32::try_from(files.len()).map_err(|_| format!("too many files: {}", files.len()))?;

    writer
        .write_all(&count.to_be_bytes())
        .await
        .map_err(|e| format!("write file count: {e}"))?;

    for file in files {
        let name_bytes = file.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| format!("filename too long: {}", file.name))?;

        writer
            .write_all(&name_len.to_be_bytes())
            .await
            .map_err(|e| format!("write name len: {e}"))?;
        writer
            .write_all(name_bytes)
            .await
            .map_err(|e| format!("write name: {e}"))?;
        writer
            .write_all(&file.size.to_be_bytes())
            .await
            .map_err(|e| format!("write file size: {e}"))?;
    }

    Ok(())
}

/// Receive file manifest: `u32 count + [u16 name_len + name + u64 size]*`
pub async fn recv_manifest(
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<Vec<FileInfo>, String> {
    let mut count_buf = [0u8; 4];
    reader
        .read_exact(&mut count_buf)
        .await
        .map_err(|e| format!("read file count: {e}"))?;

    let count = u32::from_be_bytes(count_buf) as usize;
    let mut files = Vec::with_capacity(count);

    for _ in 0..count {
        let mut name_len_buf = [0u8; 2];
        reader
            .read_exact(&mut name_len_buf)
            .await
            .map_err(|e| format!("read name len: {e}"))?;

        let name_len = u16::from_be_bytes(name_len_buf) as usize;

        let mut name_buf = vec![0u8; name_len];
        reader
            .read_exact(&mut name_buf)
            .await
            .map_err(|e| format!("read name: {e}"))?;

        let name =
            String::from_utf8(name_buf).map_err(|_| "filename is not valid UTF-8".to_string())?;

        let mut size_buf = [0u8; 8];
        reader
            .read_exact(&mut size_buf)
            .await
            .map_err(|e| format!("read file size: {e}"))?;

        let size = u64::from_be_bytes(size_buf);

        files.push(FileInfo { name, size });
    }

    Ok(files)
}

/// Send consent signal: `u8` (1 = accept, 0 = reject)
pub async fn send_consent(
    writer: &mut (impl AsyncWrite + Unpin),
    accepted: bool,
) -> Result<(), String> {
    let signal: u8 = u8::from(accepted);
    writer
        .write_all(&[signal])
        .await
        .map_err(|e| format!("write consent: {e}"))?;
    Ok(())
}

/// Receive consent signal: `u8` (1 = accept, 0 = reject)
pub async fn recv_consent(reader: &mut (impl AsyncRead + Unpin)) -> Result<bool, String> {
    let mut buf = [0u8; 1];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("read consent: {e}"))?;

    match buf[0] {
        1 => Ok(true),
        0 => Ok(false),
        other => Err(format!("invalid consent signal: {other}")),
    }
}

/// Format bytes into human-friendly size string
#[allow(clippy::cast_precision_loss)] // acceptable for display-only formatting
pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;

    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kib() {
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
    }

    #[test]
    fn test_format_size_mib() {
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn test_format_size_gib() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GiB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GiB");
    }

    #[tokio::test]
    async fn test_alias_round_trip() {
        let (mut writer, mut reader) = duplex(1024);
        let alias = "bold-azure-falcon";

        send_alias(&mut writer, alias).await.unwrap();
        drop(writer); // signal EOF so reader doesn't hang
        let received = recv_alias(&mut reader).await.unwrap();
        assert_eq!(received, alias);
    }

    #[tokio::test]
    async fn test_alias_empty_rejected() {
        // Manually write a zero-length alias
        let (mut writer, mut reader) = duplex(1024);
        writer.write_all(&0u16.to_be_bytes()).await.unwrap();
        drop(writer);

        let err = recv_alias(&mut reader).await.unwrap_err();
        assert!(err.contains("empty alias"));
    }

    #[tokio::test]
    async fn test_manifest_round_trip() {
        let (mut writer, mut reader) = duplex(4096);
        let files = vec![
            FileInfo {
                name: "hello.txt".to_string(),
                size: 42,
            },
            FileInfo {
                name: "photo.jpg".to_string(),
                size: 1_500_000,
            },
        ];

        send_manifest(&mut writer, &files).await.unwrap();
        drop(writer);
        let received = recv_manifest(&mut reader).await.unwrap();

        assert_eq!(received.len(), 2);
        assert_eq!(received[0].name, "hello.txt");
        assert_eq!(received[0].size, 42);
        assert_eq!(received[1].name, "photo.jpg");
        assert_eq!(received[1].size, 1_500_000);
    }

    #[tokio::test]
    async fn test_manifest_empty() {
        let (mut writer, mut reader) = duplex(1024);
        let files: Vec<FileInfo> = vec![];

        send_manifest(&mut writer, &files).await.unwrap();
        drop(writer);
        let received = recv_manifest(&mut reader).await.unwrap();

        assert!(received.is_empty());
    }

    #[tokio::test]
    async fn test_consent_accept() {
        let (mut writer, mut reader) = duplex(1024);

        send_consent(&mut writer, true).await.unwrap();
        drop(writer);
        assert!(recv_consent(&mut reader).await.unwrap());
    }

    #[tokio::test]
    async fn test_consent_reject() {
        let (mut writer, mut reader) = duplex(1024);

        send_consent(&mut writer, false).await.unwrap();
        drop(writer);
        assert!(!recv_consent(&mut reader).await.unwrap());
    }

    #[tokio::test]
    async fn test_consent_invalid_signal() {
        let (mut writer, mut reader) = duplex(1024);
        writer.write_all(&[42]).await.unwrap();
        drop(writer);

        let err = recv_consent(&mut reader).await.unwrap_err();
        assert!(err.contains("invalid consent signal"));
    }
}
