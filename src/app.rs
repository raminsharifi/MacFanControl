use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

use crate::control::{Cmd, Controller, Note};
use crate::curve::Curve;
use crate::fan::{self, Fan, FanMode};
use crate::persist;
use crate::plot::Geometry;
use crate::smc::{Smc, is_root};
use crate::temps::Temps;

pub const HISTORY_LEN: usize = 240;
const UNDO_LIMIT: usize = 100;
/// Minimum gap between fan updates while a curve point is being dragged.
const LIVE_APPLY_EVERY: Duration = Duration::from_millis(120);

pub struct Status {
    pub error: bool,
    pub text: String,
}

pub struct App {
    pub smc: Smc,
    pub fans: Vec<Fan>,
    pub desired: Vec<Option<f32>>,
    pub temps: Temps,
    pub model: String,
    pub selected: usize,
    pub linked: bool,
    pub is_root: bool,
    pub status: Option<Status>,
    pub history: Vec<VecDeque<u64>>,
    pub curve: Curve,
    pub curve_active: bool,
    pub curve_input: Option<String>,
    pub plot: Cell<Geometry>,
    pub selected_point: Option<usize>,
    pub dragging: bool,
    pub hover: Option<usize>,
    pub cursor: Option<(f32, f32)>,
    undo: Vec<Curve>,
    drag_origin: Option<Curve>,
    last_live_apply: Instant,
    controller: Controller,
    keep_on_exit: bool,
    needs_refresh: bool,
    last_refresh: Instant,
    last_history: Instant,
}

impl App {
    pub fn new(
        smc: Smc,
        fans: Vec<Fan>,
        temps: Temps,
        model: String,
        controller: Controller,
        curve: Option<Curve>,
    ) -> App {
        let history = fans.iter().map(|_| VecDeque::new()).collect();
        let desired = vec![None; fans.len()];
        let curve_active = curve.is_some();
        let curve = curve.or_else(persist::load).unwrap_or_default();
        let mut app = App {
            smc,
            fans,
            desired,
            temps,
            model,
            selected: 0,
            linked: true,
            is_root: is_root(),
            status: None,
            history,
            curve,
            curve_active,
            curve_input: None,
            plot: Cell::new(Geometry::default()),
            selected_point: None,
            dragging: false,
            hover: None,
            cursor: None,
            undo: Vec::new(),
            drag_origin: None,
            last_live_apply: Instant::now(),
            controller,
            keep_on_exit: false,
            needs_refresh: false,
            last_refresh: Instant::now(),
            last_history: Instant::now(),
        };
        app.record_history();
        app.refit_plot();
        if app.curve_active {
            app.apply_curve();
        }
        app
    }

    pub fn keep_on_exit(&self) -> bool {
        self.keep_on_exit
    }

    pub fn run(
        &mut self,
        terminal: &mut DefaultTerminal,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            if self.drain_notes() {
                return Err("fan control thread stopped unexpectedly; its safety guard restored \
                            automatic control — verify with `macfan --list`"
                    .into());
            }
            if self.last_refresh.elapsed() >= Duration::from_secs(1) {
                self.refresh();
                self.needs_refresh = false;
            } else if self.needs_refresh {
                self.refresh_fans();
                self.needs_refresh = false;
            }
            if self.last_history.elapsed() >= Duration::from_secs(1) {
                self.record_history();
                self.last_history = Instant::now();
            }
            terminal.draw(|frame| crate::ui::draw(frame, self))?;
            if event::poll(Duration::from_millis(100))? {
                // Drain everything queued (mouse motion floods) before redrawing.
                loop {
                    let quit = match event::read()? {
                        Event::Key(k) => k.kind == KeyEventKind::Press && self.on_key(k),
                        Event::Mouse(m) => {
                            self.on_mouse(m);
                            false
                        }
                        _ => false,
                    };
                    if quit {
                        self.finish();
                        return Ok(());
                    }
                    if !event::poll(Duration::ZERO)? {
                        break;
                    }
                }
            }
        }
    }

    fn finish(&mut self) {
        self.controller.send(if self.keep_on_exit {
            Cmd::QuitKeep
        } else {
            Cmd::QuitRestore
        });
    }

    fn drain_notes(&mut self) -> bool {
        loop {
            match self.controller.rx.try_recv() {
                Ok(Note::Info(text)) => self.status = Some(Status { error: false, text }),
                Ok(Note::Error { index, text }) => {
                    match index {
                        Some(i) => self.desired[i] = None,
                        None => self.desired.iter_mut().for_each(|d| *d = None),
                    }
                    self.curve_active = false;
                    self.status = Some(Status { error: true, text });
                }
                Ok(Note::Applied) => {
                    self.status = None;
                    self.needs_refresh = true;
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => return true,
            }
        }
    }

    fn refresh_fans(&mut self) {
        for fan in &mut self.fans {
            fan::refresh(&self.smc, fan);
        }
    }

    fn refresh(&mut self) {
        self.refresh_fans();
        self.temps.refresh(&self.smc);
        self.last_refresh = Instant::now();
        if self.curve_active {
            self.apply_curve();
        }
    }

    fn record_history(&mut self) {
        for (i, fan) in self.fans.iter().enumerate() {
            let h = &mut self.history[i];
            h.push_back(fan.actual.max(0.0) as u64);
            while h.len() > HISTORY_LEN {
                h.pop_front();
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        if self.curve_input.is_some() {
            self.on_curve_input_key(key);
            return false;
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('Q') => {
                self.keep_on_exit = true;
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.fans.len().saturating_sub(1));
            }
            KeyCode::Right if shift => self.bump(500.0),
            KeyCode::Left if shift => self.bump(-500.0),
            KeyCode::Right | KeyCode::Char('l') => self.bump(100.0),
            KeyCode::Left | KeyCode::Char('h') => self.bump(-100.0),
            KeyCode::Char('L') => self.bump(500.0),
            KeyCode::Char('H') => self.bump(-500.0),
            KeyCode::Char('+') | KeyCode::Char('=') => self.bump(100.0),
            KeyCode::Char('-') => self.bump(-100.0),
            KeyCode::Char('m') => self.toggle_mode(),
            KeyCode::Char('c') => self.toggle_curve(),
            KeyCode::Char('C') => {
                self.curve_input = Some(self.curve.to_input_string());
                self.status = None;
            }
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('D') => self.reset_curve(),
            KeyCode::Char('a') => self.all_auto(),
            KeyCode::Char('f') => self.full_blast(),
            KeyCode::Char(' ') => self.linked = !self.linked,
            KeyCode::Char('r') => self.refresh(),
            _ => {}
        }
        false
    }

    fn point_at(&self, col: u16, row: u16) -> Option<usize> {
        let geo = self.plot.get();
        self.curve
            .points()
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let dc = geo.col(p.temp).abs_diff(col);
                let dr = geo.row(p.pct).abs_diff(row);
                (dc <= 2 && dr <= 1).then_some((i, dc + 2 * dr))
            })
            .min_by_key(|&(_, d)| d)
            .map(|(i, _)| i)
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        let geo = self.plot.get();
        if self.curve_input.is_some() || !geo.is_usable() {
            self.hover = None;
            self.cursor = None;
            return;
        }
        let inside = geo.contains(m.column, m.row);
        self.cursor = inside.then(|| geo.data(m.column, m.row));
        self.hover = if inside { self.point_at(m.column, m.row) } else { None };
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if inside => {
                let origin = self.curve.clone();
                let hit = self.hover.or_else(|| {
                    let (temp, pct) = geo.data(m.column, m.row);
                    self.curve.insert(temp, pct)
                });
                self.selected_point = hit;
                self.dragging = hit.is_some();
                self.drag_origin = hit.map(|_| origin);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                if let Some(i) = self.selected_point {
                    let (temp, pct) = geo.data(m.column, m.row);
                    self.curve.move_point(i, temp, pct);
                    self.apply_live();
                }
            }
            // A release can be lost (pointer left the window); motion with no button means it was.
            MouseEventKind::Up(MouseButton::Left) | MouseEventKind::Moved if self.dragging => {
                self.end_drag();
            }
            MouseEventKind::Down(MouseButton::Right) if inside => {
                if let Some(i) = self.hover {
                    let before = self.curve.clone();
                    if self.curve.remove(i) {
                        self.record_undo(before);
                        self.selected_point = None;
                        self.hover = None;
                        self.curve_edited();
                    } else {
                        self.status = Some(Status {
                            error: true,
                            text: "a fan curve needs at least two points".into(),
                        });
                    }
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if inside => {
                let delta = if m.kind == MouseEventKind::ScrollUp { 1.0 } else { -1.0 };
                if let Some(i) = self.hover.or(self.selected_point) {
                    let before = self.curve.clone();
                    self.selected_point = Some(i);
                    self.curve.nudge_pct(i, delta);
                    self.record_undo(before);
                    self.curve_edited();
                }
            }
            _ => {}
        }
    }

    fn end_drag(&mut self) {
        self.dragging = false;
        if let Some(origin) = self.drag_origin.take() {
            self.record_undo(origin);
        }
        self.curve_edited();
    }

    /// Pushes `before` onto the undo stack if the curve actually changed.
    fn record_undo(&mut self, before: Curve) {
        if before == self.curve {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }

    fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.curve = previous;
            self.selected_point = None;
            self.curve_edited();
        }
    }

    fn reset_curve(&mut self) {
        let before = std::mem::take(&mut self.curve);
        self.record_undo(before);
        self.selected_point = None;
        self.curve_edited();
    }

    /// Re-applies the curve while dragging, rate-limited so the SMC isn't flooded.
    fn apply_live(&mut self) {
        if self.curve_active && self.last_live_apply.elapsed() >= LIVE_APPLY_EVERY {
            self.last_live_apply = Instant::now();
            self.apply_curve();
        }
    }

    /// Called once an edit of the curve is finished (not on every drag step).
    fn curve_edited(&mut self) {
        self.status = None;
        self.refit_plot();
        persist::save(&self.curve);
        if self.curve_active {
            self.apply_curve();
        }
    }

    fn refit_plot(&mut self) {
        let mut geo = self.plot.get();
        (geo.lo, geo.hi) = self.curve.view_range();
        self.plot.set(geo);
    }

    fn on_curve_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.curve_input = None;
                self.status = None;
            }
            KeyCode::Enter => {
                let Some(input) = self.curve_input.as_deref() else {
                    return;
                };
                match Curve::parse(input) {
                    Ok(curve) => {
                        let before = std::mem::replace(&mut self.curve, curve);
                        self.record_undo(before);
                        self.selected_point = None;
                        self.refit_plot();
                        persist::save(&self.curve);
                        self.curve_input = None;
                        self.status = None;
                        self.curve_active = true;
                        self.apply_curve();
                    }
                    Err(text) => {
                        self.status = Some(Status { error: true, text });
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(buf) = &mut self.curve_input {
                    buf.pop();
                }
                self.status = None;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(buf) = &mut self.curve_input {
                    buf.clear();
                }
                self.status = None;
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                if let Some(buf) = &mut self.curve_input {
                    buf.push(ch);
                }
                self.status = None;
            }
            _ => {}
        }
    }

    fn target_indices(&self) -> Vec<usize> {
        if self.linked {
            (0..self.fans.len()).collect()
        } else {
            vec![self.selected]
        }
    }

    fn bump(&mut self, delta: f32) {
        self.curve_active = false;
        for i in self.target_indices() {
            let fan = &self.fans[i];
            let base = self.desired[i].unwrap_or(if fan.mode == FanMode::Manual {
                fan.target
            } else {
                fan.actual
            });
            let rpm = (base + delta).clamp(fan.min.max(0.0), fan.max);
            self.desired[i] = Some(rpm);
            self.controller.send(Cmd::SetTarget { index: i, rpm });
        }
    }

    fn toggle_mode(&mut self) {
        self.curve_active = false;
        let to_manual = self.fans[self.selected].mode != FanMode::Manual;
        for i in self.target_indices() {
            if to_manual {
                let fan = &self.fans[i];
                let rpm = self.desired[i].unwrap_or(fan.actual.max(fan.min));
                self.desired[i] = Some(rpm);
                self.controller.send(Cmd::SetTarget { index: i, rpm });
            } else {
                self.desired[i] = None;
                self.controller.send(Cmd::SetAuto { index: i });
            }
        }
    }

    fn toggle_curve(&mut self) {
        if self.curve_active {
            self.all_auto();
        } else {
            self.curve_active = true;
            self.apply_curve();
        }
    }

    fn apply_curve(&mut self) {
        let Some((_, hotspot)) = self.temps.hottest else {
            self.status = Some(Status {
                error: true,
                text: "no readable temperature sensors for fan curve".into(),
            });
            self.curve_active = false;
            return;
        };
        for i in 0..self.fans.len() {
            let rpm = self.curve.rpm_for(&self.fans[i], hotspot).round();
            let same_desired = self.desired[i].is_some_and(|d| (d - rpm).abs() < 1.0);
            let hw_matches =
                self.fans[i].mode == FanMode::Manual && (self.fans[i].target - rpm).abs() < 1.0;
            if same_desired && (hw_matches || self.fans[i].mode != FanMode::Manual) {
                continue;
            }
            self.desired[i] = Some(rpm);
            self.controller.send(Cmd::SetTarget { index: i, rpm });
        }
    }

    fn all_auto(&mut self) {
        self.curve_active = false;
        self.desired.iter_mut().for_each(|d| *d = None);
        self.controller.send(Cmd::AllAuto);
    }

    fn full_blast(&mut self) {
        self.curve_active = false;
        for i in self.target_indices() {
            let rpm = self.fans[i].max;
            self.desired[i] = Some(rpm);
            self.controller.send(Cmd::SetTarget { index: i, rpm });
        }
    }
}
