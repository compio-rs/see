/// Represents a version number for tracking changes in the watch channel.
///
/// Uses a step size of 2 to avoid conflicts with the closed bit flag.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Version(usize);

impl Version {
    /// Initial version when channel is created
    pub const INITIAL: Self = Version(0);
    /// Step size for version increments to avoid conflicts with closed bit
    ///
    /// Uses step size 2 so version numbers are always even, leaving LSB
    /// available for closed flag without affecting version comparison.
    pub const STEP: usize = 2;

    /// Decrements the version number, used to force detection of changes.
    ///
    /// This is a wrapping subtraction to avoid overflow issues.
    #[inline]
    pub fn decrement(&mut self) {
        self.0 = self.0.wrapping_sub(Self::STEP);
    }

    pub fn inner(&self) -> usize {
        self.0
    }
}

/// A snapshot of the channel state that combines version and closed status.
///
/// The least significant bit is used to track whether the channel is closed,
/// while the remaining bits store the version number.
#[derive(Copy, Clone, Debug)]
pub struct StateSnapshot(usize);

impl StateSnapshot {
    /// Bit mask for the closed flag (least significant bit)
    pub const CLOSED_BIT: usize = 1;

    pub fn from_usize(value: usize) -> Self {
        StateSnapshot(value)
    }

    /// Extracts the version number from the state snapshot.
    ///
    /// Masks out the closed bit (LSB) to get the pure version number.
    #[inline]
    pub fn version(self) -> Version {
        Version(self.0 & !Self::CLOSED_BIT)
    }

    /// Checks if the channel is closed.
    #[inline]
    pub fn is_closed(self) -> bool {
        (self.0 & Self::CLOSED_BIT) == Self::CLOSED_BIT
    }
}
