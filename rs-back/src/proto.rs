use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

/// Write a command token followed by a null terminator, e.g. b"predict" -> b"predict\0".
pub async fn write_command<W: AsyncWriteExt + Unpin>(w: &mut W, cmd: &str) -> io::Result<()> {
    w.write_all(cmd.as_bytes()).await?;
    w.write_u8(0).await
}

/// Write a big-endian u32 length prefix followed by the payload bytes.
pub async fn write_framed<W: AsyncWriteExt + Unpin>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    w.write_u32(payload.len() as u32).await?;
    w.write_all(payload).await
}

/// Read a big-endian u32 length prefix, then that many bytes.
pub async fn read_framed<R: AsyncReadExt + Unpin>(r: &mut R) -> io::Result<Vec<u8>> {
    let len = r.read_u32().await?;
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn command_has_null_terminator() {
        let mut buf = Vec::new();
        write_command(&mut buf, "predict").await.unwrap();
        assert_eq!(buf, b"predict\0");
    }

    #[tokio::test]
    async fn framed_roundtrip() {
        let mut buf = Vec::new();
        write_framed(&mut buf, b"hello").await.unwrap();
        // 4-byte big-endian length (5) + payload
        assert_eq!(&buf[0..4], &[0, 0, 0, 5]);
        assert_eq!(&buf[4..], b"hello");

        let mut cur = Cursor::new(buf);
        let out = read_framed(&mut cur).await.unwrap();
        assert_eq!(out, b"hello");
    }
}
