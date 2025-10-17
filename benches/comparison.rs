//! Benchmark comparison between see and tokio::sync::watch channels.
//!
//! This benchmark measures performance characteristics including:
//! - Single Producer Single Consumer (SPSC) throughput
//! - Multiple Producer Single Consumer (MPSC) throughput
//! - Send-only overhead (no receivers)
//! - Receive-only overhead (borrow operations)
//!
//! The benchmarks test various receiver counts to understand scaling behavior
//! and compare performance across different workloads.

use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use see::{Sender, channel};
use tokio::runtime::Runtime;

/// Number of receivers to test in SPSC/MPSC benchmarks.
/// Tests single receiver (SPSC) and multiple receivers (MPSC) scenarios.
const RECEIVER_COUNTS: &[usize] = &[1, 4, 16, 64];

/// Size of the payload being sent through the channel (1KB).
/// This simulates realistic data transfer scenarios.
const PAYLOAD_SIZE: usize = 1024; // 1KB

/// Number of operations to perform in each benchmark iteration.
/// This provides consistent measurement across different receiver counts.
const OPERATIONS_PER_BENCH: u64 = 1000;
/// Benchmarks SPSC (Single Producer Single Consumer) and MPSC (Multiple
/// Producer Single Consumer) throughput for both tokio-watch and see channels.
///
/// This benchmark measures how well each implementation scales with increasing
/// numbers of receivers, simulating real-world scenarios where multiple
/// components need to observe the same state changes.
fn benchmark_spsc_mpsc(c: &mut Criterion) {
    let mut group = c.benchmark_group("SPSC/MPSC Throughput");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(20));
    group.throughput(Throughput::Elements(OPERATIONS_PER_BENCH));

    for &n_receivers in RECEIVER_COUNTS {
        // Benchmark tokio's native watch channel implementation
        // This serves as the baseline for comparison
        // Benchmark see implementation
        // Note: see uses a different API pattern where receivers
        // are created from the sender via `subscribe()` method
        group.bench_with_input(
            BenchmarkId::new("tokio_watch", n_receivers),
            &n_receivers,
            |b, &n| {
                let rt = Runtime::new().unwrap();
                b.to_async(rt).iter_with_setup(
                    || {
                        let (tx, rx) = tokio::sync::watch::channel([0u8; PAYLOAD_SIZE]);
                        let receivers: Vec<_> = (0..n).map(|_| rx.clone()).collect();
                        let completion_counter = Arc::new(AtomicUsize::new(n));
                        (tx, receivers, completion_counter)
                    },
                    |(tx, mut receivers, counter)| async move {
                        let sender_counter = counter.clone();
                        let sender_task = tokio::spawn(async move {
                            for i in 0..OPERATIONS_PER_BENCH {
                                if tx.send([i as u8; PAYLOAD_SIZE]).is_err() {
                                    break;
                                }
                            }
                            while sender_counter.load(Ordering::Acquire) > 0 {
                                tokio::task::yield_now().await;
                            }
                        });

                        let mut receiver_tasks = Vec::new();
                        for mut rx in receivers.drain(..) {
                            let receiver_counter = counter.clone();
                            receiver_tasks.push(tokio::spawn(async move {
                                while rx.changed().await.is_ok() {
                                    if *rx.borrow()
                                        == [(OPERATIONS_PER_BENCH - 1) as u8; PAYLOAD_SIZE]
                                    {
                                        receiver_counter.fetch_sub(1, Ordering::Release);
                                        break;
                                    }
                                }
                            }));
                        }

                        sender_task.await.unwrap();
                        for task in receiver_tasks {
                            task.await.unwrap();
                        }
                    },
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("see", n_receivers),
            &n_receivers,
            |b, &n| {
                let rt = Runtime::new().unwrap();
                b.to_async(rt).iter_with_setup(
                    || {
                        let (tx, rx) = channel([0u8; PAYLOAD_SIZE]);
                        let mut receivers: Vec<_> = (0..n - 1).map(|_| tx.subscribe()).collect();
                        receivers.push(rx);
                        let completion_counter = Arc::new(AtomicUsize::new(n));
                        (tx, receivers, completion_counter)
                    },
                    |(tx, mut receivers, counter)| async move {
                        let sender_counter = counter.clone();
                        let sender_task = tokio::spawn(async move {
                            for i in 0..OPERATIONS_PER_BENCH {
                                if tx.send([i as u8; PAYLOAD_SIZE]).is_err() {
                                    break;
                                }
                            }
                            while sender_counter.load(Ordering::Acquire) > 0 {
                                tokio::task::yield_now().await;
                            }
                        });

                        let mut receiver_tasks = Vec::new();
                        for mut rx in receivers.drain(..) {
                            let receiver_counter = counter.clone();
                            receiver_tasks.push(tokio::spawn(async move {
                                while rx.changed().await.is_ok() {
                                    if *rx.borrow_and_update()
                                        == [(OPERATIONS_PER_BENCH - 1) as u8; PAYLOAD_SIZE]
                                    {
                                        receiver_counter.fetch_sub(1, Ordering::Release);
                                        break;
                                    }
                                }
                            }));
                        }

                        sender_task.await.unwrap();
                        for task in receiver_tasks {
                            task.await.unwrap();
                        }
                    },
                );
            },
        );
    }
    group.finish();
}

/// Benchmarks the overhead of send operations when there are no active
/// receivers.
///
/// This measures the raw performance of the send operation without
/// the cost of notifying receivers, showing the minimal overhead
/// of each implementation.
fn benchmark_send_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Send Only Overhead");
    group.throughput(Throughput::Elements(OPERATIONS_PER_BENCH));
    group.bench_function("tokio_watch", |b| {
        let (tx, _) = tokio::sync::watch::channel([0u8; PAYLOAD_SIZE]);
        b.iter(|| {
            for i in 0..OPERATIONS_PER_BENCH {
                let _ = tx.send([(i % 256) as u8; PAYLOAD_SIZE]);
            }
        });
    });
    group.bench_function("see", |b| {
        let tx = Sender::new([0u8; PAYLOAD_SIZE]);
        b.iter(|| {
            for i in 0..OPERATIONS_PER_BENCH {
                let _ = tx.send([(i % 256) as u8; PAYLOAD_SIZE]);
            }
        });
    });
    group.finish();
}

/// Benchmarks the overhead of receive (borrow) operations.
///
/// This measures the cost of acquiring a read guard to access
/// the current value, which is a common operation in watch channel usage.
fn benchmark_recv_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("Receive (Borrow) Only Overhead");
    group.throughput(Throughput::Elements(1));
    let (_tx, rx) = tokio::sync::watch::channel([0u8; PAYLOAD_SIZE]);
    group.bench_function("tokio_watch", |b| {
        b.iter(|| {
            let _guard = rx.borrow();
            black_box(_guard);
        });
    });
    let (_tx, rx_my) = channel([0u8; PAYLOAD_SIZE]);
    group.bench_function("see", |b| {
        b.iter(|| {
            let _guard = rx_my.borrow();
            black_box(_guard);
        });
    });
    group.finish();
}

// Register all benchmark functions with criterion
// This creates a benchmark group that will run all the defined benchmarks
criterion_group!(
    benches,
    benchmark_spsc_mpsc,
    benchmark_send_only,
    benchmark_recv_only
);
criterion_main!(benches);
