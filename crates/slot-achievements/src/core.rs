use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

use slot_retro::{AchievementLayout, AvInfo, ButtonMask, CoreError, Link, RetroCore, Rumble};

use crate::{network, Control, Frame, Hub, ServiceLink, QUEUE_SIZE, RAM_SIZE};

static NEXT_GAME: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Tracked {
    core: Box<dyn RetroCore>,
    layout: AchievementLayout,
    hub: Hub,
    /// The hub epoch `link` was taken at; a different epoch means a service came or went.
    epoch: u64,
    link: Option<ServiceLink>,
    rom: Option<PathBuf>,
    recycled: mpsc::Receiver<Vec<u8>>,
    recycle: mpsc::SyncSender<Vec<u8>>,
    generation: u64,
    sequence: u64,
    timeline: u64,
    allocated: usize,
    tracking: bool,
}

impl Tracked {
    pub fn new(core: Box<dyn RetroCore>, layout: AchievementLayout, hub: Hub) -> Self {
        let (recycle, recycled) = mpsc::sync_channel(QUEUE_SIZE + 1);
        let epoch = hub.epoch();
        let link = hub.link();
        Self {
            core,
            layout,
            hub,
            epoch,
            link,
            rom: None,
            recycled,
            recycle,
            generation: 0,
            sequence: 0,
            timeline: 0,
            allocated: 0,
            tracking: true,
        }
    }

    /// Tell the attached service about the loaded game under a fresh generation, so its
    /// evaluator starts from the game's cached or online definitions.
    fn announce(&mut self) {
        let (Some(link), Some(rom)) = (&self.link, &self.rom) else {
            return;
        };
        self.generation = NEXT_GAME.fetch_add(1, Ordering::Relaxed);
        link.current.store(self.generation, Ordering::Release);
        let _ = link.controls.send(Control::Load(network::Load {
            generation: self.generation,
            path: rom.clone(),
        }));
    }

    /// Let go of the attached service's game, if it is still ours.
    fn leave(&mut self) {
        if let Some(link) = &self.link {
            let _ = link.controls.send(Control::Unload(self.generation));
            let _ = link.current.compare_exchange(
                self.generation,
                0,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
        }
    }

    /// Follow a service that attached, restarted or stopped while the game runs.
    fn relink(&mut self) {
        let epoch = self.hub.epoch();
        if epoch == self.epoch {
            return;
        }
        self.epoch = epoch;
        self.leave();
        self.link = self.hub.link();
        // Buffers still queued at a stopped worker never come back; start a fresh pool.
        let (recycle, recycled) = mpsc::sync_channel(QUEUE_SIZE + 1);
        (self.recycle, self.recycled, self.allocated) = (recycle, recycled, 0);
        self.timeline += 1;
        self.announce();
    }

    fn snapshot(&mut self) {
        self.sequence += 1;
        let Some(link) = &self.link else {
            return;
        };
        if !link.enabled.load(Ordering::Acquire) {
            return;
        }
        let mut ram = match self.recycled.try_recv() {
            Ok(ram) => ram,
            Err(_) if self.allocated < QUEUE_SIZE + 1 => {
                self.allocated += 1;
                vec![0; RAM_SIZE]
            }
            Err(_) => return,
        };
        let valid = self.core.achievement_memory(self.layout, &mut ram);
        if !self.layout.complete(&valid) {
            let _ = self.recycle.try_send(ram);
            return;
        }
        let frame = Frame {
            generation: self.generation,
            sequence: self.sequence,
            timeline: self.timeline,
            layout: self.layout,
            earned_at: network::now(),
            ram,
            valid,
            recycle: self.recycle.clone(),
        };
        // In particular, never use send(): HTTPS, fsync, or evaluation must not backpressure
        // gameplay. Sequence gaps reset the worker's hit/delta state before it proceeds.
        if let Err(error) = link.frames.try_send(frame) {
            let (mpsc::TrySendError::Full(frame) | mpsc::TrySendError::Disconnected(frame)) = error;
            let _ = self.recycle.try_send(frame.ram);
        }
    }
}

impl RetroCore for Tracked {
    fn achievement_tracking(&mut self, on: bool) {
        if self.tracking != on {
            self.timeline += 1;
        }
        self.tracking = on;
    }

    fn load(&mut self, rom: &Path) -> Result<(), CoreError> {
        self.core.load(rom)?;
        self.relink();
        self.rom = Some(rom.to_path_buf());
        self.announce();
        Ok(())
    }
    fn run_frame(&mut self, input: ButtonMask) {
        self.relink();
        self.core.run_frame(input);
        if self.tracking {
            self.snapshot();
        }
    }
    fn run_frame_linked(&mut self, p1: ButtonMask, p2: ButtonMask) {
        self.core.run_frame_linked(p1, p2);
        self.timeline += 1;
    }
    fn set_option(&mut self, key: &str, value: &str) {
        self.core.set_option(key, value);
    }
    fn set_frame_skip(&mut self, skip: bool) {
        self.core.set_frame_skip(skip);
    }
    fn video_xrgb8888(&self) -> &[u8] {
        self.core.video_xrgb8888()
    }
    fn take_audio(&mut self) -> Vec<i16> {
        self.core.take_audio()
    }
    fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        self.core.serialize()
    }
    fn unserialize(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.core.unserialize(data)?;
        self.timeline += 1;
        Ok(())
    }
    fn save_ram(&self) -> Option<Vec<u8>> {
        self.core.save_ram()
    }
    fn load_save_ram(&mut self, data: &[u8]) -> Result<(), CoreError> {
        self.core.load_save_ram(data)
    }
    fn av_info(&self) -> AvInfo {
        self.core.av_info()
    }
    fn rumble(&self) -> Rumble {
        self.core.rumble()
    }
    fn net(&self) -> Link {
        self.core.net()
    }
    fn start_link(&mut self, client_id: u16) {
        self.timeline += 1;
        self.core.start_link(client_id);
    }
    fn pump_link(&mut self) {
        self.core.pump_link();
    }
    fn stop_link(&mut self) {
        self.core.stop_link();
        self.timeline += 1;
    }
    fn achievement_memory(&self, layout: AchievementLayout, ram: &mut [u8]) -> [usize; 3] {
        self.core.achievement_memory(layout, ram)
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.leave();
    }
}
