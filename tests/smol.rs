//! Tests for see integration with the smol runtime.
//!
//! These tests verify that the watch channel works correctly with the smol
//! async runtime, including basic send/receive operations, error handling,
//! and channel lifecycle management.
use std::time::Duration;

use see::{channel, error::RecvError};

#[test]
fn basic_send_recv() {
    smol::block_on(async {
        let (tx, mut rx) = channel(0);
        assert_eq!(*rx.borrow(), 0);
        let _keep_alive = tx.clone();
        let sender_handle = smol::spawn(async move {
            smol::Timer::after(Duration::from_millis(20)).await;
            assert!(tx.send(42).is_ok());
        });
        assert!(rx.changed().await.is_ok());
        let guard = rx.borrow_and_update();
        assert!(guard.has_changed());
        assert_eq!(*guard, 42);
        sender_handle.await;
    });
}

#[test]
fn sender_dropped() {
    smol::block_on(async {
        let (tx, rx) = channel("live");
        drop(tx);
        let result = rx.changed().await;
        assert!(matches!(result, Err(RecvError::Failed)));
    });
}

#[test]
fn all_receivers_dropped() {
    smol::block_on(async {
        let (tx, rx) = channel(100);
        assert!(!tx.is_closed());
        let tx_clone = tx.clone();
        let closed_handle = smol::spawn(async move {
            tx_clone.closed().await;
        });
        drop(rx);
        closed_handle.await;
        assert!(tx.is_closed());
        assert!(tx.send(200).is_err());
    });
}
