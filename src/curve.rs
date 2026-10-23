use std::fmt;

use crate::fan::Fan;

const MAX_TEMP: f32 = 120.0;
/// Minimum spacing between neighbouring points, in °C.
const MIN_GAP: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub temp: f32,
    pub pct: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    points: Vec<Point>,
}

impl Default for Curve {
    fn default() -> Self {
        Self {
            points: vec![
                Point { temp: 40.0, pct: 30.0 },
                Point { temp: 60.0, pct: 60.0 },
                Point { temp: 90.0, pct: 100.0 },
            ],
        }
    }
}

impl Curve {
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut normalized = String::with_capacity(input.len());
        let mut in_parens = false;
        for ch in input.chars() {
            match ch {
                '(' => {
                    in_parens = true;
                    normalized.push(' ');
                }
                ')' => {
                    in_parens = false;
                    normalized.push(' ');
                }
                ',' if in_parens => normalized.push(':'),
                _ => normalized.push(ch),
            }
        }
        if in_parens {
            return Err("unclosed '(' in curve specification".into());
        }

        let normalized = normalized.replace("->", ",").replace(['→', ';'], ",");
        let cleaned = collapse_sep_spaces(&normalized);

        let mut points = Vec::new();
        for token in cleaned.split([',', ' ']).map(str::trim).filter(|s| !s.is_empty()) {
            let Some((t_raw, p_raw)) = token.split_once(':') else {
                return Err(format!(
                    "invalid curve point '{token}' (expected temp:pct, e.g. 40:30,60:60,90:100)"
                ));
            };
            let temp = parse_temp(t_raw).ok_or_else(|| {
                format!("invalid temperature '{t_raw}' in '{token}' (expected 0–120°C)")
            })?;
            let pct = parse_pct(p_raw).ok_or_else(|| {
                format!("invalid fan percentage '{p_raw}' in '{token}' (expected 0–100%)")
            })?;
            points.push(Point { temp, pct });
        }

        if points.is_empty() {
            return Err("curve requires at least one temp:pct point (e.g. 40:30,60:60,90:100)".into());
        }

        points.sort_by(|a, b| a.temp.total_cmp(&b.temp));
        for w in points.windows(2) {
            if (w[0].temp - w[1].temp).abs() < 1e-4 && (w[0].pct - w[1].pct).abs() >= 1e-4 {
                return Err(format!(
                    "duplicate temperature {}°C with conflicting fan speeds",
                    fmt_num(w[0].temp)
                ));
            }
        }
        points.dedup_by(|a, b| (a.temp - b.temp).abs() < 1e-4);

        Ok(Self { points })
    }

    pub fn eval(&self, temp: f32) -> f32 {
        if !temp.is_finite() || self.points.is_empty() {
            return 100.0;
        }
        let first = self.points[0];
        if temp <= first.temp {
            return first.pct.clamp(0.0, 100.0);
        }
        let last = self.points[self.points.len() - 1];
        if temp >= last.temp {
            return last.pct.clamp(0.0, 100.0);
        }
        for w in self.points.windows(2) {
            if temp >= w[0].temp && temp <= w[1].temp {
                let span = w[1].temp - w[0].temp;
                let pct = if span.abs() < f32::EPSILON {
                    w[1].pct
                } else {
                    w[0].pct + (temp - w[0].temp) / span * (w[1].pct - w[0].pct)
                };
                return pct.clamp(0.0, 100.0);
            }
        }
        last.pct.clamp(0.0, 100.0)
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Temperature range the plot should show: at least 20–110°C, widened to
    /// fit any points outside it.
    pub fn view_range(&self) -> (f32, f32) {
        let first = self.points.first().map_or(40.0, |p| p.temp);
        let last = self.points.last().map_or(90.0, |p| p.temp);
        let lo = ((first / 10.0).floor() * 10.0).clamp(0.0, 20.0);
        let hi = ((last / 10.0).ceil() * 10.0).clamp(110.0, MAX_TEMP);
        (lo, hi)
    }

    /// Moves point `i` to (temp, pct), keeping it between its neighbours.
    pub fn move_point(&mut self, i: usize, temp: f32, pct: f32) {
        let lo = if i > 0 { self.points[i - 1].temp + MIN_GAP } else { 0.0 };
        let hi = match self.points.get(i + 1) {
            Some(next) => next.temp - MIN_GAP,
            None => MAX_TEMP,
        };
        let temp = temp.round().min(hi).max(lo);
        self.points[i] = Point { temp, pct: pct.round().clamp(0.0, 100.0) };
    }

    /// Adds a point, returning its index, or `None` if one is already within `MIN_GAP`.
    pub fn insert(&mut self, temp: f32, pct: f32) -> Option<usize> {
        let temp = temp.round().clamp(0.0, MAX_TEMP);
        if self.points.iter().any(|p| (p.temp - temp).abs() < MIN_GAP) {
            return None;
        }
        let i = self.points.partition_point(|p| p.temp < temp);
        self.points.insert(i, Point { temp, pct: pct.round().clamp(0.0, 100.0) });
        Some(i)
    }

    /// Removes point `i`; a curve always keeps at least two points.
    pub fn remove(&mut self, i: usize) -> bool {
        if self.points.len() <= 2 || i >= self.points.len() {
            return false;
        }
        self.points.remove(i);
        true
    }

    pub fn nudge_pct(&mut self, i: usize, delta: f32) {
        if let Some(p) = self.points.get_mut(i) {
            p.pct = (p.pct + delta).round().clamp(0.0, 100.0);
        }
    }

    pub fn rpm_for(&self, fan: &Fan, temp: f32) -> f32 {
        let pct = self.eval(temp);
        let min = fan.min.max(0.0);
        let max = fan.max.max(min);
        (min + (max - min) * (pct / 100.0)).clamp(min, max)
    }

    pub fn to_input_string(&self) -> String {
        self.points
            .iter()
            .map(|p| format!("{}:{}", fmt_num(p.temp), fmt_num(p.pct)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for Curve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, p) in self.points.iter().enumerate() {
            if i > 0 {
                write!(f, " → ")?;
            }
            write!(f, "{}°C:{}%", fmt_num(p.temp), fmt_num(p.pct))?;
        }
        Ok(())
    }
}

fn fmt_num(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v.round())
    } else {
        format!("{v:.1}")
    }
}

fn collapse_sep_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    let mut after_sep = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !after_sep {
                pending_space = true;
            }
        } else if ch == ':' || ch == '=' {
            pending_space = false;
            after_sep = true;
            out.push(':');
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            after_sep = false;
            out.push(ch);
        }
    }
    out
}

fn parse_temp(s: &str) -> Option<f32> {
    let s = s.trim();
    let s = s.strip_suffix('c').or_else(|| s.strip_suffix('C')).unwrap_or(s);
    let s = s.strip_suffix('°').unwrap_or(s).trim();
    let v: f32 = s.parse().ok()?;
    (v.is_finite() && (0.0..=120.0).contains(&v)).then_some(v)
}

fn parse_pct(s: &str) -> Option<f32> {
    let s = s.trim();
    let s = s.strip_suffix('%').unwrap_or(s).trim();
    let v: f32 = s.parse().ok()?;
    (v.is_finite() && (0.0..=100.0).contains(&v)).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan::FanMode;

    #[test]
    fn piecewise_linear_interpolation() {
        let c = Curve::default(); // (40, 30) -> (60, 60) -> (90, 100)
        assert!((c.eval(25.0) - 30.0).abs() < 1e-4);
        assert!((c.eval(40.0) - 30.0).abs() < 1e-4);
        assert!((c.eval(50.0) - 45.0).abs() < 1e-4);
        assert!((c.eval(60.0) - 60.0).abs() < 1e-4);
        assert!((c.eval(75.0) - 80.0).abs() < 1e-4);
        assert!((c.eval(90.0) - 100.0).abs() < 1e-4);
        assert!((c.eval(105.0) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn parse_formats_and_sorting() {
        let expected = Curve::default();
        for input in [
            "40:30,60:60,90:100",
            "40c:30%,60c:60%,90c:100%",
            "40C:30%, 60C:60%, 90C:100%",
            "40°C:30% → 60°C:60% → 90°C:100%",
            "(40c, 30%) -> (60c, 60%) -> (90c, 100%)",
            "90c=100% 60c=60% 40c=30%",
            "90:100;60:60;40:30",
        ] {
            let parsed = Curve::parse(input).unwrap_or_else(|e| panic!("failed on '{input}': {e}"));
            assert_eq!(parsed, expected, "mismatch for '{input}'");
        }
    }

    #[test]
    fn rpm_mapping() {
        let c = Curve::default();
        let fan = Fan {
            index: 0,
            name: "Left Fan".into(),
            min: 1000.0,
            max: 5000.0,
            actual: 0.0,
            target: 0.0,
            mode: FanMode::Auto,
            md_key: None,
        };
        assert!((c.rpm_for(&fan, 40.0) - 2200.0).abs() < 1e-3); // 30%
        assert!((c.rpm_for(&fan, 60.0) - 3400.0).abs() < 1e-3); // 60%
        assert!((c.rpm_for(&fan, 90.0) - 5000.0).abs() < 1e-3); // 100%
    }

    #[test]
    fn editing_keeps_points_ordered() {
        let mut c = Curve::default();
        c.move_point(1, 95.0, 120.0); // clamped between 40 and 90, pct capped
        assert_eq!(c.points()[1], Point { temp: 89.0, pct: 100.0 });
        c.move_point(0, -5.0, 10.4);
        assert_eq!(c.points()[0], Point { temp: 0.0, pct: 10.0 });
        assert_eq!(c.insert(89.0, 50.0), None);
        assert_eq!(c.insert(70.0, 50.0), Some(1));
        assert_eq!(c.points().len(), 4);
        assert!(c.remove(2));
        assert!(c.remove(1));
        assert!(!c.remove(0)); // keeps two points
        c.nudge_pct(0, -50.0);
        assert_eq!(c.points()[0].pct, 0.0);
    }

    #[test]
    fn reject_invalid_curves() {
        assert!(Curve::parse("").is_err());
        assert!(Curve::parse("40:120").is_err());
        assert!(Curve::parse("200:50").is_err());
        assert!(Curve::parse("60:40,60:80").is_err());
        assert!(Curve::parse("(40, 30").is_err());
    }
}
