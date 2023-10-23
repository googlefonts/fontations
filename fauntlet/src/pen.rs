use skrifa::raw::types::{Pen, PenCommand};

pub struct RegularizingPen<'a, P> {
    inner: &'a mut P,
    pending_move: Option<(f32, f32)>,
    last_start: (f32, f32),
    last_end: Option<(f32, f32)>,
}

impl<'a, P: Pen> RegularizingPen<'a, P> {
    pub fn new(inner: &'a mut P) -> Self {
        Self {
            inner,
            pending_move: None,
            last_start: Default::default(),
            last_end: None,
        }
    }

    fn flush_pending_move(&mut self) {
        if let Some(start) = self.pending_move.take() {
            self.inner.move_to(start.0, start.1);
        }
    }
}

impl<'a, P: Pen> Pen for RegularizingPen<'a, P> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pending_move = Some((x, y));
        self.last_start = (x, y);
        self.last_end = Some((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if self.last_end != Some((x, y)) {
            self.flush_pending_move();
            self.inner.line_to(x, y);
            self.last_end = Some((x, y));
        }
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.flush_pending_move();
        self.inner.quad_to(cx0, cy0, x, y);
        self.last_end = Some((x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.flush_pending_move();
        self.inner.curve_to(cx0, cy0, cx1, cy1, x, y);
        self.last_end = Some((x, y));
    }

    fn close(&mut self) {
        // FreeType emits lines instead of close commands. Do the same if we
        // haven't already reconnected to the start point.
        if self.last_end != Some(self.last_start) {
            self.inner.line_to(self.last_start.0, self.last_start.1);
        }
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct RecordingPen(pub Vec<PenCommand>);

impl Pen for RecordingPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(PenCommand::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(PenCommand::LineTo { x, y });
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push(PenCommand::QuadTo { cx0, cy0, x, y });
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(PenCommand::CurveTo {
            cx0,
            cy0,
            cx1,
            cy1,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.0.push(PenCommand::Close)
    }
}

pub struct NullPen {}

impl Pen for NullPen {
    fn move_to(&mut self, _x: f32, _y: f32) {}
    fn quad_to(&mut self, _cx0: f32, _cy0: f32, _x: f32, _y: f32) {}
    fn curve_to(&mut self, _cx0: f32, _cy0: f32, _cx1: f32, _cy1: f32, _x: f32, _y: f32) {}
    fn line_to(&mut self, _x: f32, _y: f32) {}
    fn close(&mut self) {}
}
