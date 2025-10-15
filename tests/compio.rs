//! Tests for compio-watch integration with the compio runtime.
//!
//! These tests verify that the watch channel works correctly with the compio
//! async runtime, including basic send/receive operations, error handling,
//! and channel lifecycle management.

use compio_watch::{channel, error::RecvError};
use std::time::Duration;

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
/// - The `changed()` method returns `RecvError::Failed` when channel is closed
#[compio::test]
async fn sender_dropped() {
    let (tx, rx) = channel("live");
    drop(tx);
    let result = rx.changed().await;
    assert!(matches!(result, Err(RecvError::Failed)));
}

/// Tests behavior when all receivers are dropped.
///
/// This test verifies that:
/// - The `is_closed()` method correctly reports channel status
/// - The `closed()` async method completes when last receiver is dropped
/// - Sending to a closed channel returns an error
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
