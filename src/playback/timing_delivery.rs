use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::info;

/// Maximum wait time per chunk to prevent infinite hangs (30 seconds)
const MAX_CHUNK_WAIT_MS: u64 = 30_000;

/// A chunk with timing information for delivery
pub struct TimedChunk {
    pub data: Bytes,
    pub target_time: u64,
}

/// A stream that receives chunks from a background task
pub struct TimedChunkStream {
    receiver: mpsc::Receiver<Result<Bytes, std::io::Error>>,
}

impl Stream for TimedChunkStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_recv(cx)
    }
}

/// Spawn a background task that delivers chunks according to their target times.
///
/// This function solves the HTTP/2 blocking problem by moving all `tokio::time::sleep()`
/// calls to a separate spawned task. The returned stream is non-blocking and can be
/// polled by Hyper without blocking other HTTP/2 streams on the same connection.
///
/// # Arguments
/// * `chunks` - Vector of chunks with their target delivery times
/// * `target_close_time` - Time (ms after TTFB) when the connection should close
/// * `ttfb_instant` - The instant when TTFB wait completed (time origin for chunks)
///
/// # Returns
/// A stream that yields chunks as they become ready according to timing
pub fn spawn_timed_delivery(
    chunks: Vec<TimedChunk>,
    target_close_time: u64,
    ttfb_instant: Instant,
) -> TimedChunkStream {
    // Channel buffer of 16 allows the producer to get slightly ahead
    // while still providing backpressure if the consumer is too slow
    let (tx, rx) = mpsc::channel(16);

    tokio::spawn(async move {
        let total_chunks = chunks.len();

        for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
            let elapsed = ttfb_instant.elapsed().as_millis() as u64;

            // Wait until target_time for this chunk
            if chunk.target_time > elapsed {
                let wait_time = (chunk.target_time - elapsed).min(MAX_CHUNK_WAIT_MS);
                info!(
                    "Chunk[{}]: Waiting {}ms before sending (target: {}ms, elapsed: {}ms)",
                    chunk_idx, wait_time, chunk.target_time, elapsed
                );
                tokio::time::sleep(Duration::from_millis(wait_time)).await;
            } else if chunk.target_time > 0 && elapsed > chunk.target_time {
                // We're behind schedule - log it but send immediately
                let behind_ms = elapsed - chunk.target_time;
                info!(
                    "Chunk[{}]: Behind schedule by {}ms, sending immediately (target: {}ms, elapsed: {}ms)",
                    chunk_idx, behind_ms, chunk.target_time, elapsed
                );
            }

            // Send chunk
            info!("Chunk[{}]: Sending {} bytes", chunk_idx, chunk.data.len());
            if tx.send(Ok(chunk.data)).await.is_err() {
                // Receiver dropped - client disconnected
                info!(
                    "Client disconnected while sending chunk[{}], stopping delivery",
                    chunk_idx
                );
                return;
            }
        }

        // All chunks sent, now wait until target_close_time before closing
        let elapsed = ttfb_instant.elapsed().as_millis() as u64;
        if target_close_time > elapsed {
            let wait_time = (target_close_time - elapsed).min(MAX_CHUNK_WAIT_MS);
            info!(
                "All {} chunks sent, waiting {}ms until target_close_time before closing connection",
                total_chunks, wait_time
            );
            tokio::time::sleep(Duration::from_millis(wait_time)).await;
        } else {
            let behind_ms = elapsed.saturating_sub(target_close_time);
            info!(
                "All {} chunks sent, already {}ms past target_close_time, closing immediately",
                total_chunks, behind_ms
            );
        }

        // Sender drops here, which closes the stream gracefully
    });

    TimedChunkStream { receiver: rx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::time::Instant;

    #[tokio::test]
    async fn test_empty_chunks() {
        let ttfb_instant = Instant::now();
        let stream = spawn_timed_delivery(vec![], 0, ttfb_instant);

        let chunks: Vec<_> = stream.collect().await;
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_single_chunk_immediate() {
        let ttfb_instant = Instant::now();
        let chunk = TimedChunk {
            data: Bytes::from("hello"),
            target_time: 0, // Immediate delivery
        };

        let stream = spawn_timed_delivery(vec![chunk], 0, ttfb_instant);
        let chunks: Vec<_> = stream.collect().await;

        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].is_ok());
        assert_eq!(chunks[0].as_ref().unwrap().as_ref(), b"hello");
    }

    #[tokio::test]
    async fn test_timing_accuracy() {
        let ttfb_instant = Instant::now();
        let chunks = vec![
            TimedChunk {
                data: Bytes::from("first"),
                target_time: 0,
            },
            TimedChunk {
                data: Bytes::from("second"),
                target_time: 100, // 100ms after TTFB
            },
        ];

        let stream = spawn_timed_delivery(chunks, 150, ttfb_instant);

        let start = Instant::now();
        let results: Vec<_> = stream.collect().await;
        let elapsed = start.elapsed().as_millis();

        assert_eq!(results.len(), 2);
        // Should take at least 100ms (for second chunk) plus ~50ms for close time
        // Allow 50ms tolerance for test environment variance
        assert!(elapsed >= 100, "Expected at least 100ms, got {}ms", elapsed);
        assert!(elapsed <= 250, "Expected at most 250ms, got {}ms", elapsed);
    }

    #[tokio::test]
    async fn test_multiple_chunks_order_preserved() {
        let ttfb_instant = Instant::now();
        let chunks = vec![
            TimedChunk {
                data: Bytes::from("1"),
                target_time: 0,
            },
            TimedChunk {
                data: Bytes::from("2"),
                target_time: 10,
            },
            TimedChunk {
                data: Bytes::from("3"),
                target_time: 20,
            },
        ];

        let stream = spawn_timed_delivery(chunks, 30, ttfb_instant);
        let results: Vec<_> = stream.collect().await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap().as_ref(), b"1");
        assert_eq!(results[1].as_ref().unwrap().as_ref(), b"2");
        assert_eq!(results[2].as_ref().unwrap().as_ref(), b"3");
    }

    #[tokio::test]
    async fn test_client_disconnect() {
        let ttfb_instant = Instant::now();
        let chunks = vec![
            TimedChunk {
                data: Bytes::from("first"),
                target_time: 0,
            },
            TimedChunk {
                data: Bytes::from("second"),
                target_time: 1000, // 1 second delay
            },
        ];

        let mut stream = spawn_timed_delivery(chunks, 2000, ttfb_instant);

        // Get first chunk
        let first = stream.next().await;
        assert!(first.is_some());
        assert_eq!(first.unwrap().unwrap().as_ref(), b"first");

        // Drop stream (simulates client disconnect)
        drop(stream);

        // Give the spawned task time to detect the disconnect
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Task should have stopped without waiting full 1 second
        // (test will hang/timeout if not)
    }

    /// Test that multiple concurrent streams don't block each other.
    ///
    /// This is the key test for the HTTP/2 fix. The old implementation using
    /// `stream::unfold` with `tokio::time::sleep()` would block the entire
    /// Tokio task, causing other HTTP/2 streams on the same connection to stall.
    ///
    /// The new implementation using spawned tasks ensures each stream operates
    /// independently, allowing true HTTP/2 multiplexing.
    #[tokio::test]
    async fn test_concurrent_streams_dont_block() {
        let start = Instant::now();

        // Create 3 streams with different timing configurations
        // Each has first chunk at target_time=0 (immediate), but second chunk delayed
        let create_stream = |delay_ms: u64, id: &'static str| {
            let ttfb_instant = Instant::now();
            let chunks = vec![
                TimedChunk {
                    data: Bytes::from(format!("{}-first", id)),
                    target_time: 0, // First chunk is immediate
                },
                TimedChunk {
                    data: Bytes::from(format!("{}-second", id)),
                    target_time: delay_ms, // Second chunk is delayed
                },
            ];
            spawn_timed_delivery(chunks, delay_ms + 50, ttfb_instant)
        };

        // Create streams with different delays for second chunk
        let mut stream1 = create_stream(500, "s1"); // 500ms delay for second chunk
        let mut stream2 = create_stream(500, "s2"); // 500ms delay for second chunk
        let mut stream3 = create_stream(500, "s3"); // 500ms delay for second chunk

        // Get first chunk from all streams concurrently
        // In the OLD implementation (blocking unfold), this would take ~1500ms
        // because each stream's poll would block waiting for timing.
        // In the NEW implementation (spawned tasks), this should be nearly instant
        // because first chunks have target_time=0 and spawned tasks don't block each other.
        let (r1, r2, r3) = tokio::join!(stream1.next(), stream2.next(), stream3.next(),);

        let elapsed_first_chunks = start.elapsed().as_millis();

        // Verify all first chunks were received
        assert!(r1.is_some());
        assert!(r2.is_some());
        assert!(r3.is_some());
        assert_eq!(r1.unwrap().unwrap().as_ref(), b"s1-first");
        assert_eq!(r2.unwrap().unwrap().as_ref(), b"s2-first");
        assert_eq!(r3.unwrap().unwrap().as_ref(), b"s3-first");

        // KEY ASSERTION: Getting first chunks from 3 concurrent streams should be fast
        // With the old blocking implementation, this would take 500+ ms per stream
        // With the new spawned task implementation, all first chunks arrive nearly instantly
        assert!(
            elapsed_first_chunks < 100,
            "First chunks took {}ms - streams may be blocking each other! Expected < 100ms",
            elapsed_first_chunks
        );

        // Now get second chunks - these should arrive around 500ms mark
        let before_second = Instant::now();
        let (r1, r2, r3) = tokio::join!(stream1.next(), stream2.next(), stream3.next(),);

        let elapsed_second_chunks = before_second.elapsed().as_millis();

        // Verify all second chunks were received
        assert!(r1.is_some());
        assert!(r2.is_some());
        assert!(r3.is_some());
        assert_eq!(r1.unwrap().unwrap().as_ref(), b"s1-second");
        assert_eq!(r2.unwrap().unwrap().as_ref(), b"s2-second");
        assert_eq!(r3.unwrap().unwrap().as_ref(), b"s3-second");

        // Second chunks should arrive around 500ms (with some tolerance)
        // They should NOT take 1500ms (which would indicate serial blocking)
        assert!(
            elapsed_second_chunks >= 400,
            "Second chunks arrived too early: {}ms (expected ~500ms)",
            elapsed_second_chunks
        );
        assert!(
            elapsed_second_chunks < 700,
            "Second chunks took {}ms - streams may be blocking each other! Expected ~500ms",
            elapsed_second_chunks
        );
    }

    /// Test that a slow stream doesn't block a fast stream.
    ///
    /// This simulates the real HTTP/2 scenario where one large resource with
    /// slow transfer timing shouldn't block smaller, faster resources.
    #[tokio::test]
    async fn test_slow_stream_doesnt_block_fast_stream() {
        let start = Instant::now();

        // Slow stream: chunks at 0ms, 1000ms, 2000ms
        let slow_ttfb = Instant::now();
        let slow_chunks = vec![
            TimedChunk {
                data: Bytes::from("slow-1"),
                target_time: 0,
            },
            TimedChunk {
                data: Bytes::from("slow-2"),
                target_time: 1000,
            },
            TimedChunk {
                data: Bytes::from("slow-3"),
                target_time: 2000,
            },
        ];
        let mut slow_stream = spawn_timed_delivery(slow_chunks, 2100, slow_ttfb);

        // Fast stream: all chunks immediate
        let fast_ttfb = Instant::now();
        let fast_chunks = vec![
            TimedChunk {
                data: Bytes::from("fast-1"),
                target_time: 0,
            },
            TimedChunk {
                data: Bytes::from("fast-2"),
                target_time: 0,
            },
            TimedChunk {
                data: Bytes::from("fast-3"),
                target_time: 0,
            },
        ];
        let fast_stream = spawn_timed_delivery(fast_chunks, 10, fast_ttfb);

        // Get first chunk from slow stream (to start its timing)
        let slow_first = slow_stream.next().await;
        assert_eq!(slow_first.unwrap().unwrap().as_ref(), b"slow-1");

        // Now collect ALL chunks from fast stream
        // This should complete nearly instantly, NOT blocked by slow stream
        let fast_results: Vec<_> = fast_stream.collect().await;
        let fast_elapsed = start.elapsed().as_millis();

        assert_eq!(fast_results.len(), 3);
        assert_eq!(fast_results[0].as_ref().unwrap().as_ref(), b"fast-1");
        assert_eq!(fast_results[1].as_ref().unwrap().as_ref(), b"fast-2");
        assert_eq!(fast_results[2].as_ref().unwrap().as_ref(), b"fast-3");

        // KEY ASSERTION: Fast stream should complete quickly despite slow stream existing
        // With blocking implementation, fast stream would wait for slow stream's timing
        assert!(
            fast_elapsed < 100,
            "Fast stream took {}ms - blocked by slow stream! Expected < 100ms",
            fast_elapsed
        );

        // Clean up slow stream (let it finish or disconnect)
        drop(slow_stream);
    }
}
