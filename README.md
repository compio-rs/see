# `See`

A high-performance, asynchronous runtime-agnostic alternative to `tokio::sync::watch`. 

## Cross-Runtime Compatibility

This library is completely runtime-agnostic and does not rely on any specific scheduler. It can be used seamlessly in various asynchronous environments such as `smol`, and others(non-tested).

## Performance Advantages (Sync Version)

In most scenarios, its measured performance is superior to `tokio`'s native implementation.

| Number of Subscribers | `see::sync` | `tokio::sync::watch` |     Advantage     |
| :-------------------- | :------------: | :------------------: | :---------------: |
| 1                     |  **84.9 µs**   |       213.8 µs       |  **2.3x faster**  |
| 4                     |  **350.6 µs**  |       388.6 µs       |  **11% faster**   |
| 16                    |    1.52 ms     |     **1.14 ms**      | `tokio` is faster |
| 64                    |  **2.17 ms**   |       2.67 ms        |  **23% faster**   |

_(Benchmark platform CPU: Intel Core i5-11300H. More details can be found in the code repository.)_

## `Sync` and `Unsync` Implementations

The library offers two versions of channels, `sync` and `unsync`, with significant performance differences and different underlying components:

### `see::sync`

The synchronized version uses the following components for thread-safe operations:
* [`parking_lot::RwLock`](https://crates.io/crates/parking_lot)
* [`event-listener`](https://crates.io/crates/event-listener)
* `std::sync::atomic`

This version is meant for multi-threaded scenarios where the channel needs to be sent across thread boundaries.

### `see::unsync`

The unsynchronized version uses lighter-weight components for single-threaded use cases:
* `std::cell::RefCell`
* `std::cell::Cell`
* [`local-event`](https://crates.io/crates/local-event)
* `std::rc::Rc`

This version is significantly faster in single-threaded scenarios because it avoids synchronization overhead.

Benchmarks, run on a single thread, highlight the concrete trade-offs:

*   **Throughput (`spsc`):** The `unsync` channel can process a high volume of messages approximately **8 times faster** than its `sync` counterpart (~4.8 µs vs. ~38.6 µs for 1,000 messages).
*   **Access & Latency (`borrow`, `full_cycle`):** For individual operations like borrowing the current value or completing a single send-receive cycle, the `unsync` version demonstrates a latency that is about **4 times lower** (~3.5 ns vs. ~14.3 ns for a borrow operation).

This performance gap stems from the fundamental design of each module. The `sync` version must use atomic operations and internal locking to guarantee thread safety (`Send + Sync`), which incurs a substantial overhead even in uncontested scenarios. In contrast, the `unsync` version relies on standard memory operations, making it drastically more efficient.

Therefore, the choice is clear:

*   **Use `see::unsync`** for maximum performance when all related asynchronous tasks are guaranteed to run on a single thread. This is the ideal choice for single-threaded runtimes like `compio`.
*   **Use `see::sync`** only when you explicitly need to share the channel between multiple OS threads and are willing to accept the associated performance cost.
