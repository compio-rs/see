//! Tests for see integration with the compio runtime.
//!
//! These tests verify that the watch channel works correctly with the compio
//! async runtime, including basic send/receive operations, error handling,
//! and channel lifecycle management.

use std::time::Duration;

#[cfg(feature = "stream")]
use futures_util::StreamExt;
use see::{error::RecvError, sync::channel};

/// Tests basic send and receive functionality with compio runtime.
///
/// This test verifies that:
/// - Initial value is correctly set
/// - Sender can send new values asynchronously
/// - Receiver can detect changes and read updated values
/// - The `has_changed()` method correctly reports value changes
#[compio::test]
async fn basic_send_recv() {
    let (tx, mut rx) = channel(0);
    let _keep_alive = tx.clone();
    assert_eq!(*rx.borrow(), 0);
    let sender_handle = compio::runtime::spawn(async move {
        compio::time::sleep(Duration::from_millis(20)).await;
        assert!(tx.send(42).is_ok());
    });
    assert!(rx.changed().await.is_ok());
    let guard = rx.borrow_and_update();
    assert!(guard.has_changed());
    assert_eq!(*guard, 42);
    sender_handle.await.unwrap();
}

/// Tests behavior when the sender is dropped.
///
/// This test verifies that:
/// - When all senders are dropped, receivers get appropriate errors
/// - The `changed()` method returns `RecvError::ChannelClosed` when channel is
///   closed
#[compio::test]
async fn sender_dropped() {
    let (tx, rx) = channel("live");
    drop(tx);
    let result = rx.changed().await;
    assert!(matches!(result, Err(RecvError::ChannelClosed)));
}

/// Tests behavior when all receivers are dropped.
///
/// This test verifies that:
/// - The `is_closed()` method correctly reports channel status
/// - Sending fails when all receivers are dropped
/// - The `closed()` method properly awaits channel closure
#[compio::test]
async fn all_receivers_dropped() {
    let (tx, rx) = channel(100);
    assert!(!tx.is_closed());
    let tx_clone = tx.clone();
    let closed_handle = compio::runtime::spawn(async move {
        tx_clone.closed().await;
    });
    drop(rx);
    closed_handle.await.unwrap();
    assert!(tx.is_closed());
    assert!(tx.send(200).is_err());
}

/// Tests stream functionality for sync version.
///
/// This test verifies that:
/// - Stream correctly yields initial value
/// - Stream correctly yields updated values
/// - Stream properly terminates when sender is dropped
#[compio::test]
#[cfg(feature = "stream")]
async fn sync_stream_basic() {
    let (tx, rx) = channel(10);
    let mut stream = rx.into_stream();

    // First value from stream should be initial value
    let value = stream.next().await;
    assert_eq!(value, Some(10));

    // Send a new value
    tx.send(20).unwrap();

    // Stream should yield the new value
    let value = stream.next().await;
    assert_eq!(value, Some(20));

    // Drop sender
    drop(tx);

    // Next call should yield None to indicate stream end
    let value = stream.next().await;
    assert_eq!(value, None);
}

/// Tests stream from_changes functionality for sync version.
///
/// This test verifies that:
/// - Stream correctly waits for changes before yielding values
/// - Stream properly terminates when sender is dropped
#[compio::test]
#[cfg(feature = "stream")]
async fn sync_stream_from_changes() {
    let (tx, rx) = channel("initial");
    let mut stream = rx.into_stream();

    // Send a new value immediately
    tx.send("updated").unwrap();

    // Stream should yield the updated value
    let value = stream.next().await;
    assert_eq!(value, Some("updated"));

    // Drop sender
    drop(tx);

    // Next call should yield None to indicate stream end
    let value = stream.next().await;
    assert_eq!(value, None);
}
