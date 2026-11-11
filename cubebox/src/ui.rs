//! Screen layout and drawing.
//!
//! Four pages (timer / records / status / config), a 7-segment timer readout and
//! a wrapped scramble. Everything draws into the RAM canvas; the caller flushes.

use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_9X18};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::{Drawable, Point};
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::text::{Text, TextStyle};

use shared::timer::{Penalty, Phase};

use crate::display::Canvas;
use crate::state::{AppState, Page};

const BG: Rgb565 = Rgb565::BLACK;
const FG: Rgb565 = Rgb565::WHITE;
const DIM: Rgb565 = Rgb565::new(0x88, 0x88, 0x88);
const ACCENT: Rgb565 = Rgb565::new(0x3D, 0xD5, 0x6B);
const WARN: Rgb565 = Rgb565::new(0xFF, 0xC1, 0x07);
const BAD: Rgb565 = Rgb565::new(0xEF, 0x53, 0x50);

pub struct Ui {
    last_revision: u64,
    last_phase: Phase,
    last_elapsed_tenth: u32,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            last_revision: u64::MAX,
            last_phase: Phase::Scrambling,
            last_elapsed_tenth: u32::MAX,
        }
    }

    /// True when the state differs from what was last painted.
    ///
    /// The caller re-runs [`Ui::render`] for every display band (the canvas
    /// only holds one horizontal strip at a time).
    pub fn needs_redraw(&mut self, st: &AppState) -> bool {
        let phase = st.phase();
        let tenth = st.elapsed_ms() / 100;
        let changed = st.revision != self.last_revision
            || phase != self.last_phase
            || (phase == Phase::Solving && tenth != self.last_elapsed_tenth);
        if !changed {
            return false;
        }
        self.last_revision = st.revision;
        self.last_phase = phase;
        self.last_elapsed_tenth = tenth;
        true
    }

    /// Paint the whole screen into the canvas's current band. Coordinates are
    /// absolute; the canvas clips rows outside the band.
    pub fn render(&mut self, canvas: &mut Canvas, st: &AppState) {
        canvas.clear(BG);
        self.top_bar(canvas, st);
        match st.page {
            Page::Timer => self.page_timer(canvas, st),
            Page::Records => self.page_records(canvas, st),
            Page::Room => self.page_room(canvas, st),
            Page::Status => self.page_status(canvas, st),
            Page::Config => self.page_config(canvas, st),
        }
    }

    // ------------------------------------------------------------------ chrome

    fn top_bar(&self, canvas: &mut Canvas, st: &AppState) {
        canvas.rect(0, 0, 240, 22, Rgb565::new(0x18, 0x18, 0x18));

        let title_style = MonoTextStyle::new(&FONT_6X10, FG);
        let _ = Text::with_text_style(
            st.page.title(),
            Point::new(6, 15),
            title_style,
            TextStyle::default(),
        )
        .draw(canvas);

        // Right side: BLE dot + battery + pending uploads.
        let (dot_color, label) = if st.ble_connected {
            (ACCENT, "BLE")
        } else {
            (DIM, "---")
        };
        canvas.rect(150, 7, 8, 8, dot_color);
        let _ = Text::with_text_style(
            label,
            Point::new(161, 15),
            MonoTextStyle::new(&FONT_6X10, dot_color),
            TextStyle::default(),
        )
        .draw(canvas);

        if st.battery > 0 {
            let txt = format!("{}%", st.battery);
            let _ = Text::with_text_style(
                &txt,
                Point::new(196, 15),
                MonoTextStyle::new(&FONT_6X10, DIM),
                TextStyle::default(),
            )
            .draw(canvas);
        }

        if st.pending_uploads > 0 {
            canvas.rect(0, 22, 240, 2, WARN);
        }
    }

    // ------------------------------------------------------------------- pages

    fn page_timer(&self, canvas: &mut Canvas, st: &AppState) {
        // Scramble block.
        let scramble = st.scramble_text();
        let lines = wrap(&scramble, 26);
        let style = MonoTextStyle::new(&FONT_9X18, if st.session.is_some() { FG } else { DIM });
        for (i, line) in lines.iter().take(3).enumerate() {
            let _ = Text::with_text_style(
                line,
                Point::new(6, 46 + i as i32 * 20),
                style,
                TextStyle::default(),
            )
            .draw(canvas);
        }

        // Status line under the scramble.
        let phase_txt = match st.phase() {
            Phase::Scrambling => "SCRAMBLE",
            Phase::Ready => "READY",
            Phase::Solving => "SOLVING",
            Phase::Done => "DONE",
        };
        let phase_color = match st.phase() {
            Phase::Ready => ACCENT,
            Phase::Solving => WARN,
            Phase::Done => ACCENT,
            Phase::Scrambling => DIM,
        };
        let _ = Text::with_text_style(
            phase_txt,
            Point::new(6, 124),
            MonoTextStyle::new(&FONT_6X10, phase_color),
            TextStyle::default(),
        )
        .draw(canvas);

        // Timer readout.
        let ms = if st.phase() == Phase::Done {
            st.recent.first().map(|r| r.display_ms()).unwrap_or(0)
        } else {
            st.elapsed_ms()
        };
        let text = format_time(ms);
        self.seven_segment(canvas, &text, 10, 140, 92, FG);

        // Averages / best.
        let ao5 = st.average(5);
        let ao12 = st.average(12);
        let best = st.best();
        let stats = format!(
            "ao5 {:<8} ao12 {:<8}",
            fmt_centis(ao5),
            fmt_centis(ao12)
        );
        let _ = Text::with_text_style(
            &stats,
            Point::new(8, 258),
            MonoTextStyle::new(&FONT_6X10, DIM),
            TextStyle::default(),
        )
        .draw(canvas);
        let best_txt = format!("best {}", fmt_centis(best.map(|b| b as i64)));
        let _ = Text::with_text_style(
            &best_txt,
            Point::new(8, 274),
            MonoTextStyle::new(&FONT_6X10, DIM),
            TextStyle::default(),
        )
        .draw(canvas);

        // Last result.
        if let Some(row) = st.recent.first() {
            let (txt, color) = match row.penalty {
                Penalty::None => (format_time(row.duration_ms), FG),
                Penalty::PlusTwo => (format!("+2 {}", format_time(row.display_ms())), WARN),
                Penalty::Dnf => ("DNF".to_string(), BAD),
            };
            let _ = Text::with_text_style(
                &txt,
                Point::new(8, 296),
                MonoTextStyle::new(&FONT_6X10, color),
                TextStyle::default(),
            )
            .draw(canvas);
        }

        let hint = "S:run/stop  N:new  P:+2/DNF  M:page";
        let _ = Text::with_text_style(
            hint,
            Point::new(8, 312),
            MonoTextStyle::new(&FONT_6X10, DIM),
            TextStyle::default(),
        )
        .draw(canvas);
    }

    fn page_records(&self, canvas: &mut Canvas, st: &AppState) {
        let style = MonoTextStyle::new(&FONT_6X10, FG);
        let dim = MonoTextStyle::new(&FONT_6X10, DIM);
        if st.recent.is_empty() {
            let _ = Text::with_text_style(
                "no attempts yet",
                Point::new(8, 60),
                dim,
                TextStyle::default(),
            )
            .draw(canvas);
            return;
        }
        for (i, row) in st.recent.iter().take(12).enumerate() {
            let y = 40 + i as i32 * 18;
            let (time_txt, color) = match row.penalty {
                Penalty::None => (format_time(row.duration_ms), FG),
                Penalty::PlusTwo => (format!("+2 {}", format_time(row.display_ms())), WARN),
                Penalty::Dnf => ("DNF".to_string(), BAD),
            };
            let _ = Text::with_text_style(
                &format!("{:>2}", i + 1),
                Point::new(6, y),
                dim,
                TextStyle::default(),
            )
            .draw(canvas);
            let _ = Text::with_text_style(
                &time_txt,
                Point::new(26, y),
                MonoTextStyle::new(&FONT_6X10, color),
                TextStyle::default(),
            )
            .draw(canvas);
            let scr = truncate(&row.scramble, 12);
            let _ = Text::with_text_style(
                &scr,
                Point::new(110, y),
                dim,
                TextStyle::default(),
            )
            .draw(canvas);
        }
        let _ = Text::with_text_style(
            &format!("{} attempts stored", st.recent.len()),
            Point::new(8, 300),
            style,
            TextStyle::default(),
        )
        .draw(canvas);
    }

    fn page_room(&self, canvas: &mut Canvas, st: &AppState) {
        let dim = MonoTextStyle::new(&FONT_6X10, DIM);
        let fg = MonoTextStyle::new(&FONT_6X10, FG);

        let Some(room) = st.room.as_ref() else {
            let _ = Text::with_text_style(
                "No room",
                Point::new(8, 50),
                MonoTextStyle::new(&FONT_9X18, FG),
                TextStyle::default(),
            )
            .draw(canvas);
            for (i, line) in [
                "S -> host a room",
                "N -> join first room",
                "P -> back",
            ]
            .iter()
            .enumerate()
            {
                let _ = Text::with_text_style(
                    line,
                    Point::new(8, 90 + i as i32 * 18),
                    dim,
                    TextStyle::default(),
                )
                .draw(canvas);
            }
            return;
        };

        // Header: room id + role.
        let role = if room.hosting { "HOST" } else { "GUEST" };
        let _ = Text::with_text_style(
            &format!("ROOM {:08X} {role}", room.room_id),
            Point::new(6, 42),
            MonoTextStyle::new(&FONT_6X10, ACCENT),
            TextStyle::default(),
        )
        .draw(canvas);

        let _ = Text::with_text_style(
            &format!("BO{} R{}/{} {}", room.rounds, room.round, room.rounds, room.phase),
            Point::new(6, 58),
            fg,
            TextStyle::default(),
        )
        .draw(canvas);

        // Player rows (name, wins, ready, last result).
        for (i, p) in room.players.iter().take(5).enumerate() {
            let y = 82 + i as i32 * 24;
            let marker = if p.is_me { "*" } else { " " };
            let name = format!("{marker}{:<8}", truncate(&p.name, 8));
            let _ = Text::with_text_style(&name, Point::new(6, y), fg, TextStyle::default())
                .draw(canvas);

            let _ = Text::with_text_style(
                &format!("W{}", p.wins),
                Point::new(110, y),
                MonoTextStyle::new(&FONT_6X10, WARN),
                TextStyle::default(),
            )
            .draw(canvas);

            let (state_txt, color) = if p.dnf {
                ("DNF".to_string(), BAD)
            } else if p.last_ms > 0 {
                (format_time(p.last_ms), FG)
            } else if p.ready {
                ("READY".to_string(), ACCENT)
            } else {
                ("--".to_string(), DIM)
            };
            let _ = Text::with_text_style(
                &state_txt,
                Point::new(148, y),
                MonoTextStyle::new(&FONT_6X10, color),
                TextStyle::default(),
            )
            .draw(canvas);
        }

        if let Some(w) = &room.winner {
            let _ = Text::with_text_style(
                &format!("WINNER: {w}"),
                Point::new(8, 232),
                MonoTextStyle::new(&FONT_9X18, ACCENT),
                TextStyle::default(),
            )
            .draw(canvas);
        }

        if !room.note.is_empty() {
            let _ = Text::with_text_style(
                &truncate(&room.note, 26),
                Point::new(8, 254),
                dim,
                TextStyle::default(),
            )
            .draw(canvas);
        }

        let hint = if room.hosting {
            "S:start  N:join  P:leave"
        } else {
            "N:join  P:leave"
        };
        let _ = Text::with_text_style(hint, Point::new(8, 290), dim, TextStyle::default())
            .draw(canvas);
    }

    fn page_status(&self, canvas: &mut Canvas, st: &AppState) {        let rows = [
            ("cube", if st.ble_connected { st.ble_name.clone() } else { "-".into() }),
            ("battery", format!("{}%", st.battery)),
            ("wifi", if st.wifi_ok { "connected".into() } else { "offline".into() }),
            (
                "account",
                if st.logged_in {
                    format!("{} ({})", st.nick, st.user_id)
                } else {
                    "not logged in".into()
                },
            ),
            ("pending", format!("{} record(s)", st.pending_uploads)),
            ("group", format!("type {} id {}", st.group_type.as_str(), st.group_id)),
        ];
        for (i, (k, v)) in rows.iter().enumerate() {
            let y = 44 + i as i32 * 22;
            let _ = Text::with_text_style(
                k,
                Point::new(8, y),
                MonoTextStyle::new(&FONT_6X10, DIM),
                TextStyle::default(),
            )
            .draw(canvas);
            let _ = Text::with_text_style(
                v,
                Point::new(70, y),
                MonoTextStyle::new(&FONT_6X10, FG),
                TextStyle::default(),
            )
            .draw(canvas);
        }
        if !st.status_msg.is_empty() {
            for (i, line) in wrap(&st.status_msg, 26).iter().take(4).enumerate() {
                let _ = Text::with_text_style(
                    line,
                    Point::new(8, 200 + i as i32 * 16),
                    MonoTextStyle::new(&FONT_6X10, WARN),
                    TextStyle::default(),
                )
                .draw(canvas);
            }
        }
    }

    fn page_config(&self, canvas: &mut Canvas, st: &AppState) {
        let lines = [
            "Provisioning:",
            "1. join AP \"GanCubeEmu\"",
            "2. open http://192.168.4.1",
            "3. save WiFi + log in",
            "",
            "Timer groups:",
            "S+N  cycle group",
            "",
            &format!("api: prod gateway"),
            &format!("user: {}", if st.logged_in { st.user_id.to_string() } else { "-".into() }),
        ];
        for (i, line) in lines.iter().enumerate() {
            let color = if i == 5 { ACCENT } else { FG };
            let _ = Text::with_text_style(
                line,
                Point::new(8, 42 + i as i32 * 18),
                MonoTextStyle::new(&FONT_6X10, color),
                TextStyle::default(),
            )
            .draw(canvas);
        }
    }

    // ----------------------------------------------------------- 7-segment digits

    /// Draw a time string ("12.345", "1:02.345", "DNF") with 7-segment digits.
    fn seven_segment(&self, canvas: &mut Canvas, text: &str, x: i32, y: i32, h: u32, color: Rgb565) {
        if text == "DNF" {
            let _ = Text::with_text_style(
                "DNF",
                Point::new(x + 10, y + 20),
                MonoTextStyle::new(&FONT_9X18, BAD),
                TextStyle::default(),
            )
            .draw(canvas);
            return;
        }
        let w = (h * 55) / 100; // classic 7-seg aspect
        let gap = (w / 4).max(4) as i32;
        let mut cx = x;
        for ch in text.chars() {
            match ch {
                ':' => {
                    let d = (h / 8).max(4) as u32;
                    canvas.rect(cx, y + (h as i32) / 3, d, d, color);
                    canvas.rect(cx, y + (h as i32) * 2 / 3, d, d, color);
                    cx += d as i32 + gap;
                }
                '.' => {
                    let d = (h / 8).max(4) as u32;
                    canvas.rect(cx, y + h as i32 - d as i32, d, d, color);
                    cx += d as i32 + gap;
                }
                d @ '0'..='9' => {
                    draw_digit(canvas, cx, y, w, h, d.to_digit(10).unwrap() as u8, color);
                    cx += w as i32 + gap;
                }
                _ => {}
            }
        }
    }
}

fn draw_digit(canvas: &mut Canvas, x: i32, y: i32, w: u32, h: u32, digit: u8, color: Rgb565) {
    // Segment map: a,b,c,d,e,f,g
    const SEGMENTS: [[bool; 7]; 10] = [
        [true, true, true, true, true, true, false],    // 0
        [false, true, true, false, false, false, false], // 1
        [true, true, false, true, true, false, true],   // 2
        [true, true, true, true, false, false, true],   // 3
        [false, true, true, false, false, true, true],  // 4
        [true, false, true, true, false, true, true],   // 5
        [true, false, true, true, true, true, true],    // 6
        [true, true, true, false, false, false, false], // 7
        [true, true, true, true, true, true, true],     // 8
        [true, true, true, true, false, true, true],    // 9
    ];
    let seg = SEGMENTS[(digit as usize) % 10];
    let t = (w / 5).max(3); // thickness
    let h_i = h as i32;
    let w_i = w as i32;
    let hh = h / 2;

    // a: top horizontal
    if seg[0] {
        canvas.rect(x + t as i32, y, w.saturating_sub(2 * t), t, color);
    }
    // b: top right vertical
    if seg[1] {
        canvas.rect(x + w_i - t as i32, y + t as i32, t, hh.saturating_sub(t), color);
    }
    // c: bottom right vertical
    if seg[2] {
        canvas.rect(
            x + w_i - t as i32,
            y + hh as i32,
            t,
            hh.saturating_sub(t),
            color,
        );
    }
    // d: bottom horizontal
    if seg[3] {
        canvas.rect(
            x + t as i32,
            y + h_i - t as i32,
            w.saturating_sub(2 * t),
            t,
            color,
        );
    }
    // e: bottom left vertical
    if seg[4] {
        canvas.rect(x, y + hh as i32, t, hh.saturating_sub(t), color);
    }
    // f: top left vertical
    if seg[5] {
        canvas.rect(x, y + t as i32, t, hh.saturating_sub(t), color);
    }
    // g: middle horizontal
    if seg[6] {
        canvas.rect(
            x + t as i32,
            y + hh as i32 - t as i32 / 2,
            w.saturating_sub(2 * t),
            t,
            color,
        );
    }
}

// ------------------------------------------------------------------- helpers

/// Format milliseconds as `12.345` or `1:02.345`.
pub fn format_time(ms: u32) -> String {
    let total_ms = ms;
    let minutes = total_ms / 60_000;
    let seconds = (total_ms / 1000) % 60;
    let millis = total_ms % 1000;
    if minutes > 0 {
        format!("{}:{:02}.{:03}", minutes, seconds, millis)
    } else {
        format!("{}.{:03}", seconds, millis)
    }
}

fn fmt_centis(v: Option<i64>) -> String {
    match v {
        None => "  --  ".to_string(),
        Some(c) => {
            if c >= 60_000 {
                format!("{}:{:05.2}", c / 6000, (c % 6000) as f64 / 100.0)
            } else {
                format!("{}.{:02}", c / 100, c % 100)
            }
        }
    }
}

/// Greedy wrap on whitespace.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
        } else if line.len() + 1 + word.len() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push(line.clone());
            line.clear();
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}
