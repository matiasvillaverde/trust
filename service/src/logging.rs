//! Best-effort operational logs. Financial evidence is committed by Journal,
//! never through this queue. A stopped collector must not stop protective work.
use std::io::Write;
use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};

pub const BUFFERED_LINES: usize = 2048;

pub fn buffered<T: Write + Send + 'static>(sink: T) -> (NonBlocking, WorkerGuard) {
    NonBlockingBuilder::default()
        .buffered_lines_limit(BUFFERED_LINES)
        .lossy(true)
        .thread_name("trust-operational-logs")
        .finish(sink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io, sync::mpsc, time::Duration};

    struct BlockedSink {
        entered: mpsc::SyncSender<()>,
        release: Option<mpsc::Receiver<()>>,
    }
    impl Write for BlockedSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(release) = self.release.take() {
                let _ = self.entered.send(());
                let _ = release.recv();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn blocked_log_sink_is_bounded_and_does_not_block_producers(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (mut writer, guard) = buffered(BlockedSink {
            entered: entered_tx,
            release: Some(release_rx),
        });
        let counter = writer.error_counter();
        writer.write_all(b"first\n")?;
        entered_rx.recv_timeout(Duration::from_secs(2))?;
        let (finished_tx, finished_rx) = mpsc::sync_channel(1);
        let producer = std::thread::spawn(move || {
            for _ in 0..BUFFERED_LINES.saturating_add(64) {
                let _ = writer.write_all(b"bounded event\n");
            }
            let _ = finished_tx.send(());
        });
        let completed = finished_rx.recv_timeout(Duration::from_secs(2));
        let dropped = counter.dropped_lines();
        // Always unblock the sink before checking the assertion or joining.
        release_tx.send(())?;
        producer.join().map_err(|_| "producer failed")?;
        drop(guard);
        assert!(
            completed.is_ok(),
            "a stalled collector blocked the producer"
        );
        assert_eq!(dropped, 64);
        let metrics = crate::metrics::Metrics::default();
        metrics
            .log_drops
            .set(counter)
            .map_err(|_| "counter already set")?;
        assert!(metrics
            .render(true, 8)
            .contains("trust_log_dropped_lines_total 64\n"));
        Ok(())
    }
}
