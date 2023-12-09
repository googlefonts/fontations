//! Rounding state.

use super::{
    super::math::{ceil, floor, round, round_pad},
    GraphicsState,
};

/// Rounding strategies supported by the interpreter.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum RoundMode {
    /// Set by `RTG` instruction.
    #[default]
    Grid,
    /// Set by `RTHG` instruction.
    HalfGrid,
    /// Set by `RTDG` instruction.
    DoubleGrid,
    /// Set by `RDTG` instruction.
    DownToGrid,
    /// Set by `RUTG` instruction.
    UpToGrid,
    /// Set by `ROFF` instruction.
    Off,
    /// Set by `SROUND` instruction.
    Super,
    /// Set by `S45ROUND` instruction.
    Super45,
}

/// Graphics state that controls rounding.
///
/// See <https://developer.apple.com/fonts/TrueType-Reference-Manual/RM04/Chap4.html#round%20state>
#[derive(Copy, Clone, Debug)]
pub struct RoundState {
    pub mode: RoundMode,
    pub threshold: i32,
    pub phase: i32,
    pub period: i32,
}

impl Default for RoundState {
    fn default() -> Self {
        Self {
            mode: RoundMode::Grid,
            threshold: 0,
            phase: 0,
            period: 64,
        }
    }
}

impl RoundState {
    pub fn round(&self, distance: i32) -> i32 {
        use RoundMode::*;
        match self.mode {
            HalfGrid => {
                if distance >= 0 {
                    (floor(distance) + 32).max(0)
                } else {
                    (-(floor(-distance) + 32)).min(0)
                }
            }
            Grid => {
                if distance >= 0 {
                    round(distance).max(0)
                } else {
                    (-round(-distance)).min(0)
                }
            }
            DoubleGrid => {
                if distance >= 0 {
                    round_pad(distance, 32).max(0)
                } else {
                    (-round_pad(-distance, 32)).min(0)
                }
            }
            DownToGrid => {
                if distance >= 0 {
                    floor(distance).max(0)
                } else {
                    (-floor(-distance)).min(0)
                }
            }
            UpToGrid => {
                if distance >= 0 {
                    ceil(distance).max(0)
                } else {
                    (-ceil(-distance)).min(0)
                }
            }
            Super => {
                if distance >= 0 {
                    let val =
                        ((distance + (self.threshold - self.phase)) & -self.period) + self.phase;
                    if val < 0 {
                        self.phase
                    } else {
                        val
                    }
                } else {
                    let val =
                        -(((self.threshold - self.phase) - distance) & -self.period) - self.phase;
                    if val > 0 {
                        -self.phase
                    } else {
                        val
                    }
                }
            }
            Super45 => {
                if distance >= 0 {
                    let val = (((distance + (self.threshold - self.phase)) / self.period)
                        * self.period)
                        + self.phase;
                    if val < 0 {
                        self.phase
                    } else {
                        val
                    }
                } else {
                    let val = -((((self.threshold - self.phase) - distance) / self.period)
                        * self.period)
                        - self.phase;
                    if val > 0 {
                        -self.phase
                    } else {
                        val
                    }
                }
            }
            Off => distance,
        }
    }
}

impl GraphicsState<'_> {
    pub fn round(&self, distance: i32) -> i32 {
        self.round_state.round(distance)
    }
}
