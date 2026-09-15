// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strongly-typed identifiers used across the simulation engine.

use std::fmt;

/// Unique board identifier (UUID v4, generated without external crates).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BoardId([u8; 16]);

impl BoardId {
    /// Generates a new random (v4-flavored) board id.
    pub fn new() -> Self {
        let mut bytes = [0u8; 16];
        let mut state = seed_from_entropy();
        for chunk in bytes.chunks_mut(8) {
            state = xorshift64_star(state);
            chunk.copy_from_slice(&state.to_le_bytes()[..chunk.len()]);
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
        bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
        Self(bytes)
    }

    /// Wraps an existing 16-byte UUID.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the raw 16 bytes.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl Default for BoardId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BoardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
            b[14], b[15]
        )
    }
}

fn seed_from_entropy() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15);
    nanos ^ (&nanos as *const u64 as usize as u64)
}

fn xorshift64_star(mut x: u64) -> u64 {
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    x.wrapping_mul(0x2545F4914F6CDD1D)
}

macro_rules! copy_id {
    ($(#[$meta:meta])* $name:ident, $inner:ty) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        pub struct $name(pub $inner);

        impl $name {
            /// Creates a new id.
            pub const fn new(value: $inner) -> Self {
                Self(value)
            }

            /// Returns the inner value.
            pub const fn value(&self) -> $inner {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        pub struct $name(pub String);

        impl $name {
            /// Creates a new id.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Returns the inner value.
            pub fn value(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

copy_id!(
    /// Zero-based layer index within a [`Stackup`](crate::Stackup).
    LayerId,
    u32
);
string_id!(
    /// Reference designator based component id (e.g. `"U1"`).
    ComponentId
);
string_id!(
    /// Net identifier (net name).
    NetId
);
copy_id!(
    /// Trace identifier.
    TraceId,
    u64
);
copy_id!(
    /// Via identifier.
    ViaId,
    u64
);
copy_id!(
    /// Pad identifier.
    PadId,
    u64
);
string_id!(
    /// Material identifier (e.g. `"copper"`, `"fr4"`).
    MaterialId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_id_is_valid_uuid_v4_shape() {
        let id = BoardId::new();
        let s = id.to_string();
        assert_eq!(s.len(), 36);
        let parts: Vec<&str> = s.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(s.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    }

    #[test]
    fn board_ids_are_distinct() {
        assert_ne!(BoardId::new(), BoardId::new());
    }

    #[test]
    fn simple_ids_round_trip() {
        let net = NetId::new("VCC");
        assert_eq!(net.value(), "VCC");
        assert_eq!(net.to_string(), "VCC");
        assert_eq!(LayerId::new(3).value(), 3);
    }
}
