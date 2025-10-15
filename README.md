# `compio-watch`

A high-performance, asynchronous runtime-agnostic alternative to `tokio::sync::watch`. It uses `parking_lot::RwLock` and `event-listener` as its underlying implementation and provides a `tokio`-compatible API.

## Cross-Runtime Compatibility

This library is completely runtime-agnostic and does not rely on any specific scheduler. It can be used seamlessly in various asynchronous environments such as `smol`, and others(non-tested).

## Performance Advantages

In most scenarios, its measured performance is superior to `tokio`'s native implementation.

| Number of Subscribers | `compio-watch` | `tokio::sync::watch` |     Advantage     |
| :-------------------- | :------------: | :------------------: | :---------------: |
| 1                     |  **84.9 µs**   |       213.8 µs       |  **2.3x faster**  |
| 4                     |  **350.6 µs**  |       388.6 µs       |  **11% faster**   |
| 16                    |    1.52 ms     |     **1.14 ms**      | `tokio` is faster |
| 64                    |  **2.17 ms**   |       2.67 ms        |  **23% faster**   |

_(Benchmark platform CPU: Intel Core i5-11300H. More details can be found in the code repository.)_
