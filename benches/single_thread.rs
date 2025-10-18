#![allow(unused)]
use std::{hint::black_box, rc::Rc};

use compio::runtime::{Runtime, spawn};
use criterion::{Criterion, async_executor::AsyncExecutor, criterion_group, criterion_main};
use see::*;

struct CompioExecutor(Runtime);

// A newtype wrapper around Rc<CompioExecutor>
#[derive(Clone)]
struct SharedCompioExecutor(Rc<CompioExecutor>);

impl SharedCompioExecutor {
    fn new() -> Self {
        Self(Rc::new(CompioExecutor(Runtime::new().unwrap())))
    }
}

// Now, we implement the trait for our local newtype.
impl AsyncExecutor for SharedCompioExecutor {
    fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.0.0.block_on(future)
    }
}

const MESSAGES_PER_ITER: u64 = 1_000;

// =================================================================
// SPSC Throughput Benchmarks
// =================================================================

fn bench_unsync_spsc(c: &mut Criterion) {
    // Use our new wrapper type.
    let executor = SharedCompioExecutor::new();

    c.bench_function("spsc_unsync", |b| {
        // Pass a clone of the wrapper to to_async.
        b.to_async(executor.clone()).iter(|| async {
            let (tx, mut rx) = unsync::channel(0);

            let send_task = spawn(async move {
                for i in 1..=MESSAGES_PER_ITER {
                    tx.send(i).unwrap();
                }
            });

            let recv_task = spawn(async move {
                while *rx.borrow_and_update() < MESSAGES_PER_ITER {
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
            });
            send_task.await.unwrap();
            recv_task.await.unwrap();
        });
    });
}

fn bench_sync_spsc(c: &mut Criterion) {
    let executor = SharedCompioExecutor::new();

    c.bench_function("spsc_sync", |b| {
        b.to_async(executor.clone()).iter(|| async {
            let (tx, mut rx) = sync::channel(0);

            let send_task = spawn(async move {
                for i in 1..=MESSAGES_PER_ITER {
                    tx.send(i).unwrap();
                }
            });

            let recv_task = spawn(async move {
                while *rx.borrow_and_update() < MESSAGES_PER_ITER {
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
            });

            send_task.await.unwrap();
            recv_task.await.unwrap();
        });
    });
}

// =================================================================
// Sync Borrow Benchmark (Unchanged)
// =================================================================

fn bench_borrow(c: &mut Criterion) {
    let mut group = c.benchmark_group("borrow");

    group.bench_function("unsync", |b| {
        let (_tx, rx) = unsync::channel(0);
        b.iter(|| {
            black_box(rx.borrow());
        })
    });

    group.bench_function("sync", |b| {
        let (_tx, rx) = sync::channel(0);
        b.iter(|| {
            black_box(rx.borrow());
        })
    });

    group.finish();
}

// =================================================================
// Full Cycle Latency Benchmark
// =================================================================

fn bench_full_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_cycle");
    let executor = SharedCompioExecutor::new();

    group.bench_function("unsync", |b| {
        b.to_async(executor.clone()).iter_with_setup(
            || unsync::channel(0),
            |(tx, mut rx)| async move {
                tx.send(1).unwrap();
                rx.changed().await.unwrap();
                let val = rx.borrow_and_update();
                black_box(val);
            },
        );
    });

    group.bench_function("sync", |b| {
        b.to_async(executor.clone()).iter_with_setup(
            || sync::channel(0),
            |(tx, mut rx)| async move {
                tx.send(1).unwrap();
                rx.changed().await.unwrap();
                let val = rx.borrow_and_update();
                black_box(val);
            },
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_unsync_spsc,
    bench_sync_spsc,
    bench_borrow,
    bench_full_cycle
);
criterion_main!(benches);
