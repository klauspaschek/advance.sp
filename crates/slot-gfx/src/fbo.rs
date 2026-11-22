use crate::draw::{Draw, Sprites, TexId};
use crate::grade::blue_light_gain;
use crate::pipeline::GamePass;
use crate::quad::Quad;
use crate::shaders::{BLIT_FRAG, BLIT_VERT, OVERLAY_FRAG, TURN_VERT};
use crate::surface::{blit_rect, GfxError, Surface, OUT_H, OUT_W};
use crate::Canvas;

pub const BACKDROP: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

struct Turn {
    panel: (u32, u32),
    prog: gl::types::GLuint,
    fbo: gl::types::GLuint,
    tex: gl::types::GLuint,
    size: (u32, u32),
}

/// Where the window's picture goes on a framebuffer larger than the window: on a TV through
/// HDMI, the framebuffer is 16:9 and the handheld's 3:2 screen sits in the middle of it, or fills
/// it for a game set to Stretch. `rect` is in GL's framebuffer coordinates (y up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Letterbox {
    pub surface: (u32, u32),
    pub rect: (i32, i32, i32, i32),
}

/// The handheld's `window` on a `surface` of another shape: as large as it fits at its own
/// shape, centred, or the whole surface when `fill`.
pub fn letterbox(surface: (u32, u32), window: (u32, u32), fill: bool) -> Letterbox {
    let (sw, sh) = (surface.0.max(1), surface.1.max(1));
    if fill {
        return Letterbox {
            surface,
            rect: (0, 0, sw as i32, sh as i32),
        };
    }
    let (ww, wh) = (window.0.max(1) as f32, window.1.max(1) as f32);
    let scale = (sw as f32 / ww).min(sh as f32 / wh);
    let (w, h) = ((ww * scale).round() as i32, (wh * scale).round() as i32);
    Letterbox {
        surface,
        rect: ((sw as i32 - w) / 2, (sh as i32 - h) / 2, w, h),
    }
}

pub struct Compositor {
    fbo: gl::types::GLuint,
    tex: gl::types::GLuint,
    size: (u32, u32),
    blit: gl::types::GLuint,
    u_gain: gl::types::GLint,
    u_blit_uv: gl::types::GLint,
    gain: [f32; 3],
    shake: f32,
    window: (u32, u32),
    screen: f32,
    quad: Quad,
    game: GamePass,
    sprites: Sprites,
    game_bezel: Option<TexId>,
    sp_frame: bool,
    game_destination: Option<[f32;4]>,
    turn: Option<Turn>,
    letterbox: Option<Letterbox>,
    /// The TV's bezel, across the whole framebuffer, and where the game's picture is on the
    /// window (left uncovered); see [`Compositor::set_tv_bezel`].
    tv_bezel: Option<gl::types::GLuint>,
    tv_picture: Option<[f32; 4]>,
    overlay: gl::types::GLuint,
    u_overlay_gain: gl::types::GLint,
    u_overlay_uv: gl::types::GLint,
    captured: Option<(u32, u32, Vec<u8>)>,
}

impl Compositor {
    pub fn new(surface: &dyn Surface) -> Result<Self, GfxError> {
        crate::gl::load(surface);
        let blit = crate::shaders::program(BLIT_VERT, BLIT_FRAG)?;
        let overlay = crate::shaders::program(BLIT_VERT, OVERLAY_FRAG)?;
        let tex = crate::gl::texture(OUT_W, OUT_H, gl::NEAREST, gl::CLAMP_TO_EDGE, gl::RGBA, None);
        unsafe {
            let mut fbo = 0;
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                tex,
                0,
            );
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            if status != gl::FRAMEBUFFER_COMPLETE {
                gl::DeleteFramebuffers(1, &fbo);
                gl::DeleteTextures(1, &tex);
                gl::DeleteProgram(blit);
                return Err(GfxError::Framebuffer(status));
            }
            gl::UseProgram(blit);
            gl::Uniform1i(crate::gl::uniform_location(blit, "u_tex"), 0);
            let u_gain = crate::gl::uniform_location(blit, "u_gain");
            let u_blit_uv = crate::gl::uniform_location(blit, "u_uv");
            gl::Uniform4f(u_blit_uv, 0.0, 0.0, 1.0, 1.0);
            gl::UseProgram(overlay);
            gl::Uniform1i(crate::gl::uniform_location(overlay, "u_tex"), 0);
            let u_overlay_gain = crate::gl::uniform_location(overlay, "u_gain");
            let u_overlay_uv = crate::gl::uniform_location(overlay, "u_uv");

            Ok(Compositor {
                fbo,
                tex,
                size: (OUT_W, OUT_H),
                blit,
                u_gain,
                u_blit_uv,
                gain: blue_light_gain(0),
                shake: 0.0,
                window: (OUT_W, OUT_H),
                screen: 0.0,
                quad: Quad::new(),
                game: GamePass::new()?,
                sprites: Sprites::new()?,
                game_bezel: None,
                sp_frame: true,
                game_destination: None,
                turn: None,
                letterbox: None,
                tv_bezel: None,
                tv_picture: None,
                overlay,
                u_overlay_gain,
                u_overlay_uv,
                captured: None,
            })
        }
    }

    pub fn set_panel_turn(&mut self, panel: (u32, u32), ccw: bool) -> Result<(), GfxError> {
        let prog = crate::shaders::program(TURN_VERT, BLIT_FRAG)?;
        unsafe {
            gl::UseProgram(prog);
            gl::Uniform1i(crate::gl::uniform_location(prog, "u_tex"), 0);
            gl::Uniform3f(crate::gl::uniform_location(prog, "u_gain"), 1.0, 1.0, 1.0);
            gl::Uniform1f(
                crate::gl::uniform_location(prog, "u_ccw"),
                if ccw { 1.0 } else { 0.0 },
            );
        }
        self.turn = Some(Turn {
            panel,
            prog,
            fbo: 0,
            tex: 0,
            size: (0, 0),
        });
        Ok(())
    }

    /// Places the window on a larger framebuffer (see [`Letterbox`]); `None` draws it at the
    /// framebuffer's own size, as on the handheld's screen.
    pub fn set_letterbox(&mut self, letterbox: Option<Letterbox>) {
        self.letterbox = letterbox;
    }

    /// The framebuffer's size while the window is letterboxed on it (a TV), else `None`.
    pub fn surface(&self) -> Option<(u32, u32)> {
        self.letterbox.map(|lb| lb.surface)
    }

    /// The TV's bezel, `w` x `h` RGBA rows from the top, drawn across the whole framebuffer of a
    /// letterbox; `None` removes it.
    pub fn set_tv_bezel(&mut self, image: Option<(u32, u32, &[u8])>) {
        if let Some(tex) = self.tv_bezel.take() {
            unsafe { gl::DeleteTextures(1, &tex) };
        }
        self.tv_bezel = image.map(|(w, h, rgba)| {
            crate::gl::texture(w, h, gl::LINEAR, gl::CLAMP_TO_EDGE, gl::RGBA, Some(rgba))
        });
    }

    /// Where the game's picture is on the window (x, y, w, h from the top left) while the TV's
    /// bezel shows around it; `None` hides the bezel.
    pub fn set_tv_picture(&mut self, picture: Option<[f32; 4]>) {
        self.tv_picture = picture;
    }

    /// A rectangle of the window, in GL coordinates, where it lands on the framebuffer `target`.
    fn placed(
        &self,
        window: (u32, u32),
        target: gl::types::GLuint,
        r: (i32, i32, i32, i32),
    ) -> (i32, i32, i32, i32) {
        match self.letterbox {
            Some(lb) if target == 0 => place(lb, window, r),
            _ => r,
        }
    }

    pub fn upload_game(&mut self, xrgb8888: &[u8]) {
        self.game.upload(xrgb8888);
    }

    pub fn draw_game(&mut self) {
        self.in_game_rect(|c| c.game.draw(&c.quad));
    }

    pub fn draw_list(&mut self, items: &[Draw]) {
        let mut from = 0;
        for (i, item) in items.iter().enumerate() {
            match *item {
                Draw::Game => {
                    self.sprites.draw(&items[from..i], &self.quad);
                    self.in_game_rect(|c| {
                        c.game.draw(&c.quad);
                        if c.panel_bezel().is_none() {
                            c.draw_game_bezel();
                        }
                    });
                    if let Some(bezel) = self.panel_bezel() {
                        self.sprites.draw(&[bezel], &self.quad);
                    }
                }
                Draw::Shot { tex } => {
                    self.sprites.draw(&items[from..i], &self.quad);
                    if let Some(tex) = self.sprites.source(tex) {
                        self.in_game_rect(|c| c.game.draw_still(tex, &c.quad));
                    }
                }
                Draw::Capture => {
                    self.sprites.draw(&items[from..i], &self.quad);
                    let (w, h) = self.size;
                    self.captured = Some((w, h, self.read_frame()));
                }
                _ => continue,
            }
            from = i + 1;
        }
        self.sprites.draw(&items[from..], &self.quad);
    }

    pub fn create_texture(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.sprites.create_texture(w, h, rgba)
    }

    pub fn create_ui_texture(&mut self,w:u32,h:u32,rgba:&[u8])->TexId {self.sprites.create_ui_texture(w,h,rgba)}
    pub fn update_ui_texture(&mut self,id:TexId,w:u32,h:u32,rgba:&[u8]){self.sprites.update_ui_texture(id,w,h,rgba);}
    pub fn create_background_texture(&mut self,w:u32,h:u32,rgba:&[u8])->TexId {self.sprites.create_background_texture(w,h,rgba)}
    pub fn update_background_texture(&mut self,id:TexId,w:u32,h:u32,rgba:&[u8]){self.sprites.update_background_texture(id,w,h,rgba);}
    pub fn mark_hint_texture(&mut self,id:TexId,side:usize){self.sprites.mark_hint_texture(id,side);}
    pub fn set_hint_light(&mut self,light:[bool;2]){self.sprites.set_hint_light(light);}
    pub fn mark_backdrop_texture(&mut self,id:TexId,zone:usize){self.sprites.mark_backdrop_texture(id,zone);}
    pub fn set_backdrop_light(&mut self,light:[bool;3]){self.sprites.set_backdrop_light(light);}
    pub fn set_light_ui(&mut self,light:bool){self.sprites.set_light_ui(light);}

    pub fn create_texture_nearest(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.sprites.create_texture_nearest(w, h, rgba)
    }

    pub fn update_texture(&mut self, id: TexId, w: u32, h: u32, rgba: &[u8]) {
        self.sprites.update_texture(id, w, h, rgba);
    }

    pub fn set_blue_light(&mut self, step: u8) {
        self.gain = blue_light_gain(step);
    }

    pub fn set_screen_effect(&mut self, effect: crate::ScreenEffect) {
        self.game.set_effect(effect);
    }

    pub fn set_picture(&mut self, rect: [f32; 4]) {
        self.game.set_picture(rect);
    }

    pub fn set_game_bezel(&mut self, tex: Option<TexId>) {
        self.game_bezel = tex;
    }

    pub fn set_sp_frame(&mut self, on: bool) {
        self.sp_frame = on;
    }

    fn draw_game_bezel(&mut self) {
        let Some(tex) = self.game_bezel else {
            return;
        };
        let (x, y, w, h) = self.game.frame();
        self.sprites.set_target(Canvas::FRAMED);
        self.sprites.draw(
            &[Draw::Tex {
                x,
                y,
                w,
                h,
                tex,
                alpha: 1.0,
            }],
            &self.quad,
        );
        self.sprites.set_target(Canvas::of(self.window));
    }

    pub fn set_paper(&mut self, size: u32, gray: &[u8]) {
        self.game.set_paper(size, gray);
    }

    pub fn fit(&mut self, window: (u32, u32)) {
        self.window = window;
        self.resize(canvas_size(window));
    }

    fn native(&self) -> bool {
        self.game_destination.is_some() || crate::bezel::framed(self.window)
    }

    fn resize(&mut self, size: (u32, u32)) {
        if size == self.size {
            return;
        }
        let tex = crate::gl::texture(
            size.0,
            size.1,
            gl::NEAREST,
            gl::CLAMP_TO_EDGE,
            gl::RGBA,
            None,
        );
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                tex,
                0,
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteTextures(1, &self.tex);
        }
        self.tex = tex;
        self.size = size;
    }

    pub fn set_screen_power(&mut self, t: f32) {
        self.screen = t;
        self.game.set_power(t);
    }

    pub fn set_screen_footer(&mut self, px: f32) {
        self.game.set_bottom(px);
    }

    pub fn set_game_fills_frame(&mut self, on: bool) {
        self.game.set_fills_frame(on);
    }

    pub fn set_game_destination(&mut self, rect: Option<[f32;4]>) {
        self.game_destination=rect;
        self.game.set_destination(rect);
    }
    pub fn set_game_source_rect(&mut self, rect: [f32; 4]) {
        self.game.set_source_rect(rect);
    }

    pub fn set_shake(&mut self, dx: f32) {
        self.shake = dx;
    }

    pub fn take_capture(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        self.captured.take()
    }

    pub fn frame_size(&self) -> (u32, u32) {
        self.size
    }

    pub fn read_frame(&self) -> Vec<u8> {
        let (w, h) = self.size;
        let stride = w as usize * 4;
        let mut buf = vec![0u8; stride * h as usize];
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                w as i32,
                h as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
            );
        }
        let mut top_down = Vec::with_capacity(buf.len());
        for row in buf.chunks_exact(stride).rev() {
            top_down.extend_from_slice(row);
        }
        top_down
    }

    pub fn read_window(&mut self, window: (u32, u32)) -> Vec<u8> {
        let (w, h) = window;
        let tex = crate::gl::texture(w, h, gl::NEAREST, gl::CLAMP_TO_EDGE, gl::RGBA, None);
        let mut fbo = 0;
        unsafe {
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                tex,
                0,
            );
        }
        self.output(window, fbo);
        let stride = w as usize * 4;
        let mut buf = vec![0u8; stride * h as usize];
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                w as i32,
                h as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteFramebuffers(1, &fbo);
            gl::DeleteTextures(1, &tex);
        }
        buf.chunks_exact(stride).rev().flatten().copied().collect()
    }

    pub fn begin_frame(&mut self) {
        let size = if self.native() {
            self.window
        } else {
            canvas_size(self.window)
        };
        self.resize(size);
        self.sprites.set_target(Canvas::of(self.window));
        self.game.set_fbo_scale(
            self.size.0 as f32 / OUT_W as f32,
            self.size.1 as f32 / OUT_H as f32,
        );
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::Viewport(0, 0, self.size.0 as i32, self.size.1 as i32);
            gl::ClearColor(BACKDROP[0], BACKDROP[1], BACKDROP[2], BACKDROP[3]);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
    }

    pub fn end_frame(&mut self, window: (u32, u32)) {
        let Some(mut turn) = self.turn.take() else {
            self.output(window, 0);
            return;
        };
        if turn.size != window {
            unsafe {
                if turn.fbo != 0 {
                    gl::DeleteFramebuffers(1, &turn.fbo);
                    gl::DeleteTextures(1, &turn.tex);
                }
                turn.tex = crate::gl::texture(
                    window.0,
                    window.1,
                    gl::NEAREST,
                    gl::CLAMP_TO_EDGE,
                    gl::RGBA,
                    None,
                );
                gl::GenFramebuffers(1, &mut turn.fbo);
                gl::BindFramebuffer(gl::FRAMEBUFFER, turn.fbo);
                gl::FramebufferTexture2D(
                    gl::FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0,
                    gl::TEXTURE_2D,
                    turn.tex,
                    0,
                );
            }
            turn.size = window;
        }
        self.output(window, turn.fbo);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, turn.panel.0 as i32, turn.panel.1 as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(turn.prog);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, turn.tex);
        }
        self.quad.draw();
        self.turn = Some(turn);
    }

    fn output(&mut self, window: (u32, u32), target: gl::types::GLuint) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, target);
            gl::Viewport(0, 0, window.0 as i32, window.1 as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.blit);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::Uniform3f(self.u_gain, self.gain[0], self.gain[1], self.gain[2]);
            gl::BindTexture(gl::TEXTURE_2D, self.tex);
        }
        if self.native() {
            let canvas = Canvas::of(window);
            let dx = (self.shake * window.0 as f32 / canvas.w).round() as i32;
            let (x, y, w, h) = self.placed(window, target, (dx, 0, window.0 as i32, window.1 as i32));
            self.blit_part(x, y, w, h, [0.0, 0.0, 1.0, 1.0]);
            return;
        }
        let (x, y, w, h) = self.placed(window, target, blit_rect(window, self.shake));
        self.blit_part(x, y, w, h, [0.0, 0.0, 1.0, 1.0]);
        self.draw_tv_bezel(window, target);
    }

    /// The TV's bezel over the whole framebuffer but the game's picture, which stays as drawn.
    fn draw_tv_bezel(&self, window: (u32, u32), target: gl::types::GLuint) {
        let (Some(lb), Some(tex), Some([x, y, w, h])) = (self.letterbox, self.tv_bezel, self.tv_picture)
        else {
            return;
        };
        if target != 0 {
            return;
        }
        // the picture, from the window's top-left logical canvas to the framebuffer's GL corner
        let canvas = Canvas::of(window);
        let (sx, sy) = (window.0 as f32 / canvas.w, window.1 as f32 / canvas.h);
        let top = ((y * sy).round() as i32, ((y + h) * sy).round() as i32);
        let left = ((x * sx).round() as i32, ((x + w) * sx).round() as i32);
        let gl_rect = (left.0, window.1 as i32 - top.1, left.1 - left.0, top.1 - top.0);
        let hole = place(lb, window, gl_rect);
        unsafe {
            gl::UseProgram(self.overlay);
            gl::Uniform3f(self.u_overlay_gain, self.gain[0], self.gain[1], self.gain[2]);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }
        for (x, y, w, h) in around(lb.surface, hole) {
            let uv = image_uv(lb.surface, (x, y, w, h));
            unsafe {
                gl::Viewport(x, y, w, h);
                gl::Uniform4f(self.u_overlay_uv, uv[0], uv[1], uv[2], uv[3]);
            }
            self.quad.draw();
        }
        unsafe {
            gl::Disable(gl::BLEND);
            gl::UseProgram(self.blit);
            gl::BindTexture(gl::TEXTURE_2D, self.tex);
        }
    }

    fn panel_bezel(&self) -> Option<Draw> {
        let tex = self
            .game_bezel
            .filter(|_| crate::bezel::framed(self.window))?;
        let canvas = Canvas::of(self.window);
        Some(Draw::Tex {
            x: 0.0,
            y: 0.0,
            w: canvas.w,
            h: canvas.h,
            tex,
            alpha: 1.0,
        })
    }
}

impl Compositor {
    fn in_game_rect(&mut self, draw: impl FnOnce(&mut Self)) {
        if !self.native() || self.game_destination.is_some() {
            return draw(self);
        }
        let (x, top, w, h) = crate::bezel::game_rect(self.window, self.sp_frame);
        self.game
            .set_fbo_scale(w as f32 / OUT_W as f32, h as f32 / OUT_H as f32);
        let y = self.size.1 as i32 - top - h;
        self.game.set_origin(x as f32, y as f32);
        unsafe { gl::Viewport(x, y, w, h) };
        draw(self);
        self.game.set_origin(0.0, 0.0);
        self.game.set_fbo_scale(
            self.size.0 as f32 / OUT_W as f32,
            self.size.1 as f32 / OUT_H as f32,
        );
        unsafe { gl::Viewport(0, 0, self.size.0 as i32, self.size.1 as i32) };
    }

    fn blit_part(&self, x: i32, y: i32, w: i32, h: i32, uv: [f32; 4]) {
        unsafe {
            gl::Viewport(x, y, w, h);
            gl::Uniform4f(self.u_blit_uv, uv[0], uv[1], uv[2], uv[3]);
        }
        self.quad.draw();
    }
}

/// The parts of a `surface` around `hole` (GL rectangles, y up): below, above, left, right.
fn around(surface: (u32, u32), hole: (i32, i32, i32, i32)) -> Vec<(i32, i32, i32, i32)> {
    let (sw, sh) = (surface.0 as i32, surface.1 as i32);
    let x0 = hole.0.clamp(0, sw);
    let x1 = (hole.0 + hole.2).clamp(x0, sw);
    let y0 = hole.1.clamp(0, sh);
    let y1 = (hole.1 + hole.3).clamp(y0, sh);
    [
        (0, 0, sw, y0),
        (0, y1, sw, sh - y1),
        (0, y0, x0, y1 - y0),
        (x1, y0, sw - x1, y1 - y0),
    ]
    .into_iter()
    .filter(|r| r.2 > 0 && r.3 > 0)
    .collect()
}

/// Texture coordinates (u, v, du, dv) of a GL rectangle of the surface, for an image stretched
/// across the surface whose rows start at its top (v 0 is the image's top, GL y 0 the bottom).
fn image_uv(surface: (u32, u32), r: (i32, i32, i32, i32)) -> [f32; 4] {
    let (sw, sh) = (surface.0.max(1) as f32, surface.1.max(1) as f32);
    [
        r.0 as f32 / sw,
        1.0 - r.1 as f32 / sh,
        r.2 as f32 / sw,
        -(r.3 as f32) / sh,
    ]
}

/// `r`, a rectangle of `window`, scaled and moved into the letterbox's rectangle.
fn place(lb: Letterbox, window: (u32, u32), r: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let sx = lb.rect.2 as f32 / window.0.max(1) as f32;
    let sy = lb.rect.3 as f32 / window.1.max(1) as f32;
    let x0 = lb.rect.0 + (r.0 as f32 * sx).round() as i32;
    let y0 = lb.rect.1 + (r.1 as f32 * sy).round() as i32;
    let x1 = lb.rect.0 + ((r.0 + r.2) as f32 * sx).round() as i32;
    let y1 = lb.rect.1 + ((r.1 + r.3) as f32 * sy).round() as i32;
    (x0, y0, x1 - x0, y1 - y0)
}

#[cfg(test)]
mod letterbox_tests {
    use super::*;

    #[test]
    fn the_handhelds_screen_sits_centred_and_full_height_on_a_wide_framebuffer() {
        let lb = letterbox((848, 480), (720, 480), false);
        assert_eq!(lb.rect, (64, 0, 720, 480));
        assert_eq!(place(lb, (720, 480), (0, 0, 720, 480)), (64, 0, 720, 480));
    }

    #[test]
    fn a_stretched_game_fills_the_framebuffer() {
        let lb = letterbox((848, 480), (720, 480), true);
        assert_eq!(lb.rect, (0, 0, 848, 480));
        assert_eq!(place(lb, (720, 480), (0, 0, 720, 480)), (0, 0, 848, 480));
        // a part of the window keeps its place within it
        assert_eq!(place(lb, (720, 480), (360, 0, 360, 240)), (424, 0, 424, 240));
    }

    #[test]
    fn a_tv_bezel_covers_everything_around_the_picture_and_nothing_of_it() {
        // a Game Boy picture (480x432 at 120,24 from the top) on the TV: GL y up
        let lb = letterbox((848, 480), (720, 480), false);
        let hole = place(lb, (720, 480), (120, 24, 480, 432));
        assert_eq!(hole, (184, 24, 480, 432));
        let parts = around((848, 480), hole);
        assert_eq!(
            parts,
            vec![(0, 0, 848, 24), (0, 456, 848, 24), (0, 24, 184, 432), (664, 24, 184, 432)]
        );
        let covered: i32 = parts.iter().map(|r| r.2 * r.3).sum();
        assert_eq!(covered, 848 * 480 - 480 * 432);
        // a GBA picture fills the canvas: only the side bars
        let hole = place(lb, (720, 480), (0, 0, 720, 480));
        assert_eq!(around((848, 480), hole), vec![(0, 0, 64, 480), (784, 0, 64, 480)]);
    }

    #[test]
    fn the_bezel_image_lands_upright_where_it_is_drawn() {
        // the bottom-left GL corner shows the image's bottom-left: rows start at the top
        assert_eq!(image_uv((848, 480), (0, 0, 64, 480)), [0.0, 1.0, 64.0 / 848.0, -1.0]);
        let [u, v, du, dv] = image_uv((848, 480), (784, 456, 64, 24));
        assert!((u - 784.0 / 848.0).abs() < 1e-6 && (u + du - 1.0).abs() < 1e-6);
        assert!((v - 24.0 / 480.0).abs() < 1e-6 && (v + dv).abs() < 1e-6, "the top strip is the image's top");
    }

    #[test]
    fn a_framebuffer_of_the_windows_own_shape_is_filled() {
        assert_eq!(letterbox((720, 480), (720, 480), false).rect, (0, 0, 720, 480));
        assert_eq!(letterbox((1440, 960), (720, 480), false).rect, (0, 0, 1440, 960));
    }
}

pub fn canvas_size(window: (u32, u32)) -> (u32, u32) {
    let (_, _, w, h) = blit_rect(window, 0.0);
    let (w, h) = (w.max(1) as u32, h.max(1) as u32);
    if w % OUT_W == 0 && h % OUT_H == 0 && w / OUT_W == h / OUT_H {
        (OUT_W, OUT_H)
    } else {
        (w, h)
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
            gl::DeleteTextures(1, &self.tex);
            gl::DeleteProgram(self.blit);
            gl::DeleteProgram(self.overlay);
            if let Some(tex) = self.tv_bezel {
                gl::DeleteTextures(1, &tex);
            }
            if let Some(turn) = &self.turn {
                gl::DeleteProgram(turn.prog);
                if turn.fbo != 0 {
                    gl::DeleteFramebuffers(1, &turn.fbo);
                    gl::DeleteTextures(1, &turn.tex);
                }
            }
        }
    }
}
