use super::*;

/// Frame bytes into complete UTF-8 lines. Readiness must also work without a
/// newline (some servers print prompts), independently of OS pipe chunk sizes.
pub(super) fn read_stream(
    mut reader: impl Read,
    channel: &str,
    markers: &[String],
    ready: SyncSender<()>,
    logs: SyncSender<(Stamp, String, String)>,
) {
    let mut pending = Vec::new();
    let mut ready_sent = markers.is_empty();
    loop {
        let mut data = [0; 4096];
        let n = reader.read(&mut data).unwrap_or(0);
        pending.extend_from_slice(&data[..n]);
        let ready_now = !ready_sent
            && markers
                .iter()
                .any(|m| pending.windows(m.len().max(1)).any(|w| w == m.as_bytes()));
        while let Some(end) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<_> = pending.drain(..=end).collect();
            emit_line(&line, channel, &logs);
        }
        if ready_now || n == 0 || pending.len() >= 1024 * 1024 {
            if !pending.is_empty() {
                emit_line(&pending, channel, &logs);
                pending.clear();
            }
            if ready_now {
                ready_sent = true;
                let _ = ready.try_send(());
            }
        }
        if n == 0 {
            break;
        }
    }
}
fn emit_line(bytes: &[u8], channel: &str, logs: &SyncSender<(Stamp, String, String)>) {
    let text = String::from_utf8_lossy(bytes)
        .trim_end_matches(['\r', '\n'])
        .to_string();
    if !text.is_empty() {
        let channel = if ["Error :", "Error:", "ERROR:"]
            .iter()
            .any(|p| text.trim_start().starts_with(p))
        {
            "server-error"
        } else {
            channel
        };
        let _ = logs.try_send((Stamp::now(), channel.into(), text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Bytes(std::io::Cursor<Vec<u8>>);
    impl Read for Bytes {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(&mut b[..1])
        }
    }
    #[test]
    fn fragmented_utf8_lines_and_ready_without_newline() {
        let (tx, rx) = mpsc::sync_channel(1);
        let (logs, lines) = mpsc::sync_channel(16);
        read_stream(
            Bytes(std::io::Cursor::new(
                "Info : 双核\r\nservice ready".as_bytes().to_vec(),
            )),
            "server-stderr",
            &["service ready".into()],
            tx,
            logs,
        );
        assert!(rx.try_recv().is_ok());
        assert_eq!(
            lines.try_iter().map(|(_, _, s)| s).collect::<Vec<_>>(),
            ["Info : 双核", "service ready"]
        );
    }
    #[test]
    fn stderr_is_not_error_but_real_openocd_error_is_retained() {
        let (logs, lines) = mpsc::sync_channel(8);
        emit_line(b"Info : target halted\r\n", "server-stderr", &logs);
        emit_line(b"Error : Failed to write memory\n", "server-stderr", &logs);
        let lines: Vec<_> = lines.try_iter().collect();
        assert_eq!(lines[0].1, "server-stderr");
        assert_eq!(lines[1].1, "server-error");
        assert_eq!(lines[1].2, "Error : Failed to write memory");
    }
}
