//! Unsynchronized watch channel implementation.
//!
//! This module provides a watch channel implementation that is not
//! thread-safe and is intended for use in single-threaded contexts.
//!
//! If you need thread-safety, consider using the [`sync`](crate::sync) module.

use std::{
    borrow,
    cell::{Cell, Ref, RefCell},
    mem,
    ops::Deref,
    rc::Rc,
};

use local_event::Event;

use crate::{
    error::{RecvError, SendError},
    state::{StateSnapshot, Version},
};

/// State container that combines version and closed status.
///
/// This uses a single `Cell<usize>` to store both the version number
/// and the closed status, reducing memory overhead and improving
/// cache locality.
#[derive(Debug)]
struct State(Cell<usize>);

impl State {
    /// Creates a new state with the initial version.
    fn new() -> Self {
        State(Cell::new(Version::INITIAL.inner()))
    }

    /// Loads the current state snapshot with acquire ordering.
    ///
    /// Acquire ordering ensures subsequent reads see the state changes
    /// that happened before this load.
    #[inline]
    fn load(&self) -> StateSnapshot {
        StateSnapshot::from_usize(self.0.get())
    }

    /// Increments the version number.
    #[inline]
    fn increment_version(&self) {
        self.0.update(|x| x + Version::STEP);
    }

    /// Marks the channel as closed by setting the closed bit.
    fn set_closed(&self) {
        self.0.update(|x| x | StateSnapshot::CLOSED_BIT);
    }
}

/// Shared state between all senders and receivers of a watch channel.
///
/// Contains the actual value and counters for tracking active senders and
/// receivers.
#[derive(Debug)]
struct Shared<T> {
    /// Combined version and closed state
    state: State,
    /// Number of active senders
    tx_count: Cell<usize>,
    /// Number of active receivers
    rx_count: Cell<usize>,
    /// Event notified when the value changes
    changed: Event,
    /// Event notified when the channel closes (all receivers dropped)
    closed: Event,
    /// The actual value protected by a read-write lock
    value: RefCell<T>,
}

impl<T> Shared<T> {
    /// Creates a new shared state with the initial value.
    fn new(init: T) -> Self {
        Self {
            value: init.into(),
            state: State::new(),
            tx_count: Cell::new(0),
            rx_count: Cell::new(0),
            changed: Event::new(),
            closed: Event::new(),
        }
    }

    /// Returns the current number of active senders.
    #[inline]
    fn tx_count(&self) -> usize {
        self.tx_count.get()
    }

    /// Returns the current number of active receivers.
    #[inline]
    fn rx_count(&self) -> usize {
        self.rx_count.get()
    }
}

/// The sending half of the watch channel.
///
/// Used to send new values to all connected receivers.
/// Multiple senders can be created by cloning an existing sender.
#[derive(Debug)]
pub struct Sender<T> {
    shared: Rc<Shared<T>>,
}

impl<T> Sender<T> {
    /// Creates a sender from an existing shared state.
    /// Internal use only - increments the sender count.
    fn from_shared(shared: Rc<Shared<T>>) -> Self {
        shared.tx_count.update(|x| x + 1);
        Self { shared }
    }

    /// Creates a new watch channel with the given initial value.
    #[must_use]
    pub fn new(init: T) -> Self {
        let shared = Rc::new(Shared::new(init));
        shared.tx_count.update(|x| x + 1);
        Self { shared }
    }

    /// Conditionally notifies receivers of value changes.
    ///
    /// Calls `modify` with mutable access to value. Modifications happen
    /// regardless of return value. If `modify` returns `true`, receivers
    /// are notified of changes. If `false`, changes are kept but not notified.
    ///
    /// Returns `true` if receivers were notified, `false` otherwise.
    #[must_use]
    pub fn send_if_modified<F>(&self, modify: F) -> bool
    where
        F: FnOnce(&mut T) -> bool,
    {
        let mut guard = self.shared.value.borrow_mut();
        if !modify(&mut guard) {
            return false;
        }
        self.shared.state.increment_version();
        drop(guard);
        // Notify all waiting receivers of the change
        // usize::MAX means notify all listeners, not just one
        self.shared.changed.notify(usize::MAX);
        true
    }

    /// Modifies the value in-place and notifies all receivers.
    ///
    /// The modification function is called with a mutable reference
    /// to the current value, and all receivers are notified of the change.
    pub fn send_modify<F>(&self, modify: F)
    where
        F: FnOnce(&mut T),
    {
        let _ = self.send_if_modified(|value| {
            modify(value);
            true
        });
    }

    /// Replaces the current value with a new one and returns the old value.
    ///
    /// This is equivalent to swapping the values and notifying receivers
    /// of the change.
    #[must_use]
    pub fn send_replace(&self, mut value: T) -> T {
        self.send_modify(|old| mem::swap(old, &mut value));
        value
    }

    /// Sends a new value to all receivers.
    ///
    /// # Errors
    /// Returns `SendError::Failed` if the channel is closed (all receivers
    /// dropped).
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        if self.is_closed() {
            return Err(SendError::ChannelClosed(value));
        }
        let _ = self.send_replace(value);
        Ok(())
    }

    /// Returns a read-only guard to the current value.
    ///
    /// The guard implements `Deref<Target = T>` and can be used to
    /// access the current value without updating the receiver's version.
    #[must_use]
    pub fn borrow(&self) -> Guard<'_, T> {
        let inner = self.shared.value.borrow();
        // The sender/producer always sees the current version
        let has_changed = false;
        Guard { inner, has_changed }
    }

    /// Returns the number of active senders.
    #[must_use]
    pub fn sender_count(&self) -> usize {
        self.shared.tx_count()
    }

    /// Returns the number of active receivers.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.shared.rx_count()
    }

    /// Returns `true` if the channel is closed (no receivers remain).
    #[must_use]
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.receiver_count() == 0
    }

    /// Waits for all receivers to be dropped.
    ///
    /// This async function completes when the channel is closed,
    /// i.e., when the last receiver is dropped.
    pub async fn closed(&self) {
        if self.is_closed() {
            return;
        }
        let listener = self.shared.closed.listen();
        // Double-check pattern: avoid waiting if channel closed after listener creation
        if self.is_closed() {
            return;
        }
        listener.await;
        debug_assert!(self.is_closed())
    }

    /// Checks if two senders belong to the same channel.
    #[must_use]
    pub fn same_channel(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared)
    }

    /// Creates a new receiver that will receive value updates.
    ///
    /// The new receiver starts with the current version of the value
    /// and will be notified of subsequent changes.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<T> {
        let shared = self.shared.clone();
        shared.rx_count.update(|x| x + 1);
        let version = shared.state.load().version();
        // The CLOSED bit in the state tracks only whether the sender is
        // dropped, so we do not need to unset it if this reopens the channel.
        Receiver { version, shared }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.tx_count.update(|x| x + 1);
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared.tx_count.update(|x| x - 1);

        if self.shared.tx_count.get() == 0 {
            self.shared.state.set_closed();
            // Notify all waiting receivers that channel is closed
            // usize::MAX means notify all listeners, not just one
            self.shared.changed.notify(usize::MAX);
        }
    }
}

impl<T: Default> Default for Sender<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// A read-only guard that provides access to the current value.
///
/// This guard implements `Deref<Target = T>` and can be used to
/// access the value without consuming it. The guard also tracks
/// whether the value has changed since the last access.
///
/// # Lifetime Note
/// The guard holds a read lock on the underlying value. While the guard
/// is alive, no writes can occur. Keep guard lifetimes short to avoid
/// blocking senders.
#[derive(Debug)]
pub struct Guard<'a, T> {
    inner: Ref<'a, T>,
    has_changed: bool,
}

impl<T> Guard<'_, T> {
    /// Returns `true` if the value has changed since the last access.
    ///
    /// For senders, this always returns `false` since senders always
    /// see the current version.
    pub fn has_changed(&self) -> bool {
        self.has_changed
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<T> AsRef<T> for Guard<'_, T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T> borrow::Borrow<T> for Guard<'_, T> {
    #[inline]
    fn borrow(&self) -> &T {
        self
    }
}

/// The receiving half of the watch channel.
///
/// Used to receive value updates from the sender. Multiple receivers
/// can be created by cloning an existing receiver or using
/// `Sender::subscribe()`.
#[derive(Debug)]
pub struct Receiver<T> {
    shared: Rc<Shared<T>>,
    version: Version,
}

impl<T> Receiver<T> {
    /// Returns a read-only guard to the current value without updating the
    /// version.
    ///
    /// This allows inspecting the current value without marking it as "seen".
    /// Subsequent calls to `has_changed()` will still report changes.
    #[must_use]
    pub fn borrow(&self) -> Guard<'_, T> {
        let inner = self.shared.value.borrow();
        // After obtaining a read-lock no concurrent writes could occur
        // and the loaded version matches that of the borrowed reference.
        let new_version = self.shared.state.load().version();
        let has_changed = self.version != new_version;
        Guard { inner, has_changed }
    }

    /// Returns a read-only guard to the current value and updates the version.
    ///
    /// This marks the current value as "seen", so subsequent calls to
    /// `has_changed()` will only report new changes.
    #[must_use]
    pub fn borrow_and_update(&mut self) -> Guard<'_, T> {
        let inner = self.shared.value.borrow();
        let new_version = self.shared.state.load().version();
        let has_changed = self.version != new_version;
        self.version = new_version;
        Guard { inner, has_changed }
    }

    /// Checks if the value has changed since the last access.
    ///
    /// # Errors
    /// Returns `RecvError::Failed` if the channel is closed.
    pub fn has_changed(&self) -> Result<bool, RecvError> {
        let state = self.shared.state.load();
        if state.is_closed() {
            // All senders have dropped.
            return Err(RecvError::ChannelClosed);
        }
        let new_version = state.version();
        Ok(self.version != new_version)
    }

    /// Forces the receiver to detect a change on the next access.
    ///
    /// This can be useful for implementing custom change detection logic
    /// or for forcing a re-evaluation of the current value.
    pub fn mark_changed(&mut self) {
        self.version.decrement();
    }

    /// Marks the current value as seen, preventing change detection until the
    /// next update.
    pub fn mark_unchanged(&mut self) {
        let current_version = self.shared.state.load().version();
        self.version = current_version;
    }

    /// Checks if two receivers belong to the same channel.
    pub fn same_channel(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared)
    }

    /// Internal helper to check for changes without error handling.
    ///
    /// Returns `None` if the channel is closed, `Some(true)` if changed,
    /// `Some(false)` if unchanged.
    #[inline]
    fn load_change(&self) -> Option<bool> {
        let new_state = self.shared.state.load();
        if new_state.is_closed() {
            return None;
        }
        if new_state.version() != self.version {
            return Some(true);
        }
        Some(false)
    }

    /// Waits for the value to change.
    ///
    /// # Errors
    /// Returns `RecvError::Failed` if the channel is closed.
    pub async fn changed(&self) -> Result<(), RecvError> {
        // First check: avoid creating listener if change already happened
        if self.load_change().ok_or(RecvError::ChannelClosed)? {
            return Ok(());
        }
        let listener = self.shared.changed.listen();
        // Second check: avoid waiting if change happened after first check
        if self.load_change().ok_or(RecvError::ChannelClosed)? {
            return Ok(());
        }
        listener.await;
        // Final check after await to handle spurious wakeups
        self.load_change().ok_or(RecvError::ChannelClosed)?;
        Ok(())
    }

    /// Waits for a condition to become true and returns a guard to the value.
    ///
    /// This async function repeatedly checks the condition on each value change
    /// and returns when the condition evaluates to `true`.
    ///
    /// # Implementation Note
    /// Uses polling loop: check condition, wait for change, repeat.
    /// Updates version on each iteration to track seen changes.
    ///
    /// # Errors
    /// Returns `RecvError::Failed` if the channel is closed before the
    /// condition is met.
    pub async fn wait_for<F>(&mut self, mut cond: F) -> Result<Guard<'_, T>, RecvError>
    where
        F: FnMut(&T) -> bool,
    {
        loop {
            // Read current value and check if condition is met
            {
                let guard = self.shared.value.borrow();
                let new_version = self.shared.state.load().version();
                let has_changed = self.version != new_version;
                self.version = new_version;
                if cond(&guard) {
                    // We must drop the guard before awaiting to avoid holding the lock across await
                    // point
                    drop(guard);
                    // Re-acquire the guard to return it
                    let guard = self.shared.value.borrow();
                    return Ok(Guard {
                        inner: guard,
                        has_changed,
                    });
                }
                // Explicitly drop the guard to ensure it's not held during await
                drop(guard);
            }

            // Check if channel closed before waiting
            let state = self.shared.state.load();
            if state.is_closed() {
                return Err(RecvError::ChannelClosed);
            }

            // Wait for next change before checking condition again
            self.changed().await?;
        }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let version = self.version;
        let shared = self.shared.clone();
        shared.rx_count.update(|x| x + 1);
        Self { shared, version }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.rx_count.update(|x| x - 1);
        if self.shared.rx_count.get() == 0 {
            // This is the last `Receiver` handle, notify all tasks waiting on
            // `Sender::closed()` usize::MAX means notify all listeners, not
            // just one
            self.shared.closed.notify(usize::MAX);
        }
    }
}

/// Creates a new watch channel with the given initial value.
///
/// Returns a tuple containing the sender and receiver halves of the channel.
///
/// # Examples
///
/// ```
/// use see::unsync::channel;
///
/// # #[tokio::main]
/// # async fn main() {
/// let (tx, mut rx) = channel("hello");
///
/// // Send a new value
/// tx.send("world").unwrap();
///
/// // Wait for the change and read the new value
/// rx.changed().await.unwrap();
/// let value = rx.borrow_and_update();
/// assert_eq!(*value, "world");
/// # }
/// ```
#[must_use]
pub fn channel<T>(init: T) -> (Sender<T>, Receiver<T>) {
    let shared = Rc::new(Shared::new(init));
    let tx = Sender::from_shared(shared.clone());
    let rx = tx.subscribe();
    (tx, rx)
}
