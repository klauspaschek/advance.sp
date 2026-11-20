//! The button checker's face: an idealized RG SP or Xbox controller whose buttons light up while
//! they are held, the buttons slot receives named beneath, and the footer's legends.

use crate::bitmaps::ButtonLegend;
use crate::quick_menu::{button_face, with_hint, MENU_HINT_H};
use crate::{Canvas, UndoFace, OUT_H};

/// What the checker shows.
pub struct ButtonTestView<'a> {
    /// Draw the Xbox controller instead of the RG SP: a controller is connected and was the
    /// last to be pressed.
    pub controller: bool,
    /// The controller's face buttons follow the Nintendo layout (its east button is A).
    pub nintendo: bool,
    /// What is held, by the names slot gives them: `A`, `Up`, `L1`, `Menu`, `Vol+`, `Share`,
    /// `L3`, `RS left`, `LS up`...
    pub held: &'a [&'a str],
}

const LIT: &str = "#46c3ad";
const DRAWING_W: f32 = 600.0;
const DRAWING_H: f32 = 280.0;

fn hex(ink: [u8; 3], alpha: f32) -> String {
    format!("rgba({},{},{},{alpha})", ink[0], ink[1], ink[2])
}

/// A shape of the drawing, lit or not, with the label drawn on it afterwards.
struct Part {
    svg: String,
    label: Option<(&'static str, f32, f32, f32)>,
}

struct Style {
    idle: String,
    edge: String,
}

impl Style {
    fn fill(&self, lit: bool) -> String {
        if lit {
            format!(r#"fill="{LIT}" stroke="{LIT}""#)
        } else {
            format!(r#"fill="{}" stroke="{}""#, self.idle, self.edge)
        }
    }
}

fn rect(style: &Style, lit: bool, x: f32, y: f32, w: f32, h: f32, r: f32, label: Option<&'static str>) -> Part {
    Part {
        svg: format!(r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" stroke-width="2" {}/>"#, style.fill(lit)),
        label: label.map(|l| (l, x + w / 2.0, y + h / 2.0, (h * 0.62).min(20.0))),
    }
}

fn circle(style: &Style, lit: bool, cx: f32, cy: f32, r: f32, label: Option<&'static str>) -> Part {
    Part {
        svg: format!(r#"<circle cx="{cx}" cy="{cy}" r="{r}" stroke-width="2" {}/>"#, style.fill(lit)),
        label: label.map(|l| (l, cx, cy, (r * 1.1).min(20.0))),
    }
}

/// A label on its own, beside a button too small to carry one.
fn caption(label: &'static str, x: f32, y: f32, px: f32) -> Part {
    Part { svg: String::new(), label: Some((label, x, y, px)) }
}

/// A D-pad of arm width `arm` centred at `(cx, cy)`.
fn dpad(style: &Style, held: &dyn Fn(&str) -> bool, cx: f32, cy: f32, arm: f32) -> Vec<Part> {
    let h = arm / 2.0;
    let mut parts = vec![rect(style, false, cx - h, cy - h, arm, arm, 0.0, None)];
    for (name, dx, dy) in [("Up", 0.0, -1.0), ("Down", 0.0, 1.0), ("Left", -1.0, 0.0), ("Right", 1.0, 0.0)] {
        parts.push(rect(style, held(name), cx - h + dx * arm, cy - h + dy * arm, arm, arm, 4.0, None));
    }
    parts
}

/// A stick: its ring, lit while pressed in, and the knob pushed the way it is held.
fn stick(style: &Style, held: &dyn Fn(&str) -> bool, side: &str, click: &str, cx: f32, cy: f32) -> Vec<Part> {
    let dir = |d: &str| held(&format!("{side} {d}"));
    let dx = f32::from(u8::from(dir("right"))) - f32::from(u8::from(dir("left")));
    let dy = f32::from(u8::from(dir("down"))) - f32::from(u8::from(dir("up")));
    let moved = dx != 0.0 || dy != 0.0;
    // pressed in, the ring lights and the knob stays visible on it
    let ring = if held(click) {
        format!(r#"<circle cx="{cx}" cy="{cy}" r="30" fill="{LIT}" fill-opacity="0.35" stroke="{LIT}" stroke-width="4"/>"#)
    } else {
        format!(r#"<circle cx="{cx}" cy="{cy}" r="30" stroke-width="2" {}/>"#, style.fill(false))
    };
    vec![
        Part { svg: ring, label: None },
        circle(style, moved, cx + dx * 12.0, cy + dy * 12.0, 17.0, None),
    ]
}

fn handheld(style: &Style, held: &dyn Fn(&str) -> bool, ink: [u8; 3]) -> (String, Vec<Part>) {
    let body = format!(
        r#"<rect x="20" y="34" width="560" height="232" rx="42" fill="{}" stroke="{}" stroke-width="3"/><rect x="60" y="24" width="480" height="16" rx="8" fill="{}"/>"#,
        hex(ink, 0.06),
        hex(ink, 0.35),
        hex(ink, 0.18)
    );
    let mut parts = vec![
        // as on the RG SP: L1 and R1 at the corners, L2 and R2 inside them
        rect(style, held("L1"), 36.0, 4.0, 72.0, 26.0, 9.0, Some("L1")),
        rect(style, held("L2"), 116.0, 4.0, 72.0, 26.0, 9.0, Some("L2")),
        rect(style, held("R2"), 412.0, 4.0, 72.0, 26.0, 9.0, Some("R2")),
        rect(style, held("R1"), 492.0, 4.0, 72.0, 26.0, 9.0, Some("R1")),
        rect(style, held("Vol+"), 6.0, 96.0, 12.0, 36.0, 5.0, None),
        rect(style, held("Vol-"), 6.0, 140.0, 12.0, 36.0, 5.0, None),
        caption("+", 36.0, 114.0, 18.0),
        caption("−", 36.0, 158.0, 18.0),
    ];
    parts.extend(dpad(style, held, 130.0, 140.0, 34.0));
    for (name, cx, cy) in [("X", 470.0, 98.0), ("A", 512.0, 140.0), ("B", 470.0, 182.0), ("Y", 428.0, 140.0)] {
        parts.push(circle(style, held(name), cx, cy, 20.0, Some(name)));
    }
    parts.push(circle(style, held("Menu"), 300.0, 70.0, 12.0, None));
    parts.push(caption("MENU", 300.0, 95.0, 12.0));
    parts.push(caption("SELECT", 262.0, 212.0, 12.0));
    parts.push(circle(style, held("Select"), 262.0, 236.0, 11.0, None));
    parts.push(caption("START", 338.0, 212.0, 12.0));
    parts.push(circle(style, held("Start"), 338.0, 236.0, 11.0, None));
    (body, parts)
}

/// The Xbox button, after the Xbox logo: a disc crossed by two curved strokes.
fn xbox_button(style: &Style, lit: bool, cx: f32, cy: f32, ink: [u8; 3]) -> Part {
    let cross = if lit { "#ffffff".to_owned() } else { hex(ink, 0.75) };
    Part {
        svg: format!(
            r#"<circle cx="{cx}" cy="{cy}" r="16" stroke-width="2" {}/><path d="M{} {} Q{cx} {} {} {} M{} {} Q{cx} {} {} {}" fill="none" stroke="{cross}" stroke-width="3.2" stroke-linecap="round"/>"#,
            style.fill(lit),
            cx - 8.5, cy - 10.0, cy + 1.0, cx + 8.5, cy + 10.0,
            cx + 8.5, cy - 10.0, cy + 1.0, cx - 8.5, cy + 10.0,
        ),
        label: None,
    }
}

fn xbox(style: &Style, held: &dyn Fn(&str) -> bool, nintendo: bool, ink: [u8; 3]) -> (String, Vec<Part>) {
    let body = format!(
        r#"<path d="M150 62 Q300 44 450 62 Q544 74 570 172 Q588 262 520 266 Q480 268 440 217 L160 217 Q120 268 80 266 Q12 262 30 172 Q56 74 150 62 Z" fill="{}" stroke="{}" stroke-width="3"/>"#,
        hex(ink, 0.06),
        hex(ink, 0.35)
    );
    let mut parts = vec![
        // the controller's own names: the line beneath says what slot makes of them
        rect(style, held("L2"), 130.0, 2.0, 72.0, 24.0, 9.0, Some("LT")),
        rect(style, held("R2"), 398.0, 2.0, 72.0, 24.0, 9.0, Some("RT")),
        rect(style, held("L1"), 104.0, 32.0, 120.0, 22.0, 11.0, Some("LB")),
        rect(style, held("R1"), 376.0, 32.0, 120.0, 22.0, 11.0, Some("RB")),
    ];
    parts.extend(stick(style, held, "LS", "L3", 165.0, 118.0));
    parts.extend(dpad(style, held, 232.0, 172.0, 22.0));
    parts.extend(stick(style, held, "RS", "R3", 368.0, 172.0));
    // the face buttons where they sit on an Xbox pad, lit by the button slot makes of them
    for (label, nin, lab, cx, cy) in [
        ("Y", "X", "Y", 446.0, 86.0),
        ("B", "A", "B", 476.0, 116.0),
        ("A", "B", "A", 446.0, 146.0),
        ("X", "Y", "X", 416.0, 116.0),
    ] {
        parts.push(circle(style, held(if nintendo { nin } else { lab }), cx, cy, 15.0, Some(label)));
    }
    // the controller's own names (View is slot's Select, Menu its Start, the Xbox button its
    // MENU): the line beneath says what slot makes of each press
    parts.push(circle(style, held("Select"), 256.0, 120.0, 10.0, None));
    parts.push(caption("View", 256.0, 102.0, 12.0));
    parts.push(circle(style, held("Start"), 344.0, 120.0, 10.0, None));
    parts.push(caption("Menu", 344.0, 102.0, 12.0));
    parts.push(xbox_button(style, held("Menu"), 300.0, 84.0, ink));
    parts.push(rect(style, held("Share"), 288.0, 132.0, 24.0, 12.0, 6.0, None));
    parts.push(caption("Share", 300.0, 154.0, 11.0));
    (body, parts)
}

fn blit_over(dst: &mut [u8], dst_w: u32, dst_h: u32, src: &UndoFace, x: i32, y: i32) {
    for sy in 0..src.h as i32 {
        let dy = y + sy;
        if dy < 0 || dy >= dst_h as i32 {
            continue;
        }
        for sx in 0..src.w as i32 {
            let dx = x + sx;
            if dx < 0 || dx >= dst_w as i32 {
                continue;
            }
            let s = ((sy as u32 * src.w + sx as u32) * 4) as usize;
            let d = ((dy as u32 * dst_w + dx as u32) * 4) as usize;
            let a = u32::from(src.rgba[s + 3]);
            if a == 0 {
                continue;
            }
            for c in 0..3 {
                dst[d + c] = ((u32::from(src.rgba[s + c]) * a + u32::from(dst[d + c]) * (255 - a)) / 255) as u8;
            }
            dst[d + 3] = (a + u32::from(dst[d + 3]) * (255 - a) / 255) as u8;
        }
    }
}

/// Text centred at `(cx, cy)` of the face.
fn text(dst: &mut [u8], dst_w: u32, dst_h: u32, label: &str, cx: f32, cy: f32, px: f32, ink: [u8; 3], role: crate::text::Role) {
    let Some(font) = crate::text::font(role).or_else(crate::text::label_font) else {
        return;
    };
    let layout = crate::text::fit_preserving_case(font, label, 400.0, 1, px, px * 0.6);
    let w = (crate::text::line_width(font, layout.lines.first().map_or("", |l| l.as_str()), layout.px, layout.tracking).ceil() as u32 + 4).max(1);
    let h = (px * 1.6).ceil() as u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    crate::text::draw_centred(&mut rgba, w, h, &layout, ink);
    let face = UndoFace { rgba, w, h };
    blit_over(dst, dst_w, dst_h, &face, (cx - w as f32 / 2.0).round() as i32, (cy - h as f32 / 2.0).round() as i32);
}

/// The checker's whole face, the size of the canvas: title, drawing, held buttons and legends.
pub fn button_test_face(view: &ButtonTestView, ink: [u8; 3], canvas: Canvas) -> UndoFace {
    let w = canvas.w.round() as u32;
    let h = OUT_H;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let title = crate::quick_heading_face("Test buttons");
    blit_over(&mut rgba, w, h, &title, (w.saturating_sub(title.w) / 2) as i32, 0);

    let held = |name: &str| view.held.contains(&name);
    let style = Style { idle: hex(ink, 0.10), edge: hex(ink, 0.45) };
    let (body, parts) = if view.controller { xbox(&style, &held, view.nintendo, ink) } else { handheld(&style, &held, ink) };

    // the drawing fills the card between the title and the held buttons, keeping its shape
    let legend_y = h as f32 - crate::MOUTH_H - MENU_HINT_H as f32 - 12.0;
    // everything stays inside the settings card: the held buttons' line sits at one height,
    // under the controller's grips or under the handheld, whose drawing gives up a little height
    let card_bottom = crate::expressive::CARD_BOTTOM;
    let top = 66.0;
    let bottom = if view.controller { card_bottom - 8.0 } else { card_bottom - 40.0 };
    let scale = ((w as f32 - 120.0) / DRAWING_W).min((bottom - top) / DRAWING_H);
    let (dw, dh) = ((DRAWING_W * scale).round() as u32, (DRAWING_H * scale).round() as u32);
    let (ox, oy) = ((w - dw) / 2, top as u32);
    let mut svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {DRAWING_W} {DRAWING_H}" width="{dw}" height="{dh}">{body}"#);
    for part in &parts {
        svg.push_str(&part.svg);
    }
    svg.push_str("</svg>");
    if let Some(drawing) = crate::art::render_svg(&svg, dw, dh) {
        blit_over(&mut rgba, w, h, &UndoFace { rgba: drawing, w: dw, h: dh }, ox as i32, oy as i32);
    }
    for part in &parts {
        if let Some((label, x, y, px)) = part.label {
            let lit = part.svg.contains(LIT);
            let colour = if lit { [255, 255, 255] } else { ink };
            text(&mut rgba, w, h, label, ox as f32 + x * scale, oy as f32 + y * scale, px * scale, colour, crate::text::Role::Detail);
        }
    }

    // what slot receives, by name
    let names: Vec<&str> = view.held.iter().copied().filter(|n| !n.starts_with("LS ") && !n.starts_with("RS ")).collect();
    if !names.is_empty() {
        // the same height for both drawings, clear of the card's lower edge
        let line_y = card_bottom - 30.0;
        text(&mut rgba, w, h, &names.join("  "), w as f32 / 2.0, line_y, 22.0, ink, crate::text::Role::Focus);
    }

    // the footer: holding B leaves, every other press is only shown
    let back = with_hint(button_face(ButtonLegend::B), "Hold · Back", Some(ink), true);
    blit_over(&mut rgba, w, h, &back, 16, legend_y as i32);
    let any = crate::plate::word_ink("Press any button", ink);
    blit_over(&mut rgba, w, h, &any, w as i32 - 16 - any.w as i32 - crate::HINT_EDGE as i32, legend_y as i32 + (MENU_HINT_H as i32 - any.h as i32) / 2);
    UndoFace { rgba, w, h }
}
