//! Bounded-memory streaming chunker.
//!
//! Reads a file from disk in 10 MiB pieces and feeds a finite-capacity
//! tokio mpsc channel. When all workers' buffers are full, `read_exact`
//! naturally applies backpressure because the producer `send().await`
//! suspends — so the reader can never outrun the workers and RAM is bounded
//! by `queue_depth * CHUNK_SIZE` regardless of file size.
//!
//! Memory budget (10 MiB chunks, queue_depth default 4):
//!   ~40 MiB in-flight + worker buffers + per-request overhead ≈ 50–150 MiB,
//! well under the 600 MiB transfer-RAM target.

use crate::tgcloud::{models::ChunkJob, CHUNK_SIZE};
use anyhow::{Context, Result};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Spawn the chunk-reader task. Returns the receiver workers pull from.
///
/// The reader owns the `File`; it reads exactly `CHUNK_SIZE` bytes per chunk
/// (the final chunk may be smaller) and sends each `ChunkJob`. Dropping the
/// receiver (all workers gone / cancelled) causes the reader to stop.
pub fn spawn_chunk_reader(
    file_path: &Path,
    file_name: String,
    file_id: String,
    total_chunks: u32,
    queue_depth: usize,
) -> mpsc::Receiver<ChunkJob> {
    let (tx, rx) = mpsc::channel(queue_depth.max(1));
    let path = file_path.to_path_buf();

    tokio::spawn(async move {
        if let Err(e) = run_reader(&path, &file_name, &file_id, total_chunks, tx.clone()).await {
            log::error!(
                "TGCloud chunk reader for {} failed: {e:#}",
                path.display()
            );
        }
    });

    rx
}

async fn run_reader(
    path: &Path,
    file_name: &str,
    file_id: &str,
    total_chunks: u32,
    tx: mpsc::Sender<ChunkJob>,
) -> Result<()> {
    let mut file = File::open(path)
        .await
        .with_context(|| format!("open {}", path.display()))?;

    let mut chunk_index: u32 = 1; // one-based, matching TG Cloud 1.2.0
    loop {
        // Allocate exactly one chunk buffer. It is MOVED into the channel
        // (and then into the worker), so there is no per-file accumulation.
        let mut buf = vec![0u8; CHUNK_SIZE as usize];
        let mut read_total = 0usize;

        // Fill the buffer, tolerating short reads (files, pipes, NFS).
        while read_total < buf.len() {
            let n = file
                .read(&mut buf[read_total..])
                .await
                .context("chunk read failed")?;
            if n == 0 {
                break; // EOF
            }
            read_total += n;
        }

        if read_total == 0 {
            break; // clean EOF
        }
        buf.truncate(read_total);

        let job = ChunkJob {
            file_id: file_id.to_string(),
            file_name: file_name.to_string(),
            chunk_index,
            total_chunks,
            data: buf,
        };

        // Backpressure: if the queue is full this waits until a worker
        // takes a chunk. If receivers are gone (cancel/shutdown) this
        // returns Err and we stop reading.
        if tx.send(job).await.is_err() {
            log::warn!("TGCloud chunk reader: receiver dropped, stopping at chunk {chunk_index}");
            break;
        }
        chunk_index += 1;
    }
    Ok(())
}

/// Compute total chunk count for a file of `size` bytes.
pub fn total_chunks_for_size(size: u64) -> u32 {
    if size == 0 {
        1
    } else {
        ((size + CHUNK_SIZE - 1) / CHUNK_SIZE) as u32
    }
}

/// Generate a fresh upload attempt / file UUID.
pub fn new_file_id() -> String {
    Uuid::new_v4().to_string()
}
