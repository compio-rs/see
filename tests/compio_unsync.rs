use std::time::Duration;

use compio::time::sleep;
#[cfg(feature = "stream")]
use futures_util::StreamExt;
#[cfg(feature = "stream")]
use see::stream::unsync::UnsyncWatchStream;
use see::{
    error::{RecvError, SendError},
    unsync::*,
};

#[compio::test]
async fn test_initial_value() {
    let (_tx, mut rx) = channel("initial");
    assert_eq!(*rx.borrow_and_update(), "initial");
    assert!(!rx.borrow_and_update().has_changed());
}

#[compio::test]
async fn test_send_and_receive() {
    let (tx, mut rx) = channel("one");
    tx.send("two").unwrap();
    rx.changed().await.unwrap();
    let value = rx.borrow_and_update();
    assert_eq!(*value, "two");
    assert!(value.has_changed());
}

#[compio::test]
async fn test_send_replace() {
    let (tx, mut rx) = channel("one");
    let old = tx.send_replace("two");
    assert_eq!(old, "one");
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), "two");
}

#[compio::test]
async fn test_send_modify() {
    let (tx, mut rx) = channel(vec![1, 2]);
    tx.send_modify(|v| v.push(3));
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), vec![1, 2, 3]);
}

#[compio::test]
async fn test_send_if_modified() {
    let (tx, mut rx) = channel(10);

    let notified = tx.send_if_modified(|_v| false);
    assert!(!notified);
    assert!(!rx.has_changed().unwrap());

    let notified = tx.send_if_modified(|v| {
        *v = 20;
        true
    });
    assert!(notified);
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), 20);
}

#[compio::test]
async fn test_multiple_receivers() {
    let (tx, mut rx1) = channel("start");
    let mut rx2 = rx1.clone();

    tx.send("end").unwrap();

    rx1.changed().await.unwrap();
    assert_eq!(*rx1.borrow_and_update(), "end");

    rx2.changed().await.unwrap();
    assert_eq!(*rx2.borrow_and_update(), "end");
}

#[compio::test]
async fn test_multiple_senders() {
    let (tx1, mut rx) = channel(0);
    let tx2 = tx1.clone();

    assert_eq!(tx1.sender_count(), 2);
    tx1.send(1).unwrap();
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), 1);

    tx2.send(2).unwrap();
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), 2);

    drop(tx1);
    assert_eq!(tx2.sender_count(), 1);

    tx2.send(3).unwrap();
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), 3);
}

#[compio::test]
async fn test_has_changed() {
    let (tx, rx) = channel(1);
    assert!(!rx.has_changed().unwrap());

    tx.send(2).unwrap();
    assert!(rx.has_changed().unwrap());
}

#[test]
fn test_borrow_vs_borrow_and_update() {
    let (tx, mut rx) = channel("a");

    tx.send("b").unwrap();
    assert!(rx.has_changed().unwrap());

    {
        let value = rx.borrow();
        assert_eq!(*value, "b");
        assert!(value.has_changed());
    }
    assert!(rx.has_changed().unwrap());

    {
        let value = rx.borrow_and_update();
        assert_eq!(*value, "b");
        assert!(value.has_changed());
    }
    assert!(!rx.has_changed().unwrap());
}

#[test]
fn test_mark_changed_and_unchanged() {
    let (tx, mut rx) = channel(100);
    assert!(!rx.has_changed().unwrap());

    rx.mark_unchanged();
    assert!(!rx.has_changed().unwrap());

    rx.mark_changed();
    assert!(rx.has_changed().unwrap());

    rx.mark_unchanged();
    assert!(!rx.has_changed().unwrap());

    tx.send(200).unwrap();
    assert!(rx.has_changed().unwrap());
}

#[compio::test]
async fn test_sender_closed() {
    let (tx, rx) = channel("val");
    drop(tx);

    let result = rx.has_changed();
    assert!(matches!(result, Err(RecvError::ChannelClosed)));

    let changed_result = rx.changed().await;
    assert!(matches!(changed_result, Err(RecvError::ChannelClosed)));
}

#[compio::test]
async fn test_receiver_closed() {
    let (tx, rx) = channel(5);
    let closed_future = tx.closed();

    assert!(!tx.is_closed());
    drop(rx);

    assert!(tx.is_closed());

    closed_future.await;

    let send_result = tx.send(10);
    assert!(matches!(send_result, Err(SendError::ChannelClosed(10))));
}

#[compio::test]
async fn test_wait_for() {
    let (tx, mut rx) = channel(0);

    let wait_task = compio::runtime::spawn(async move {
        let guard = rx.wait_for(|&v| v == 5).await.unwrap();
        assert_eq!(*guard, 5);

        assert!(guard.has_changed());
    });

    for i in 1..=5 {
        sleep(Duration::from_millis(10)).await;
        tx.send(i).unwrap();
    }

    wait_task.await.unwrap();
}

#[compio::test]
async fn test_wait_for_closed() {
    let (tx, mut rx) = channel(0);

    let wait_task = compio::runtime::spawn(async move {
        let result = rx.wait_for(|&v| v == 5).await;
        assert!(matches!(result, Err(RecvError::ChannelClosed)));
    });

    tx.send(1).unwrap();
    drop(tx);

    wait_task.await.unwrap();
}

#[test]
fn test_counts() {
    let (tx, rx) = channel("data");
    assert_eq!(tx.sender_count(), 1);
    assert_eq!(tx.receiver_count(), 1);

    let tx2 = tx.clone();
    assert_eq!(tx.sender_count(), 2);
    assert_eq!(tx.receiver_count(), 1);

    let rx2 = rx.clone();
    assert_eq!(tx.sender_count(), 2);
    assert_eq!(tx.receiver_count(), 2);

    drop(tx2);
    assert_eq!(tx.sender_count(), 1);
    assert_eq!(tx.receiver_count(), 2);

    drop(rx2);
    assert_eq!(tx.sender_count(), 1);
    assert_eq!(tx.receiver_count(), 1);
}

#[test]
fn test_same_channel() {
    let (tx1, rx1) = channel(1);
    let (tx2, rx2) = channel(1);
    let tx1_clone = tx1.clone();
    let rx1_clone = rx1.clone();

    assert!(tx1.same_channel(&tx1_clone));
    assert!(rx1.same_channel(&rx1_clone));
    assert!(!tx1.same_channel(&tx2));
    assert!(!rx1.same_channel(&rx2));
}

#[test]
fn test_default() {
    let tx = Sender::<String>::default();
    let guard = tx.borrow();
    assert_eq!(*guard, "");
}

/// Tests stream functionality for unsync version.
///
/// This test verifies that:
/// - Stream correctly yields initial value
/// - Stream correctly yields updated values
/// - Stream properly terminates when sender is dropped
#[compio::test]
#[cfg(feature = "stream")]
async fn test_unsync_stream_basic() {
    let (tx, rx) = channel(10);
    let mut stream = UnsyncWatchStream::new(rx);

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

/// Tests stream from_changes functionality for unsync version.
///
/// This test verifies that:
/// - Stream correctly waits for changes before yielding values
/// - Stream properly terminates when sender is dropped
#[compio::test]
#[cfg(feature = "stream")]
async fn test_unsync_stream_from_changes() {
    let (tx, rx) = channel("initial");
    let mut stream = UnsyncWatchStream::from_changes(rx);

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
