//! Rounding state.

use super::GraphicsState;

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
        use super::super::math;
        use RoundMode::*;
        match self.mode {
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L1958>
            HalfGrid => {
                if distance >= 0 {
                    (math::floor(distance) + 32).max(0)
                } else {
                    (-(math::floor(-distance) + 32)).min(0)
                }
            }
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L1913>
            Grid => {
                if distance >= 0 {
                    math::round(distance).max(0)
                } else {
                    (-math::round(-distance)).min(0)
                }
            }
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L2094>
            DoubleGrid => {
                if distance >= 0 {
                    math::round_pad(distance, 32).max(0)
                } else {
                    (-math::round_pad(-distance, 32)).min(0)
                }
            }
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L2005>
            DownToGrid => {
                if distance >= 0 {
                    math::floor(distance).max(0)
                } else {
                    (-math::floor(-distance)).min(0)
                }
            }
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L2049>
            UpToGrid => {
                if distance >= 0 {
                    math::ceil(distance).max(0)
                } else {
                    (-math::ceil(-distance)).min(0)
                }
            }
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L2145>
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
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L2199>
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
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L1870>
            Off => distance,
        }
    }
}

impl GraphicsState<'_> {
    pub fn round(&self, distance: i32) -> i32 {
        self.round_state.round(distance)
    }
}

#[cfg(test)]
mod tests {
    use super::{RoundMode, RoundState};

    #[test]
    fn round_to_grid() {
        round_cases(
            RoundMode::Grid,
            &[(0, 0), (32, 64), (-32, -64), (64, 64), (50, 64)],
        );
    }

    #[test]
    fn round_to_half_grid() {
        round_cases(
            RoundMode::HalfGrid,
            &[(0, 32), (32, 32), (-32, -32), (64, 96), (50, 32)],
        );
    }

    #[test]
    fn round_to_double_grid() {
        round_cases(
            RoundMode::DoubleGrid,
            &[(0, 0), (32, 32), (-32, -32), (64, 64), (50, 64)],
        );
    }

    #[test]
    fn round_down_to_grid() {
        round_cases(
            RoundMode::DownToGrid,
            &[(0, 0), (32, 0), (-32, 0), (64, 64), (50, 0)],
        );
    }

    #[test]
    fn round_up_to_grid() {
        round_cases(
            RoundMode::UpToGrid,
            &[(0, 0), (32, 64), (-32, -64), (64, 64), (50, 64)],
        );
    }

    #[test]
    fn round_off() {
        round_cases(
            RoundMode::Off,
            &[(0, 0), (32, 32), (-32, -32), (64, 64), (50, 50)],
        );
    }

    fn round_cases(mode: RoundMode, cases: &[(i32, i32)]) {
        for (value, expected) in cases.iter().copied() {
            let state = RoundState {
                mode,
                ..Default::default()
            };
            let result = state.round(value);
            assert_eq!(
                result, expected,
                "mismatch in rounding: {mode:?}({value}) = {result} (expected {expected})"
            );
        }
    }
}
