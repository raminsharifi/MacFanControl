use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Block, BorderType, Gauge, Paragraph, Sparkline};

use crate::app::App;
use crate::fan::{Fan, FanMode};
use crate::plot::Geometry;

const ACCENT: Color = Color::Rgb(0, 215, 255);
const HOT: Color = Color::Rgb(255, 95, 95);
const WARM: Color = Color::Rgb(255, 215, 0);
const COOL: Color = Color::Rgb(0, 255, 135);
const DIM: Color = Color::Rgb(110, 110, 130);
const MANUAL: Color = Color::Rgb(255, 121, 198);

fn rpm_color(ratio: f64) -> Color {
    if ratio < 0.45 {
        COOL
    } else if ratio < 0.75 {
        WARM
    } else {
        HOT
    }
}

fn temp_color(t: f32) -> Color {
    if t < 60.0 {
        COOL
    } else if t < 85.0 {
        WARM
    } else {
        HOT
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let show_curve_row = app.curve_active || app.curve_input.is_some();
    let mut constraints = vec![Constraint::Length(1)];
    constraints.extend(app.fans.iter().map(|_| Constraint::Length(4)));
    let thermals_height = if show_curve_row { 4 } else { 3 };
    constraints.push(Constraint::Length(thermals_height));
    constraints.push(Constraint::Min(9));
    // The RPM history only earns its space once the curve plot has room to be usable.
    let fixed = 1 + 4 * app.fans.len() as u16 + thermals_height + 2;
    let show_history = frame.area().height.saturating_sub(fixed) >= 17;
    if show_history {
        constraints.push(Constraint::Length(5));
    }
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Length(1));
    let areas = Layout::vertical(constraints).split(frame.area());

    draw_title(frame, areas[0], app);
    for (i, fan) in app.fans.iter().enumerate() {
        draw_fan(
            frame,
            areas[1 + i],
            fan,
            app.desired[i],
            i == app.selected,
            app.linked,
            app.curve_active,
        );
    }
    let base = 1 + app.fans.len();
    draw_temps(frame, areas[base], app);
    draw_curve_plot(frame, areas[base + 1], app);
    let mut next = base + 2;
    if show_history {
        draw_sparkline(frame, areas[next], app);
        next += 1;
    }
    draw_status(frame, areas[next], app);
    draw_help(frame, areas[next + 1]);
}

fn draw_title(frame: &mut Frame, area: Rect, app: &App) {
    let access = if app.is_root {
        Span::styled(" CONTROL ", Style::new().fg(Color::Black).bg(COOL).bold())
    } else {
        Span::styled(" READ-ONLY ", Style::new().fg(Color::Black).bg(WARM).bold())
    };
    let line = Line::from(vec![
        Span::styled(" MACFAN ", Style::new().fg(Color::Black).bg(ACCENT).bold()),
        Span::raw(" "),
        Span::styled(&app.model, Style::new().fg(Color::White).bold()),
        Span::raw("  "),
        access,
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn mode_badge(mode: FanMode, curve_active: bool) -> Span<'static> {
    match mode {
        FanMode::Manual if curve_active => {
            Span::styled(" CURVE ", Style::new().fg(Color::Black).bg(MANUAL).bold())
        }
        FanMode::Manual => {
            Span::styled(" MANUAL ", Style::new().fg(Color::Black).bg(MANUAL).bold())
        }
        FanMode::System => Span::styled(
            " SYSTEM ",
            Style::new().fg(Color::Black).bg(Color::Rgb(150, 150, 255)).bold(),
        ),
        FanMode::Auto => Span::styled(" AUTO ", Style::new().fg(Color::Black).bg(ACCENT).bold()),
    }
}

fn draw_fan(
    frame: &mut Frame,
    area: Rect,
    fan: &Fan,
    desired: Option<f32>,
    selected: bool,
    linked: bool,
    curve_active: bool,
) {
    let marker = if selected { "▶ " } else { "  " };
    let link = if selected && linked {
        Span::styled(" linked ", Style::new().fg(DIM).italic())
    } else {
        Span::raw("")
    };
    let border_style = if selected {
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(DIM)
    };
    let block = Block::bordered()
        .border_type(if selected {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(border_style)
        .title(Line::from(vec![
            Span::styled(
                format!("{marker}{} ", fan.name),
                if selected {
                    Style::new().fg(ACCENT).bold()
                } else {
                    Style::new().fg(Color::White)
                },
            ),
            mode_badge(fan.mode, curve_active),
            link,
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);

    let mut spans = vec![
        Span::styled(
            format!("{:>5.0}", fan.actual),
            Style::new().fg(Color::White).bold(),
        ),
        Span::styled(" RPM", Style::new().fg(DIM)),
    ];
    if fan.mode == FanMode::Manual {
        spans.push(Span::styled(
            format!("  target {:>5.0}", fan.target),
            Style::new().fg(MANUAL).bold(),
        ));
    } else {
        spans.push(Span::styled(
            format!("  target {:>5.0}", fan.target),
            Style::new().fg(DIM),
        ));
    }
    if let Some(d) = desired
        && (d - fan.target).abs() > 1.0 {
            spans.push(Span::styled(
                format!(" → {d:.0}"),
                Style::new().fg(WARM).bold(),
            ));
        }
    spans.push(Span::styled(
        format!("    min {:.0} · max {:.0}", fan.min, fan.max),
        Style::new().fg(DIM),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), rows[0]);

    let span = (fan.max - fan.min).max(1.0);
    let raw = ((fan.actual - fan.min) / span) as f64;
    let ratio = if raw.is_finite() {
        raw.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let gauge = Gauge::default()
        .ratio(ratio)
        .label(Span::styled(
            format!("{:>4.0} RPM", fan.actual),
            Style::new().fg(Color::White).bold(),
        ))
        .gauge_style(Style::new().fg(rpm_color(ratio)).bg(Color::Rgb(30, 30, 40)));
    frame.render_widget(gauge, rows[1]);
}

fn draw_temps(frame: &mut Frame, area: Rect, app: &App) {
    let mut title_spans = vec![Span::styled(
        " thermals ",
        Style::new().fg(Color::White).bold(),
    )];
    if app.curve_active {
        title_spans.push(Span::styled(
            " CURVE ",
            Style::new().fg(Color::Black).bg(MANUAL).bold(),
        ));
        title_spans.push(Span::raw(" "));
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(DIM))
        .title(Line::from(title_spans));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let show_curve_row = app.curve_active || app.curve_input.is_some();
    let rows = if show_curve_row {
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner)
    } else {
        Layout::vertical([Constraint::Length(1)]).split(inner)
    };

    let temp_line = match &app.temps.hottest {
        Some((key, t)) => Line::from(vec![
            Span::styled("avg ", Style::new().fg(DIM)),
            Span::styled(
                format!("{:.1}°C", app.temps.avg),
                Style::new().fg(temp_color(app.temps.avg)).bold(),
            ),
            Span::styled("   hottest ", Style::new().fg(DIM)),
            Span::styled(
                format!("{t:.1}°C"),
                Style::new().fg(temp_color(*t)).bold(),
            ),
            Span::styled(format!(" ({key})"), Style::new().fg(DIM)),
            Span::styled(
                format!("   {} sensors", app.temps.sensor_count()),
                Style::new().fg(DIM),
            ),
        ]),
        None => Line::from(Span::styled(
            "no readable temperature sensors",
            Style::new().fg(DIM).italic(),
        )),
    };
    frame.render_widget(Paragraph::new(temp_line), rows[0]);

    if show_curve_row {
        let curve_line = if let Some(input) = &app.curve_input {
            Line::from(vec![
                Span::styled("curve ", Style::new().fg(ACCENT).bold()),
                Span::styled(input.as_str(), Style::new().fg(Color::White).bold()),
                Span::styled("█", Style::new().fg(ACCENT)),
                Span::styled(
                    "  (temp:pct, e.g. 40:30,60:60,90:100 · Enter apply · Esc cancel)",
                    Style::new().fg(DIM),
                ),
            ])
        } else {
            let mut spans = vec![
                Span::styled("curve ", Style::new().fg(DIM)),
                Span::styled(app.curve.to_string(), Style::new().fg(Color::White)),
            ];
            if let Some((_, t)) = &app.temps.hottest {
                let pct = app.curve.eval(*t);
                spans.push(Span::styled(
                    format!("   → {pct:.0}%"),
                    Style::new().fg(MANUAL).bold(),
                ));
            }
            Line::from(spans)
        };
        frame.render_widget(Paragraph::new(curve_line), rows[1]);
    }
}

const GRID: Color = Color::Rgb(55, 55, 70);
const FILL: Color = Color::Rgb(24, 24, 36);
const FILL_ACTIVE: Color = Color::Rgb(44, 26, 46);
const Y_GUTTER: u16 = 5;

fn draw_curve_plot(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if app.dragging { ACCENT } else { DIM }))
        .title(plot_title(app))
        .title_bottom(Line::from(Span::styled(
            " drag point · click add · right-click delete · scroll ±1% · u undo · D reset ",
            Style::new().fg(DIM),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Published for the mouse handler, which hit-tests against exactly what was drawn.
    let (lo, hi) = (app.plot.get().lo, app.plot.get().hi);
    let geo = Geometry {
        area: Rect {
            x: inner.x + Y_GUTTER,
            y: inner.y,
            width: inner.width.saturating_sub(Y_GUTTER),
            height: inner.height.saturating_sub(1),
        },
        lo,
        hi,
    };
    app.plot.set(if geo.is_usable() { geo } else { Geometry { lo, hi, ..Geometry::default() } });
    if !geo.is_usable() {
        return;
    }

    draw_grid(frame, &geo, inner.x);
    draw_curve(frame, &geo, app);
    draw_markers(frame, &geo, app);
}

fn plot_title(app: &App) -> Line<'static> {
    let mut spans = vec![Span::styled(" fan curve ", Style::new().fg(Color::White).bold())];
    if app.curve_active {
        spans.push(Span::styled(" CURVE ", Style::new().fg(Color::Black).bg(MANUAL).bold()));
    } else {
        spans.push(Span::styled(" inactive — press c to apply ", Style::new().fg(DIM).italic()));
    }
    let focus = if app.dragging { app.selected_point } else { app.hover.or(app.selected_point) };
    if let Some(p) = focus.and_then(|i| app.curve.points().get(i)) {
        let rpm = app.curve.rpm_for(&app.fans[app.selected], p.temp);
        spans.push(Span::styled(
            format!(" {:.0}°C → {:.0}% ≈ {rpm:.0} RPM ", p.temp, p.pct),
            Style::new().fg(WARM).bold(),
        ));
    } else if let Some((t, pct)) = app.cursor {
        spans.push(Span::styled(format!(" {t:.0}°C · {pct:.0}% "), Style::new().fg(DIM)));
    }
    Line::from(spans)
}

fn draw_grid(frame: &mut Frame, geo: &Geometry, gutter_x: u16) {
    let area = geo.area;
    let grid = Style::new().fg(GRID);
    let label = Style::new().fg(DIM);
    let buf = frame.buffer_mut();
    for pct in [0.0, 25.0, 50.0, 75.0, 100.0] {
        let row = geo.row(pct);
        for col in area.left()..area.right() {
            buf[(col, row)].set_symbol("┄").set_style(grid);
        }
        buf.set_string(gutter_x, row, format!("{pct:>3.0}%"), label);
    }
    let step = if area.width >= 60 { 10.0 } else { 20.0 };
    let mut next_free = 0;
    let mut t = (geo.lo / step).ceil() * step;
    while t <= geo.hi {
        let col = geo.col(t);
        for row in area.top()..area.bottom() {
            let cell = &mut buf[(col, row)];
            if cell.symbol() == " " {
                cell.set_symbol("┆").set_style(grid);
            }
        }
        let text = format!("{t:.0}");
        let width = text.len() as u16;
        let start = col.saturating_sub(width / 2);
        if start >= next_free && start + width <= area.right() {
            buf.set_string(start, area.bottom(), &text, label);
            next_free = start + width + 1;
        }
        t += step;
    }
    buf.set_string(area.right().saturating_sub(2), area.bottom(), "°C", label);
}

fn draw_curve(frame: &mut Frame, geo: &Geometry, app: &App) {
    let area = geo.area;
    let points = app.curve.points();
    let line_color = if app.curve_active { MANUAL } else { Color::Rgb(170, 170, 190) };
    let hot = app.temps.hottest.as_ref().map(|(_, t)| *t);
    let ([x0, x1], [y0, y1]) = geo.canvas_bounds();
    let (lo, hi) = (f64::from(geo.lo), f64::from(geo.hi));
    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([x0, x1])
        .y_bounds([y0, y1])
        .paint(|ctx| {
            if let Some(t) = hot.map(f64::from) {
                ctx.draw(&CanvasLine { x1: t, y1: 0.0, x2: t, y2: 100.0, color: Color::Rgb(0, 110, 130) });
            }
            let (Some(first), Some(last)) = (points.first(), points.last()) else {
                return;
            };
            // The curve is flat beyond its end points, so extend it to the plot edges.
            let mut path = vec![(lo, f64::from(first.pct))];
            path.extend(points.iter().map(|p| (f64::from(p.temp), f64::from(p.pct))));
            path.push((hi, f64::from(last.pct)));
            for w in path.windows(2) {
                ctx.draw(&CanvasLine { x1: w[0].0, y1: w[0].1, x2: w[1].0, y2: w[1].1, color: line_color });
            }
        });
    frame.render_widget(canvas, area);

    // Tint the region under the curve (after the canvas, which resets cell backgrounds).
    let fill = if app.curve_active { FILL_ACTIVE } else { FILL };
    let buf = frame.buffer_mut();
    for col in area.left()..area.right() {
        let (temp, _) = geo.data(col, area.y);
        for row in geo.row(app.curve.eval(temp)) + 1..area.bottom() {
            buf[(col, row)].set_bg(fill);
        }
    }
}

fn draw_markers(frame: &mut Frame, geo: &Geometry, app: &App) {
    let buf = frame.buffer_mut();
    if let Some(t) = app.temps.hottest.as_ref().map(|(_, t)| *t) {
        buf[(geo.col(t), geo.row(app.curve.eval(t)))]
            .set_symbol("◆")
            .set_style(Style::new().fg(ACCENT).bold());
    }
    for (i, p) in app.curve.points().iter().enumerate() {
        let (symbol, color) = if app.selected_point == Some(i) {
            ("◉", WARM)
        } else if app.hover == Some(i) {
            ("◉", ACCENT)
        } else {
            ("●", Color::White)
        };
        buf[(geo.col(p.temp), geo.row(p.pct))]
            .set_symbol(symbol)
            .set_style(Style::new().fg(color).bold());
    }
}

fn draw_sparkline(frame: &mut Frame, area: Rect, app: &App) {
    let fan = &app.fans[app.selected];
    let history = &app.history[app.selected];
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(DIM))
        .title(Span::styled(
            format!(" {} — rpm history ", fan.name),
            Style::new().fg(Color::White).bold(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let skip = history.len().saturating_sub(width);
    let data: Vec<u64> = history.iter().copied().skip(skip).collect();
    let spark = Sparkline::default()
        .data(data)
        .max(fan.max.max(1.0) as u64)
        .style(Style::new().fg(ACCENT));
    frame.render_widget(spark, inner);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(status) = &app.status {
        if status.error {
            Line::from(Span::styled(
                format!(" ⚠ {}", status.text),
                Style::new().fg(HOT).bold(),
            ))
        } else {
            Line::from(Span::styled(
                format!(" ◌ {}", status.text),
                Style::new().fg(WARM),
            ))
        }
    } else if !app.is_root {
        Line::from(Span::styled(
            " read-only mode — run `sudo macfan` to control fans",
            Style::new().fg(WARM),
        ))
    } else {
        Line::from(Span::styled(
            " fans restore to auto on quit (q) — use Q to keep settings",
            Style::new().fg(DIM),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help = Line::from(Span::styled(
        " ↑↓ select   ←→ ±100   ⇧←→ ±500   m manual/auto   c curve   C edit curve   u undo curve   a all auto   f full blast   space link   q quit   Q keep",
        Style::new().fg(DIM),
    ));
    frame.render_widget(Paragraph::new(help), area);
}
