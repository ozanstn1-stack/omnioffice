//! One code of a composite font, with the number of bytes that carry it.
//!
//! An `Identity-H` font has two-byte codes, and so does a `/ToUnicode` map that
//! only ever writes `<0041>`. A font whose `/Encoding` is an embedded CMap with a
//! one-byte codespace writes `<41>` instead - the same number, a different code.
//! Drawing needs both halves: the value selects the CID, the width decides how
//! many hex digits the appearance writes and how a viewer splits the string.
//!
//! Keeping them in one type keeps them in step: a parallel array of widths is a
//! second thing to forget to update.

/// A code and the number of bytes it is written with: always one or two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Code {
    /// Compared first, so the lowest value wins whatever its width.
    value: u16,
    width: u8,
}

impl Code {
    /// The code a hex string or a codespace bound stands for; `None` when it is
    /// not one or two bytes wide.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [byte] => Some(Self { value: u16::from(*byte), width: 1 }),
            [high, low] => Some(Self { value: u16::from_be_bytes([*high, *low]), width: 2 }),
            _ => None,
        }
    }

    /// A one-byte code.
    pub(crate) fn one(value: u8) -> Self {
        Self { value: u16::from(value), width: 1 }
    }

    /// A two-byte code.
    pub(crate) fn two(value: u16) -> Self {
        Self { value, width: 2 }
    }

    pub(crate) fn value(self) -> u16 {
        self.value
    }

    pub(crate) fn width(self) -> u8 {
        self.width
    }

    /// The `<hex>` body of this code: two digits for one byte, four for two.
    pub(crate) fn hex(self) -> String {
        match self.width {
            1 => format!("{:02X}", self.value & 0xFF),
            _ => format!("{:04X}", self.value),
        }
    }

    /// The code `offset` values above this one, or `None` when that would leave
    /// the code's own width.
    pub(crate) fn offset(self, offset: u16) -> Option<Self> {
        let value = self.value.checked_add(offset)?;
        let fits = match self.width {
            1 => value <= u16::from(u8::MAX),
            _ => true,
        };
        fits.then_some(Self { value, width: self.width })
    }

    /// The bytes this code is written as, for a caller that builds a string.
    pub(crate) fn to_bytes(self) -> Vec<u8> {
        match self.width {
            1 => vec![self.value as u8],
            _ => self.value.to_be_bytes().to_vec(),
        }
    }
}

impl Default for Code {
    /// An unused slot writes `<0000>`, as a zeroed two-byte code did.
    fn default() -> Self {
        Self::two(0)
    }
}

#[cfg(test)]
mod tests;
