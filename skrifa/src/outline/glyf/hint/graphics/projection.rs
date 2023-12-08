//! Point projection.

use super::{super::math::dot14, CoordAxis, GraphicsState, Point};

impl GraphicsState<'_> {
    /// Should be called whenever projection vectors are modified.
    pub fn update_projection_state(&mut self) {
        if self.freedom_vector.x == 0x4000 {
            self.fdotp = self.proj_vector.x as i32;
        } else if self.freedom_vector.y == 0x4000 {
            self.fdotp = self.proj_vector.y as i32;
        } else {
            let px = self.proj_vector.x as i32;
            let py = self.proj_vector.y as i32;
            let fx = self.freedom_vector.x as i32;
            let fy = self.freedom_vector.y as i32;
            self.fdotp = (px * fx + py * fy) >> 14;
        }
        self.proj_axes = CoordAxis::Both;
        if self.proj_vector.x == 0x4000 {
            self.proj_axes = CoordAxis::X;
        } else if self.proj_vector.y == 0x4000 {
            self.proj_axes = CoordAxis::Y;
        }
        self.dual_proj_axes = CoordAxis::Both;
        if self.dual_proj_vector.x == 0x4000 {
            self.dual_proj_axes = CoordAxis::X;
        } else if self.dual_proj_vector.y == 0x4000 {
            self.dual_proj_axes = CoordAxis::Y;
        }
        self.freedom_axes = CoordAxis::Both;
        if self.fdotp == 0x4000 {
            if self.freedom_vector.x == 0x4000 {
                self.freedom_axes = CoordAxis::X;
            } else if self.freedom_vector.y == 0x4000 {
                self.freedom_axes = CoordAxis::Y;
            }
        }
        if self.fdotp.abs() < 0x400 {
            self.fdotp = 0x4000;
        }
    }

    #[inline(always)]
    pub fn project(&self, v1: Point<i32>, v2: Point<i32>) -> i32 {
        match self.proj_axes {
            CoordAxis::X => v1.x - v2.x,
            CoordAxis::Y => v1.y - v2.y,
            CoordAxis::Both => {
                let x = v1.x - v2.x;
                let y = v1.y - v2.y;
                dot14(x, y, self.proj_vector.x as i32, self.proj_vector.y as i32)
            }
        }
    }

    #[inline(always)]
    pub fn dual_project(&self, v1: Point<i32>, v2: Point<i32>) -> i32 {
        match self.dual_proj_axes {
            CoordAxis::X => v1.x - v2.x,
            CoordAxis::Y => v1.y - v2.y,
            CoordAxis::Both => {
                let x = v1.x - v2.x;
                let y = v1.y - v2.y;
                dot14(
                    x,
                    y,
                    self.dual_proj_vector.x as i32,
                    self.dual_proj_vector.y as i32,
                )
            }
        }
    }
}
