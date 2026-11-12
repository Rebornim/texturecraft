//! Material orb pictures, cached by look and rendered off the UI thread: a quick low-resolution picture first, then the full one
//! (the web, which has no threads, renders the full picture at once)

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex};

use egui::{Color32, ColorImage, Context, Id, TextureHandle, TextureOptions};

use crate::material_orb::render;
use crate::orb_material::OrbMaterial;

/// Pictures kept; the least recently drawn go first (a list of every plain material holds about fifty)
const MAX_PICTURES: usize = 160;

/// The quick first picture is this many times smaller than the full one, and pictures this small skip it
const QUICK_DIVISOR: usize = 4;
const QUICK_BELOW_PX: usize = 48;

/// One orb to render
#[derive(Clone, Debug)]
pub(crate) struct OrbJob {
    key: u64,
    material: OrbMaterial,
    side: usize,
    backdrop: Color32,
    /// Where it is drawn: a newer job for the same place replaces an older one still waiting
    // The web renders each job at once, so nothing is replaced there
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    place: Id,
    /// The quick picture is still to come
    quick: bool,
}

impl OrbJob {
    pub(crate) fn new(material: OrbMaterial, side: usize, backdrop: Color32) -> OrbJob {
        let mut hasher = DefaultHasher::new();
        (material.key(), side, backdrop.to_array()).hash(&mut hasher);
        OrbJob { key: hasher.finish(), material, side, backdrop, place: Id::NULL, quick: side >= QUICK_BELOW_PX }
    }

    fn picture(&self) -> ColorImage {
        let side = if self.quick { self.side / QUICK_DIVISOR } else { self.side };
        render(&self.material, side, self.backdrop)
    }
}

/// What the worker answers
#[cfg(not(target_arch = "wasm32"))]
enum OrbResult {
    Picture {
        key: u64,
        full: bool,
        image: ColorImage,
    },
    /// A newer job for its place replaced it before it was finished
    Dropped {
        key: u64,
    },
}

/// A cached picture
struct Picture {
    texture: Option<TextureHandle>,
    full: bool,
    /// The pass it was last drawn in
    drawn: u64,
}

/// Every orb picture of one egui context
#[derive(Default)]
pub struct OrbPreviews {
    pictures: HashMap<u64, Picture>,
    /// The picture last drawn at each place, shown while that place's new one renders
    shown: HashMap<Id, TextureHandle>,
    #[cfg(not(target_arch = "wasm32"))]
    worker: Option<Worker>,
}

type Shared = Arc<Mutex<OrbPreviews>>;

impl OrbPreviews {
    /// The picture to draw at `place` for `job`: its own once rendered, else the last one drawn there (None before the first)
    pub(crate) fn texture(ctx: &Context, place: Id, mut job: OrbJob) -> Option<TextureHandle> {
        let shared = ctx.data_mut(|data| data.get_temp_mut_or_default::<Shared>(Id::unique("orb_previews")).clone());
        let mut previews = shared.lock().ok()?;
        previews.take_results(ctx);
        let pass = ctx.cumulative_pass_nr();
        job.place = place;
        let key = job.key;
        if let Entry::Vacant(slot) = previews.pictures.entry(key) {
            slot.insert(Picture { texture: None, full: false, drawn: pass });
            previews.start(ctx, job);
            previews.forget_old();
        }
        let ready = previews.pictures.get_mut(&key).and_then(|picture| {
            picture.drawn = pass;
            picture.texture.clone()
        });
        match ready {
            Some(texture) => {
                previews.shown.insert(place, texture.clone());
                Some(texture)
            }
            None => previews.shown.get(&place).cloned(),
        }
    }

    /// Whether every orb asked for has its full picture (tests and captures wait for this)
    pub fn settled(ctx: &Context) -> bool {
        let shared = ctx.data_mut(|data| data.get_temp_mut_or_default::<Shared>(Id::unique("orb_previews")).clone());
        let Ok(mut previews) = shared.lock() else { return true };
        previews.take_results(ctx);
        previews.pictures.values().all(|picture| picture.full)
    }

    /// The size in pixels of the picture last drawn at `place` (tests)
    #[cfg(test)]
    pub(crate) fn shown_size(ctx: &Context, place: &str) -> Option<[usize; 2]> {
        let shared = ctx.data_mut(|data| data.get_temp_mut_or_default::<Shared>(Id::unique("orb_previews")).clone());
        let previews = shared.lock().ok()?;
        previews.shown.get(&Id::unique(("material_orb", place))).map(TextureHandle::size)
    }

    fn store(&mut self, ctx: &Context, key: u64, full: bool, image: ColorImage) {
        if let Some(picture) = self.pictures.get_mut(&key)
            && !picture.full
        {
            picture.texture = Some(ctx.load_texture("material-orb", image, TextureOptions::LINEAR));
            picture.full = full;
        }
    }

    /// Drop the least recently drawn pictures past [`MAX_PICTURES`]
    fn forget_old(&mut self) {
        while self.pictures.len() > MAX_PICTURES {
            let Some(oldest) = self.pictures.iter().min_by_key(|(_, picture)| picture.drawn).map(|(key, _)| *key) else { break };
            self.pictures.remove(&oldest);
        }
    }

    /// The web renders the full picture at once
    #[cfg(target_arch = "wasm32")]
    fn start(&mut self, ctx: &Context, job: OrbJob) {
        let job = OrbJob { quick: false, ..job };
        self.store(ctx, job.key, true, job.picture());
    }

    #[cfg(target_arch = "wasm32")]
    fn take_results(&mut self, _ctx: &Context) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn start(&mut self, ctx: &Context, job: OrbJob) {
        let key = job.key;
        let worker = match self.worker.take() {
            Some(worker) => worker,
            None => match Worker::start(ctx) {
                Some(worker) => worker,
                None => {
                    // No thread: render here rather than show nothing
                    let job = OrbJob { quick: false, ..job };
                    self.store(ctx, key, true, job.picture());
                    return;
                }
            },
        };
        match worker.jobs.send(job) {
            Ok(()) => self.worker = Some(worker),
            Err(std::sync::mpsc::SendError(job)) => {
                let job = OrbJob { quick: false, ..job };
                self.store(ctx, key, true, job.picture());
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn take_results(&mut self, ctx: &Context) {
        let Some(worker) = &self.worker else { return };
        let results: Vec<OrbResult> = worker.results.try_iter().collect();
        for result in results {
            match result {
                OrbResult::Picture { key, full, image } => self.store(ctx, key, full, image),
                OrbResult::Dropped { key } => {
                    // Asked for again the next time it is drawn (its place keeps showing what it last showed)
                    self.pictures.remove(&key);
                }
            }
        }
    }
}

/// The thread that renders orbs
#[cfg(not(target_arch = "wasm32"))]
struct Worker {
    jobs: std::sync::mpsc::Sender<OrbJob>,
    results: std::sync::mpsc::Receiver<OrbResult>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Worker {
    fn start(ctx: &Context) -> Option<Worker> {
        let (jobs, job_rx) = std::sync::mpsc::channel::<OrbJob>();
        let (result_tx, results) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("texturecraft-material-orbs".into()).spawn(move || {
            // Its own pool keeps orbs from queuing ahead of painting and compositing
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1));
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("texturecraft-orb-{i}")).build();
            let mut waiting: Vec<OrbJob> = Vec::new();
            loop {
                if waiting.is_empty() {
                    let Ok(job) = job_rx.recv() else { break };
                    waiting.push(job);
                }
                waiting.extend(job_rx.try_iter());
                for key in newest_per_place(&mut waiting) {
                    if result_tx.send(OrbResult::Dropped { key }).is_err() {
                        return;
                    }
                }
                // Every quick picture before any full one, so a list fills in at once
                let next = waiting.iter().position(|job| job.quick).unwrap_or(0);
                if next >= waiting.len() {
                    continue;
                }
                let mut job = waiting.remove(next);
                let image = match &pool {
                    Ok(pool) => pool.install(|| job.picture()),
                    Err(_) => job.picture(),
                };
                let full = !job.quick;
                if result_tx.send(OrbResult::Picture { key: job.key, full, image }).is_err() {
                    return;
                }
                ctx.request_repaint();
                if !full {
                    job.quick = false;
                    waiting.push(job);
                }
            }
        });
        match spawned {
            Ok(_) => Some(Worker { jobs, results }),
            Err(e) => {
                log::warn!("could not start the material orb worker: {e}");
                None
            }
        }
    }
}

/// Keep only the newest job per place (others than [`Id::NULL`]); returns the keys of the jobs dropped
#[cfg(not(target_arch = "wasm32"))]
fn newest_per_place(waiting: &mut Vec<OrbJob>) -> Vec<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut dropped = Vec::new();
    let mut kept: Vec<OrbJob> = Vec::with_capacity(waiting.len());
    for job in waiting.drain(..).rev() {
        if job.place == Id::NULL || seen.insert(job.place) {
            kept.push(job);
        } else {
            dropped.push(job.key);
        }
    }
    kept.reverse();
    *waiting = kept;
    dropped
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn copper(roughness: f32) -> OrbMaterial {
        OrbMaterial::uniform([0.8, 0.35, 0.18], 1.0, roughness)
    }

    /// One UI pass; nothing draws the textures it makes, so their updates are dropped
    fn run(ctx: &Context, ui: impl FnMut(&Context)) {
        let mut output = ctx.run_pass(Default::default(), ui);
        output.textures_delta.clear();
    }

    /// Draw `job` at `place` every pass until it is full or 20 s pass; returns the sizes shown on the way
    fn draw_until_settled(ctx: &Context, place: Id, job: &OrbJob) -> Vec<[usize; 2]> {
        let start = Instant::now();
        let mut sizes = Vec::new();
        while start.elapsed() < Duration::from_secs(20) {
            let mut size = None;
            run(ctx, |ctx| size = OrbPreviews::texture(ctx, place, job.clone()).map(|texture| texture.size()));
            if let Some(size) = size
                && sizes.last() != Some(&size)
            {
                sizes.push(size);
            }
            if OrbPreviews::settled(ctx) {
                run(ctx, |ctx| size = OrbPreviews::texture(ctx, place, job.clone()).map(|texture| texture.size()));
                if let Some(size) = size
                    && sizes.last() != Some(&size)
                {
                    sizes.push(size);
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        sizes
    }

    #[test]
    fn an_orb_renders_off_the_ui_thread_quick_first_then_full_and_is_cached_by_its_look() {
        let ctx = Context::default();
        let place = Id::unique("orb");
        let job = OrbJob::new(copper(0.3), 200, Color32::BLACK);
        let mut first = None;
        run(&ctx, |ctx| first = OrbPreviews::texture(ctx, place, job.clone()));
        assert!(first.is_none(), "the first pass does not wait for the render");
        let sizes = draw_until_settled(&ctx, place, &job);
        assert_eq!(sizes.last(), Some(&[200, 200]), "the full picture arrives: {sizes:?}");
        assert!(sizes.contains(&[50, 50]) || sizes.len() == 1, "a quick picture may show first: {sizes:?}");
        assert!(OrbPreviews::settled(&ctx));

        // The same look again: drawn at once from the cache, nothing new to render
        let mut again = None;
        run(&ctx, |ctx| again = OrbPreviews::texture(ctx, place, job.clone()));
        assert_eq!(again.map(|texture| texture.size()), Some([200, 200]));
        assert!(OrbPreviews::settled(&ctx), "a cached look renders nothing");

        // A new look keeps the old picture on screen until its own arrives
        let rougher = OrbJob::new(copper(0.8), 200, Color32::BLACK);
        let mut meanwhile = None;
        run(&ctx, |ctx| meanwhile = OrbPreviews::texture(ctx, place, rougher.clone()));
        assert!(meanwhile.is_some(), "the last picture stays while the new one renders");
        assert!(!OrbPreviews::settled(&ctx), "the new look is rendering");
        draw_until_settled(&ctx, place, &rougher);
        assert!(OrbPreviews::settled(&ctx));
    }

    #[test]
    fn a_newer_look_for_the_same_place_replaces_one_still_waiting() {
        let job = |roughness: f32| OrbJob { place: Id::unique("orb"), ..OrbJob::new(copper(roughness), 64, Color32::BLACK) };
        let other = OrbJob { place: Id::unique("other"), ..OrbJob::new(copper(0.5), 64, Color32::BLACK) };
        let mut waiting = vec![job(0.1), other.clone(), job(0.2), job(0.3)];
        let dropped = newest_per_place(&mut waiting);
        assert_eq!(dropped.len(), 2);
        assert_eq!(waiting.iter().map(|job| job.key).collect::<Vec<_>>(), vec![other.key, job(0.3).key]);
    }

    #[test]
    fn small_orbs_skip_the_quick_picture() {
        assert!(!OrbJob::new(copper(0.3), 24, Color32::BLACK).quick);
        assert!(OrbJob::new(copper(0.3), 96, Color32::BLACK).quick);
    }
}
