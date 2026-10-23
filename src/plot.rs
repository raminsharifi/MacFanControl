use ratatui::layout::Rect;

/// Maps between curve coordinates (°C, fan %) and terminal cells of the plot.
/// Cell centres sit exactly on the endpoints, so 0% / 100% and the temperature
/// limits can be reached with the mouse.
#[derive(Clone, Copy, Default)]
pub struct Geometry {
    pub area: Rect,
    pub lo: f32,
    pub hi: f32,
}

impl Geometry {
    pub fn is_usable(&self) -> bool {
        self.area.width >= 2 && self.area.height >= 2 && self.hi > self.lo
    }

    pub fn contains(&self, col: u16, row: u16) -> bool {
        self.is_usable() && self.area.contains((col, row).into())
    }

    fn cols(&self) -> f32 {
        (self.area.width - 1) as f32
    }

    fn rows(&self) -> f32 {
        (self.area.height - 1) as f32
    }

    pub fn col(&self, temp: f32) -> u16 {
        let f = ((temp - self.lo) / (self.hi - self.lo)).clamp(0.0, 1.0);
        self.area.x + (f * self.cols()).round() as u16
    }

    pub fn row(&self, pct: f32) -> u16 {
        let f = (1.0 - pct / 100.0).clamp(0.0, 1.0);
        self.area.y + (f * self.rows()).round() as u16
    }

    /// Curve coordinates for a cell; positions outside the plot are clamped to its edge.
    pub fn data(&self, col: u16, row: u16) -> (f32, f32) {
        let c = col.clamp(self.area.x, self.area.right() - 1) - self.area.x;
        let r = row.clamp(self.area.y, self.area.bottom() - 1) - self.area.y;
        let temp = self.lo + c as f32 / self.cols() * (self.hi - self.lo);
        let pct = 100.0 - r as f32 / self.rows() * 100.0;
        (temp, pct)
    }

    /// Canvas bounds that line braille dots up with the cell grid above.
    pub fn canvas_bounds(&self) -> ([f64; 2], [f64; 2]) {
        let (w, h) = (self.area.width as f64, self.area.height as f64);
        let (lo, hi) = (self.lo as f64, self.hi as f64);
        let t = |dot: f64| lo + (dot - 0.5) / (2.0 * (w - 1.0)) * (hi - lo);
        let p = |dot: f64| 100.0 - (dot - 1.5) / (4.0 * (h - 1.0)) * 100.0;
        ([t(0.0), t(2.0 * w - 1.0)], [p(4.0 * h - 1.0), p(0.0)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::symbols::Marker;
    use ratatui::widgets::canvas::{Canvas, Line};

    #[test]
    fn cell_mapping_round_trips() {
        let geo = Geometry { area: Rect::new(5, 2, 61, 21), lo: 20.0, hi: 110.0 };
        assert_eq!(geo.data(5, 2), (20.0, 100.0));
        assert_eq!(geo.data(65, 22), (110.0, 0.0));
        for (t, p) in [(40.0, 30.0), (60.0, 60.0), (90.0, 100.0)] {
            let (col, row) = (geo.col(t), geo.row(p));
            let (t2, p2) = geo.data(col, row);
            assert!((t2 - t).abs() <= 0.75 && (p2 - p).abs() <= 2.5, "{t},{p} -> {t2},{p2}");
        }
    }

    /// The canvas line must pass through the cell of each point it joins.
    #[test]
    fn canvas_lines_up_with_cells() {
        let geo = Geometry { area: Rect::new(0, 0, 61, 21), lo: 20.0, hi: 110.0 };
        let pts = [(20.0, 0.0), (65.0, 50.0), (110.0, 100.0)];
        let mut term = Terminal::new(TestBackend::new(61, 21)).unwrap();
        let ([x0, x1], [y0, y1]) = geo.canvas_bounds();
        term.draw(|f| {
            let c = Canvas::default().marker(Marker::Braille).x_bounds([x0, x1]).y_bounds([y0, y1])
                .paint(|ctx| {
                    for w in pts.windows(2) {
                        ctx.draw(&Line { x1: w[0].0, y1: w[0].1, x2: w[1].0, y2: w[1].1, color: Color::White });
                    }
                });
            f.render_widget(c, geo.area);
        })
        .unwrap();
        let buf = term.backend().buffer();
        for (t, p) in pts {
            let (col, row) = (geo.col(t as f32), geo.row(p as f32));
            let sym = buf[(col, row)].symbol();
            assert!(sym != " ", "no line at cell ({col},{row}) for {t},{p}\n{buf:?}");
        }
    }
}
