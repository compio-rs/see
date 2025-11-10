//! # `See`
//!
//! A high-performance, asynchronous runtime-agnostic alternative to
//! `tokio::sync::watch`.
//!
//! This library provides unsync and sync watch channel, both offering a
//! `tokio`-compatible API while being completely runtime-agnostic.
//!
//! # Features
//!
//! - **Runtime-agnostic**: Works with any async runtime (tokio, smol, compio,
//!   etc.)
//! - **High performance**: Outperforms tokio's native implementation in most
//!   scenarios
//! - **Optional Thread-safe**: Both synchronized and unsynchronized versions
//!   available
//!
//! # Usage
//!
//! ```
//! use see::sync::channel;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let (tx, mut rx) = channel("initial value");
//!
//! // Send a new value
//! tx.send("new value").unwrap();
//!
//! // Receive changes
//! rx.changed().await.unwrap();
//! let value = rx.borrow_and_update();
//! assert_eq!(*value, "new value");
//! # }
//! ```

#[cfg(feature = "stream")]
mod future_store;
mod state;

#[cfg(feature = "stream")]
pub mod stream;
pub mod sync;
pub mod unsync;

/// Error types for watch channel operations.
pub mod error {
    use thiserror::Error;

    /// Error returned when attempting to send a value through a closed channel.
    #[derive(Debug, Error)]
    pub enum SendError<T> {
        /// The channel is closed and the value cannot be sent.
        #[error("failed to send value: channel is closed (value: {0:?})")]
        ChannelClosed(T),
    }

    /// Error returned when attempting to receive from a closed channel.
    #[derive(Debug, Error)]
    pub enum RecvError {
        /// The channel is closed or all senders have been dropped.
        #[error("failed to receive: channel is closed or all senders have been dropped")]
        ChannelClosed,
    }
}
