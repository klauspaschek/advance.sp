//! A spring-led gathering of the firmware letters, then a drop behind the bay.
use slot_gfx::{Canvas, Compositor, Draw, TexId};

pub const BOOT_MS: u64 = 1120;
const DROP_MS: u64 = 510;
const LAND_MS: u64 = 1010;
// The ADVANCE·SP wordmark's cream.
const INK: [u8; 3] = [242,232,198];
// Seven pieces of the firmware wordmark, measured from its left edge (the logo's ink, not the
// whole screen it is drawn on), cut where its slanted, touching letters are thinnest
// (A | D | V | AN | CE· | S | P); the gaps around the dot and between S and P are blank.
const LETTER_EDGES: [f32; 8] = [0.0, 64.0, 131.0, 201.0, 292.0, 443.0, 550.0, 650.0];
/// How far from the background a pixel may be and still count as background: the wordmark's
/// near-black specks must not draw as dark dots.
const BACKGROUND_TOLERANCE: u8 = 24;
fn is_background(rgb: [u8; 3], background: [u8; 3]) -> bool {
    rgb.iter().zip(background).all(|(c, b)| c.abs_diff(b) <= BACKGROUND_TOLERANCE)
}
/// Where the wordmark's ink lies in the firmware logo: `[x, y, width, height]`. The logo may be
/// the whole screen with the wordmark in its middle; the animation moves only the wordmark.
fn ink_bounds(logo: &crate::bootlogo::Bmp) -> Option<[usize; 4]> {
    let background = *logo.rgb.first()?;
    let ink = |x: usize, y: usize| !is_background(logo.rgb[y * logo.width + x], background);
    let columns: Vec<usize> = (0..logo.width).filter(|&x| (0..logo.height).any(|y| ink(x, y))).collect();
    let rows: Vec<usize> = (0..logo.height).filter(|&y| (0..logo.width).any(|x| ink(x, y))).collect();
    let (x0, x1) = (*columns.first()?, *columns.last()?);
    let (y0, y1) = (*rows.first()?, *rows.last()?);
    Some([x0, y0, x1 - x0 + 1, y1 - y0 + 1])
}
fn smooth(t: f32) -> f32 { let t=t.clamp(0.0,1.0); t*t*t*(t*(t*6.0-15.0)+10.0) }
fn mix(a: f32,b: f32,t: f32)->f32 { a+(b-a)*t }

/// Closed-form unit-mass spring: elapsed time, not frame count, determines the
/// pose. M3 Expressive default spatial uses stiffness 380 and damping 0.8.
/// https://m3.material.io/styles/motion/overview/how-it-works
fn spatial(ms: f32) -> f32 {
    let t=ms.max(0.0)/1000.0;
    let omega=380.0_f32.sqrt();let damping=0.8;
    let damped=omega*(1.0_f32-damping*damping).sqrt();
    1.0-(-damping*omega*t).exp()*((damped*t).cos()+damping*omega/damped*(damped*t).sin())
}
fn reveal(ms:u64)->f32 {smooth((ms as f32-180.0)/460.0)}
/// Nothing moves before this (the first letter gathers from 40 ms, the scene shows from 180 ms):
/// until then the firmware's own picture stays on screen, exactly as it drew it.
const HOLD_MS: u64 = 40;
fn blob_pose(ms:u64,canvas:Canvas)->[f32;4] {
    let t=((ms as f32-DROP_MS as f32)/(LAND_MS-DROP_MS)as f32).clamp(0.0,1.0);
    // Acceleration carries the blob through the opening instead of braking it
    // in mid-air. Stretch follows the fall, with approximately constant area.
    let stretch=(1.0+0.65*t*t).sqrt();
    let settling=spatial(ms as f32-220.0);
    let w=mix(36.0,26.0,settling)/stretch;
    let h=mix(19.0,26.0,settling)*stretch;
    let anticipation=7.0*16.0*t*t*(1.0-t)*(1.0-t);
    let cy=mix(canvas.h/2.0,canvas.h-slot_ui::MOUTH_H+40.0,t*t)-anticipation;
    [canvas.centred(w),cy-h/2.0,w,h]
}

/// The wordmark cut out of the firmware logo (`w` by `h`, at `at` within it), and the logo's
/// whole size (`full`), to start exactly where the firmware showed it. `background` is the logo's
/// own background, under the letters until the launcher shows; `hold` is the firmware's whole
/// picture for one canvas size (see `prepare`).
pub struct BootMotion {
    tex: TexId, blob: TexId, w: f32, h: f32, at: [f32; 2], full: [f32; 2],
    background: [u8; 3], logo: crate::bootlogo::Bmp, hold: Option<(TexId, [u32; 2])>,
}
impl BootMotion {
    pub fn upload(c: &mut Compositor) -> Option<Self> {
        // One small mark texture shared by all seven glyphs, plus a 64px blob.
        // No frame atlas,
        // texture uploads, or offscreen pass during the animation.
        let logo=crate::bootlogo::decode(include_bytes!("../../../card/System/bootlogo.bmp"))?;
        let background=*logo.rgb.first()?;
        let [x0,y0,w,h]=ink_bounds(&logo)?;
        let rgba:Vec<u8>=(y0..y0+h).flat_map(|y| logo.rgb[y*logo.width+x0..y*logo.width+x0+w].iter())
            .flat_map(|rgb| [rgb[0],rgb[1],rgb[2],if is_background(*rgb,background) {0}else{255}]).collect();
        let blob:Vec<u8>=(0..64).flat_map(|y|(0..64).flat_map(move |x| {
            let distance=(x as f32+0.5-32.0).hypot(y as f32+0.5-32.0);
            [INK[0],INK[1],INK[2],((32.5-distance).clamp(0.0,1.0)*255.0).round()as u8]
        })).collect();
        Some(Self{tex:c.create_texture(w as u32,h as u32,&rgba),blob:c.create_texture(64,64,&blob),
            w:w as f32,h:h as f32,at:[x0 as f32,y0 as f32],full:[logo.width as f32,logo.height as f32],
            background,logo,hold:None})
    }
    /// Uploads the picture the firmware shows on this canvas (`bootlogo::compose`, as installed for
    /// the panel), so the first frames hand off without a change: the full-screen logo's own
    /// background, its near-black specks and its shrinking on smaller screens included.
    pub fn prepare(&mut self, c: &mut Compositor, canvas: Canvas) {
        let size=[canvas.w as u32,canvas.h as u32];
        if size[0]==0 || size[1]==0 || self.hold.is_some_and(|(_,held)|held==size) {return;}
        let Some(shown)=crate::bootlogo::compose(&self.logo,(size[0] as usize,size[1] as usize)) else {return};
        let rgba:Vec<u8>=shown.rgb.iter().flat_map(|p|[p[0],p[1],p[2],255]).collect();
        let tex=match self.hold {
            Some((tex,_))=>{c.update_texture(tex,size[0],size[1],&rgba);tex}
            None=>c.create_texture(size[0],size[1],&rgba),
        };
        self.hold=Some((tex,size));
    }
    /// Where the firmware drew the wordmark on this canvas, `[x, y, scale]`: the logo centred, and
    /// shrunk whole when it is bigger than the screen, as `bootlogo::compose` builds it.
    fn placed(&self, canvas: Canvas) -> [f32; 3] {
        let scale=(canvas.w/self.full[0]).min(canvas.h/self.full[1]).min(1.0);
        let left=((canvas.w-(self.full[0]*scale).floor())/2.0).floor();
        let top=((canvas.h-(self.full[1]*scale).floor())/2.0).floor();
        [left+self.at[0]*scale,top+self.at[1]*scale,scale]
    }
    pub fn draw(&self, ms:u64, canvas:Canvas, out:&mut Vec<Draw>) {
        if ms>=BOOT_MS {return;}
        if ms<HOLD_MS {
            if let Some((tex,_))=self.hold.filter(|(_,held)|*held==[canvas.w as u32,canvas.h as u32]) {
                out.push(Draw::Tex{x:0.0,y:0.0,w:canvas.w,h:canvas.h,tex,alpha:1.0});
                return;
            }
        }
        let ground=1.0-reveal(ms);
        let [r,g,b]=self.background.map(|c|c as f32/255.0);
        if ground>0.0 {out.push(Draw::Rect{x:0.0,y:0.0,w:canvas.w,h:canvas.h,colour:[r,g,b,ground]});}
        let [ox,oy,scale]=self.placed(canvas);
        let origin=[ox,oy];
        let (mark_w,mark_h)=(self.w*scale,self.h*scale);
        if ms==0 {
            out.push(Draw::Tex{x:origin[0],y:origin[1],w:mark_w,h:mark_h,tex:self.tex,alpha:1.0});
            return;
        }
        let morph=smooth((ms as f32-220.0)/220.0);
        if morph<1.0 {
            for (index,edges) in LETTER_EDGES.windows(2).enumerate() {
                let centre=(edges[0]+edges[1])/2.0*scale;
                // The middle gathers first; the outer letters follow within
                // 27 ms, retaining a single connected visual rhythm.
                let gather=spatial(ms as f32-40.0-(index as f32-3.0).abs()*9.0);
                let scale_x=mix(1.0,0.14,gather);let scale_y=mix(1.0,0.18,gather);
                let w=(edges[1]-edges[0])*scale*scale_x;let h=mark_h*scale_y;
                let cx=mix(origin[0]+centre,canvas.w/2.0,gather);
                let cy=mix(origin[1]+mark_h/2.0,canvas.h/2.0,gather);
                out.push(Draw::Part{x:cx-w/2.0,y:cy-h/2.0,w,h,tex:self.tex,alpha:1.0-morph,uv:[edges[0]/self.w,0.0,(edges[1]-edges[0])/self.w,1.0]});
            }
        }
        if morph>0.0 {
            let [x,y,w,h]=blob_pose(ms,canvas);
            // Bay artwork includes translucent plastic. Explicitly occlude at
            // its top edge so the falling blob cannot glow through the shell.
            let visible=h.min(canvas.h-slot_ui::MOUTH_H-y).max(0.0);
            if visible>0.0 {out.push(Draw::Part{x,y,w,h:visible,tex:self.blob,alpha:morph,uv:[0.0,0.0,1.0,visible/h]});}
        }
    }
    pub fn compose(&self,ms:u64,canvas:Canvas,slot:Option<(TexId,TexId)>,out:&mut Vec<Draw>) {
        if ms>=BOOT_MS {return;}
        // Remove the scene's original bay before composing it at its final depth.
        // Drawing a second translucent copy would change the plastic's shading.
        if let Some((back,front))=slot {
            out.retain(|d|!matches!(d,Draw::Tex{tex,..}|Draw::Part{tex,..} if *tex==back||*tex==front));
        }
        self.draw(ms,canvas,out);
        // Reveal the bay with the scene, then let it occlude the descending blob.
        let reveal=reveal(ms);
        if reveal==0.0 {return;}
        let begin=out.len();slot_ui::draw_empty_slot(canvas,slot,out);
        for draw in &mut out[begin..] {match draw {
            Draw::Tex{alpha,..}|Draw::Part{alpha,..}=>*alpha*=reveal,
            Draw::Rect{colour,..}|Draw::RoundRect{colour,..}=>colour[3]*=reveal,
            _=>{}
        }}
    }
    pub fn release(self,c:&mut Compositor){for tex in [self.tex,self.blob].into_iter().chain(self.hold.map(|(tex,_)|tex)) {c.update_texture(tex,1,1,&[0,0,0,0]);}}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn logo()->crate::bootlogo::Bmp {crate::bootlogo::decode(include_bytes!("../../../card/System/bootlogo.bmp")).unwrap()}
    fn motion()->BootMotion {
        let logo=logo();let [x,y,w,h]=ink_bounds(&logo).unwrap();
        BootMotion{tex:TexId::from_raw(1),blob:TexId::from_raw(2),w:w as f32,h:h as f32,at:[x as f32,y as f32],full:[logo.width as f32,logo.height as f32],
            background:logo.rgb[0],logo,hold:None}
    }
    /// The logo as the firmware shows it on a `width` by `height` screen.
    fn firmware(width:usize,height:usize)->crate::bootlogo::Bmp {crate::bootlogo::compose(&logo(),(width,height)).unwrap()}
    #[test]
    fn handoff_starts_where_the_firmware_drew_the_wordmark_and_leaves_no_overlay() {
        let motion=motion();
        for (width,height) in [(720usize,480usize),(640,480)] {
            let canvas=Canvas{w:width as f32,h:height as f32};let mut out=vec![];
            motion.draw(0,canvas,&mut out);
            let Draw::Tex{x,y,w,h,alpha,..}=out[1] else {panic!("{:?}",out[1])};
            assert_eq!(alpha,1.0);
            // the drawn wordmark covers the firmware's ink, to within a pixel of shrinking
            let shown=firmware(width,height);
            let [ix,iy,iw,ih]=ink_bounds(&shown).unwrap();
            for (drawn,ink) in [(x,ix),(y,iy),(x+w,ix+iw),(y+h,iy+ih)] {
                assert!((drawn-ink as f32).abs()<=1.5,"{width}x{height}: drawn {drawn}, firmware ink {ink}");
            }
            for ms in [BOOT_MS,u64::MAX] {out.clear();motion.draw(ms,canvas,&mut out);assert!(out.is_empty());}
        }
    }
    #[test]
    fn letters_collect_before_a_small_blob_accelerates_below_the_slot() {
        let motion=motion();let canvas=Canvas::FRAMED;
        let mut out=vec![];motion.draw(250,canvas,&mut out);
        assert_eq!(out.iter().filter(|d|matches!(d,Draw::Part{tex,..} if *tex==motion.tex)).count(),7);
        out.clear();motion.draw(DROP_MS,canvas,&mut out);
        assert!(!out.iter().any(|d|matches!(d,Draw::Part{tex,..} if *tex==motion.tex)));
        let [_,y,w,h]=blob_pose(DROP_MS,canvas);
        assert!(w<32.0&&h<32.0&&(y+h/2.0-canvas.h/2.0).abs()<0.01);
        let mut last=blob_pose(DROP_MS,canvas);
        for ms in DROP_MS+1..=LAND_MS {
            let pose=blob_pose(ms,canvas);
            assert!((pose[0]+pose[2]/2.0-canvas.w/2.0).abs()<0.01);
            assert!(pose[1]+pose[3]/2.0>=last[1]+last[3]/2.0-0.01);
            assert!((pose[1]-last[1]).abs()<1.1,"discontinuous drop at {ms}");
            assert!(pose[2]>15.0&&pose[2]<40.0&&pose[3]>15.0&&pose[3]<40.0);
            last=pose;
        }
        assert!(last[1]>canvas.h-slot_ui::MOUTH_H);
    }
    #[test]
    fn letter_crops_cover_the_whole_mark_and_cut_only_where_it_is_thin() {
        let logo=logo();
        let [x0,y0,width,height]=ink_bounds(&logo).unwrap();
        assert_eq!(LETTER_EDGES[0],0.0);
        assert_eq!(width as f32,*LETTER_EDGES.last().unwrap(),"the pieces end where the wordmark does");
        assert!(LETTER_EDGES.windows(2).all(|e|e[0]<e[1]));
        let background=logo.rgb[0];
        let ink=|x:usize|(y0..y0+height).filter(|y|!is_background(logo.rgb[y*logo.width+x0+x],background)).count();
        let busiest=(0..width).map(ink).max().unwrap();
        for &edge in &LETTER_EDGES[1..7] {
            assert!(ink(edge as usize)*2<=busiest,"letter crop at {edge} crosses a thick stroke");
        }
    }
    #[test]
    fn gathering_is_continuous_and_has_only_a_quiet_spring_overshoot() {
        assert_eq!(spatial(0.0),0.0);
        let mut last=0.0;let mut peak=0.0_f32;
        for ms in 1..600 {let next=spatial(ms as f32);assert!((next-last).abs()<0.01);peak=peak.max(next);last=next;}
        assert!(peak>1.0&&peak<1.02);
    }
}
