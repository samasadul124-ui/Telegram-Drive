//! Dynamic multi-bot worker pool with a bounded chunk queue.
//!
//! Workers pull `ChunkJob`s from the shared receiver (work-stealing): the
//! first available bot claims the next chunk, producing out-of-order arrival
//! just like the real TG Cloud balancer. There is no fixed "bot 1 = chunk 1"
//! assignment. Each result records which `bot_worker_id` uploaded it.

use crate::tgcloud::{
    bot::BotApiClient,
    chunker,
    crypto,
    models::{ChunkJob, ChunkUploadOutcome, TransferProgress},
    CHUNK_SIZE,
};
use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Outcome of a full chunked upload.
pub struct UploadReport {
    pub chunks: Vec<ChunkUploadOutcome>,
    pub logical_sha256: String,
    pub telegram_message_id: i64,
}

/// Run a chunked upload with the given bot tokens.
///
/// * `file_path`  — source file on disk (streamed, never fully loaded).
/// * `file_name`  — logical filename (used in chunk filenames + caption).
/// * `file_id`    — stable logical UUID.
/// * `channel_id` — the ONE storage channel.
/// * `tokens`     — 1..=5 enabled bot tokens.
/// * `queue_depth`— bounded in-flight chunk count (RAM = queue_depth × 10 MiB).
/// * `cancelled`  — shared atomic flag checked cooperatively.
/// * `on_progress`— UI callback.
///
/// The reader blocks on the bounded queue when workers are saturated, so RAM
/// is independent of file size. On failure/cancel, uploaded chunks are
/// best-effort deleted and the error propagates (no resume, per spec).
pub async fn upload_chunked<F>(
    file_path: &Path,
    file_name: &str,
    file_id: &str,
    file_size: u64,
    channel_id: i64,
    tokens: &[String],
    queue_depth: usize,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    on_progress: F,
) -> Result<UploadReport>
where
    F: Fn(TransferProgress) + Send + Sync + 'static,
{
    let total_chunks = chunker::total_chunks_for_size(file_size);
    let upload_attempt_id = uuid::Uuid::new_v4().to_string();

    on_progress(TransferProgress::Started {
        upload_attempt_id: upload_attempt_id.clone(),
        file_id: file_id.to_string(),
        file_name: file_name.to_string(),
        total_bytes: file_size,
        total_chunks,
    });

    // Shared result collector + logical-hasher, protected by a mutex.
    let state = Arc::new(Mutex::new(UploadState::new(total_chunks)));
    let progress_cb = Arc::new(on_progress);
    let progress_cb2 = progress_cb.clone();
    let attempt_id2 = upload_attempt_id.clone();

    // Receiver is shared (mutable) across workers via Mutex.
    let receiver = Arc::new(Mutex::new(chunker::spawn_chunk_reader(
        file_path,
        file_name.to_string(),
        file_id.to_string(),
        total_chunks,
        queue_depth,
    )));

    let clients: Vec<(u8, BotApiClient)> = tokens
        .iter()
        .enumerate()
        .map(|(i, t)| ((i + 1) as u8, BotApiClient::new(t)))
        .collect();

    let mut handles: Vec<JoinHandle<Result<()>>> = Vec::with_capacity(clients.len());

    for (worker_id, client) in clients {
        let rx = receiver.clone();
        let st = state.clone();
        let cancel = cancelled.clone();
        let cb = progress_cb.clone();
        let attempt = upload_attempt_id.clone();
        let fname = file_name.to_string();
        let fid = file_id.to_string();

        handles.push(tokio::spawn(async move {
            worker_loop(
                worker_id,
                client,
                channel_id,
                rx,
                st,
                cancel,
                cb,
                attempt,
                fid,
                fname,
                total_chunks,
            )
            .await
        }));
    }

    // Wait for all workers.
    let mut first_error: Option<String> = None;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_error.is_none() {
                    first_error = Some(e.to_string());
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(format!("worker join: {e}"));
                }
            }
        }
    }

    // Mark cancelled flag so any stragglers stop.
    cancelled.store(true, std::sync::atomic::Ordering::SeqCst);

    let final_state = state.lock().await;

    if let Some(e) = first_error {
        // Best-effort cleanup of orphaned chunks.
        best_effort_cleanup(&clients, channel_id, &final_state.outcomes).await;
        progress_cb2(TransferProgress::Failed {
            upload_attempt_id: attempt_id2,
            error: e.clone(),
        });
        return Err(anyhow!(e));
    }

    if final_state.outcomes.len() != total_chunks as usize {
        let msg = format!(
            "incomplete upload: {}/{} chunks",
            final_state.outcomes.len(),
            total_chunks
        );
        best_effort_cleanup(&clients, channel_id, &final_state.outcomes).await;
        progress_cb2(TransferProgress::Failed {
            upload_attempt_id: attempt_id2,
            error: msg.clone(),
        });
        return Err(anyhow!(msg));
    }

    progress_cb2(TransferProgress::Finalizing {
        upload_attempt_id: attempt_id2.clone(),
    });

    let mut chunks = final_state.outcomes.clone();
    chunks.sort_by_key(|c| c.chunk_index);

    let logical_sha256 = final_state.logical_hash_hex();
    let telegram_message_id = chunks
        .last()
        .map(|c| c.telegram_message_id)
        .unwrap_or(0);

    Ok(UploadReport {
        chunks,
        logical_sha256,
        telegram_message_id,
    })
}

struct UploadState {
    outcomes: Vec<ChunkUploadOutcome>,
    hasher: sha2::Sha256,
}

impl UploadState {
    fn new(total_chunks: u32) -> Self {
        Self {
            outcomes: Vec::with_capacity(total_chunks as usize),
            hasher: sha2::Sha256::new(),
        }
    }

    fn record(&mut self, o: ChunkUploadOutcome, payload: &[u8]) {
        use sha2::Digest;
        self.hasher.update(payload);
        self.outcomes.push(o);
    }

    fn logical_hash_hex(&self) -> String {
        use sha2::Digest;
        // Clone because finalize consumes.
        let h = self.hasher.clone();
        format!("{:x}", h.finalize())
    }
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop<F>(
    worker_id: u8,
    client: BotApiClient,
    channel_id: i64,
    rx: Arc<Mutex<tokio::sync::mpsc::Receiver<ChunkJob>>>,
    state: Arc<Mutex<UploadState>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    cb: Arc<F>,
    attempt: String,
    file_id: String,
    file_name: String,
    total_chunks: u32,
) -> Result<()>
where
    F: Fn(TransferProgress) + Send + Sync,
{
    loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(anyhow!("upload cancelled"));
        }

        let job = {
            let mut r = rx.lock().await;
            r.recv().await
        };

        let job = match job {
            Some(j) => j,
            None => return Ok(()), // reader finished
        };

        let chunk_hash = crypto::md5_hex(&job.data);
        let chunk_filename = format!(
            "{}.chunk_{}_of_{}",
            file_name, job.chunk_index, total_chunks
        );
        let caption = format!(
            "[CHUNK]|fileId:{}|chunk:{}|total:{}|name:{}|hash:{}",
            file_id, job.chunk_index, job.total_chunks, file_name, chunk_hash
        );

        let data_len = job.data.len() as u64;
        let data_for_hash = job.data.clone(); // for logical hasher; 10 MiB max

        let msg = client
            .send_document(channel_id, job.data, &chunk_filename, &caption)
            .await?;

        let doc = msg
            .document
            .as_ref()
            .ok_or_else(|| anyhow!("sendDocument returned no document"))?;

        let outcome = ChunkUploadOutcome {
            chunk_index: job.chunk_index,
            chunk_size: data_len,
            chunk_hash,
            telegram_message_id: msg.message_id,
            telegram_file_id: doc.file_id.clone(),
            telegram_file_unique_id: doc.file_unique_id.clone(),
            bot_worker_id: worker_id,
        };

        {
            let mut st = state.lock().await;
            st.record(outcome.clone(), &data_for_hash);
        }

        cb(TransferProgress::ChunkUploaded {
            upload_attempt_id: attempt.clone(),
            chunk_index: job.chunk_index,
            bot_worker_id: worker_id,
            bytes_sent: data_len,
        });
    }
}

async fn best_effort_cleanup(
    clients: &[(u8, BotApiClient)],
    channel_id: i64,
    outcomes: &[ChunkUploadOutcome],
) {
    for o in outcomes {
        // Use the same worker that uploaded it if possible, else any client.
        let client = clients
            .iter()
            .find(|(id, _)| *id == o.bot_worker_id)
            .map(|(_, c)| c)
            .or_else(|| clients.first().map(|(_, c)| c));
        if let Some(c) = client {
            let _ = c.delete_message(channel_id, o.telegram_message_id).await;
        }
    }
}

/// Download a chunked file, writing chunks to the target file at the correct
/// offsets. Downloads are scheduled across workers and may complete out of
/// order; positional writes ensure correct reconstruction. RAM is bounded by
/// the number of concurrent chunk downloads.
pub async fn download_chunked<F>(
    target_path: &Path,
    total_chunks: u32,
    chunk_size: u64,
    file_ids: Vec<(u32, String, u8)>, // (chunk_index, telegram_file_id, worker_hint)
    clients: &[(u8, BotApiClient)],
    queue_depth: usize,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    on_progress: F,
) -> Result<()>
where
    F: Fn(u64, u64) + Send + Sync + 'static, // (downloaded_bytes, total_bytes)
{
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};

    let total_bytes = chunk_size.saturating_mul(total_chunks as u64);
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(target_path)
        .await?;
    let file = Arc::new(tokio::sync::Mutex::new(file));

    let list: Arc<std::sync::Mutex<Vec<(u32, String, u8)>>> =
        Arc::new(std::sync::Mutex::new(file_ids));
    let downloaded = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let cb = Arc::new(on_progress);

    let mut handles = Vec::new();
    let concurrency = clients.len().min(queue_depth).max(1);
    let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));

    for (worker_id, client) in clients {
        let list = list.clone();
        let file = file.clone();
        let cancel = cancelled.clone();
        let dl = downloaded.clone();
        let cb = cb.clone();
        let sem = sem.clone();
        let client = client.clone();
        let worker_id = *worker_id;

        handles.push(tokio::spawn(async move {
            loop {
                if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                    return Err(anyhow!("download cancelled"));
                }
                let next = {
                    let mut l = list.lock().unwrap();
                    if l.is_empty() {
                        None
                    } else {
                        // Prefer chunks hinted to this worker, else take any.
                        let pos = l
                            .iter()
                            .position(|(_, _, w)| *w == worker_id)
                            .unwrap_or(0);
                        Some(l.remove(pos))
                    }
                };
                let (chunk_index, file_id, _hint) = match next {
                    Some(v) => v,
                    None => return Ok(()),
                };

                let _permit = sem.acquire().await?;
                let tg_file = client.get_file(&file_id).await?;
                let path = tg_file
                    .file_path
                    .ok_or_else(|| anyhow!("getFile returned no file_path"))?;
                let bytes = client.download_file(&path).await?;

                let offset = (chunk_index as u64 - 1) * chunk_size;
                let mut f = file.lock().await;
                f.seek(std::io::SeekFrom::Start(offset)).await?;
                f.write_all(&bytes).await?;
                drop(f);

                let new_dl = dl.fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::SeqCst)
                    + bytes.len() as u64;
                cb(new_dl, total_bytes);
            }
        }));
    }

    for h in handles {
        h.await??;
    }

    // Flush + sync metadata.
    let mut f = file.lock().await;
    f.flush().await?;
    f.sync_all().await?;
    Ok(())
}

// Keep CHUNK_SIZE referenced for documentation / future configurable use.
const _: u64 = CHUNK_SIZE;
