//! Streaming implementations for watch channels.
//!
//! This module provides streaming capabilities for both synchronized and
//! unsynchronized watch channels. The streams yield values as they become
//! available in the watch channel.
//!
//! Two stream types are provided:
//! - [`sync::SyncWatchStream`] for synchronized watch channels
//! - [`unsync::UnsyncWatchStream`] for unsynchronized watch channels

use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll, ready},
};

use futures_util::Stream;

use crate::error::RecvError;

/// Streaming implementation for synchronized watch channels.
pub mod sync {
    use super::*;
    use crate::{future_store::sync::BoxedFutureStore, sync::Receiver};

    /// Creates a future that waits for changes on the receiver.
    async fn mk_fut<T: 'static + Clone + Send + Sync>(
        rx: Receiver<T>,
    ) -> (Result<(), RecvError>, Receiver<T>) {
        let result = rx.changed().await;
        (result, rx)
    }

    /// A stream that yields values from a synchronized watch channel.
    ///
    /// This stream will yield the current value of the channel when first
    /// polled, and then yield new values as they are sent to the channel.
    ///
    /// When the sender is dropped, the stream will yield `None` to indicate
    /// the stream has ended.
    pub struct SyncWatchStream<T> {
        /// The future store containing the pending operation.
        inner: BoxedFutureStore<'static, (Result<(), RecvError>, Receiver<T>)>,
    }

    impl<T: 'static + Clone + Send + Sync> SyncWatchStream<T> {
        /// Create a new stream from a receiver that yields the current value
        /// first.
        ///
        /// The stream will immediately yield the current value of the channel
        /// when first polled, and then wait for new values.
        ///
        /// # Example
        ///
        /// ```
        /// use futures_util::StreamExt;
        /// use see::{stream::sync::SyncWatchStream, sync::channel};
        ///
        /// # async fn doc() {
        /// let (tx, rx) = channel("hello");
        /// let mut stream = SyncWatchStream::new(rx);
        ///
        /// // First poll yields current value
        /// let value = stream.next().await;
        /// assert_eq!(value, Some("hello"));
        /// # }
        /// ```
        pub fn new(rx: Receiver<T>) -> Self {
            Self {
                inner: BoxedFutureStore::new(async move { (Ok(()), rx) }),
            }
        }

        /// Create a new stream from a receiver that waits for changes first.
        ///
        /// Unlike [`new`](Self::new), this stream will wait for the next change
        /// to the channel before yielding a value.
        ///
        /// # Example
        ///
        /// ```
        /// use futures_util::StreamExt;
        /// use see::{stream::sync::SyncWatchStream, sync::channel};
        ///
        /// # async fn doc() {
        /// let (tx, rx) = channel("hello");
        /// let mut stream = SyncWatchStream::from_changes(rx);
        ///
        /// // Update the value
        /// tx.send("world").unwrap();
        ///
        /// // First poll waits for and yields the new value
        /// let value = stream.next().await;
        /// assert_eq!(value, Some("world"));
        /// # }
        /// ```
        pub fn from_changes(rx: Receiver<T>) -> Self {
            Self {
                inner: BoxedFutureStore::new(mk_fut(rx)),
            }
        }
    }

    impl<T: Clone + 'static + Send + Sync> Stream for SyncWatchStream<T> {
        type Item = T;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let (result, mut rx) = ready!(self.inner.poll(cx));
            match result {
                Ok(_) => {
                    let received = (*rx.borrow_and_update()).clone();
                    self.inner.set(mk_fut(rx));
                    Poll::Ready(Some(received))
                }
                Err(_) => {
                    self.inner.set(mk_fut(rx));
                    Poll::Ready(None)
                }
            }
        }
    }

    impl<T> Unpin for SyncWatchStream<T> {}

    impl<T> fmt::Debug for SyncWatchStream<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("SyncWatchStream").finish()
        }
    }

    impl<T: 'static + Clone + Send + Sync> From<Receiver<T>> for SyncWatchStream<T> {
        fn from(recv: Receiver<T>) -> Self {
            Self::new(recv)
        }
    }
}

/// Streaming implementation for unsynchronized watch channels.
pub mod unsync {
    use super::*;
    use crate::{future_store::unsync::LocalBoxedFutureStore, unsync::Receiver};

    /// Creates a future that waits for changes on the receiver.
    async fn mk_fut<T: 'static + Clone>(rx: Receiver<T>) -> (Result<(), RecvError>, Receiver<T>) {
        let result = rx.changed().await;
        (result, rx)
    }

    /// A stream that yields values from an unsynchronized watch channel.
    ///
    /// This stream will yield the current value of the channel when first
    /// polled, and then yield new values as they are sent to the channel.
    ///
    /// When the sender is dropped, the stream will yield `None` to indicate
    /// the stream has ended.
    pub struct UnsyncWatchStream<T> {
        /// The future store containing the pending operation.
        inner: LocalBoxedFutureStore<'static, (Result<(), RecvError>, Receiver<T>)>,
    }

    impl<T: 'static + Clone> UnsyncWatchStream<T> {
        /// Create a new stream from a receiver that yields the current value
        /// first.
        ///
        /// The stream will immediately yield the current value of the channel
        /// when first polled, and then wait for new values.
        ///
        /// # Example
        ///
        /// ```
        /// use futures_util::StreamExt;
        /// use see::{stream::unsync::UnsyncWatchStream, unsync::channel};
        ///
        /// # async fn doc() {
        /// let (tx, rx) = channel("hello");
        /// let mut stream = UnsyncWatchStream::new(rx);
        ///
        /// // First poll yields current value
        /// let value = stream.next().await;
        /// assert_eq!(value, Some("hello"));
        /// # }
        /// ```
        pub fn new(rx: Receiver<T>) -> Self {
            Self {
                inner: LocalBoxedFutureStore::new(async move { (Ok(()), rx) }),
            }
        }

        /// Create a new stream from a receiver that waits for changes first.
        ///
        /// Unlike [`new`](Self::new), this stream will wait for the next change
        /// to the channel before yielding a value.
        ///
        /// # Example
        ///
        /// ```
        /// use futures_util::StreamExt;
        /// use see::{stream::unsync::UnsyncWatchStream, unsync::channel};
        ///
        /// # async fn doc() {
        /// let (tx, rx) = channel("hello");
        /// let mut stream = UnsyncWatchStream::from_changes(rx);
        ///
        /// // Update the value
        /// tx.send("world").unwrap();
        ///
        /// // First poll waits for and yields the new value
        /// let value = stream.next().await;
        /// assert_eq!(value, Some("world"));
        /// # }
        /// ```
        pub fn from_changes(rx: Receiver<T>) -> Self {
            Self {
                inner: LocalBoxedFutureStore::new(mk_fut(rx)),
            }
        }
    }

    impl<T: Clone + 'static> Stream for UnsyncWatchStream<T> {
        type Item = T;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let (result, mut rx) = ready!(self.inner.poll(cx));
            match result {
                Ok(_) => {
                    let received = (*rx.borrow_and_update()).clone();
                    self.inner.set(mk_fut(rx));
                    Poll::Ready(Some(received))
                }
                Err(_) => {
                    self.inner.set(mk_fut(rx));
                    Poll::Ready(None)
                }
            }
        }
    }

    impl<T> Unpin for UnsyncWatchStream<T> {}

    impl<T> fmt::Debug for UnsyncWatchStream<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("UnsyncWatchStream").finish()
        }
    }

    impl<T: 'static + Clone> From<Receiver<T>> for UnsyncWatchStream<T> {
        fn from(recv: Receiver<T>) -> Self {
            Self::new(recv)
        }
    }
}
