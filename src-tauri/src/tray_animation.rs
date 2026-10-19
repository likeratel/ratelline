//! Walks the macOS menu-bar ratel while runs are in progress.
//!
//! The decision and frame-stepping logic is platform independent so it can be
//! unit tested everywhere; only macOS drives the native tray icon. Other
//! platforms keep their static tray icon.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::time::Duration;

/// Delay between walking frames.
pub(crate) const FRAME_INTERVAL: Duration = Duration::from_millis(140);
/// Number of frames in the walking cycle (`icons/tray-walk/walk-{0..3}@2x.png`).
pub(crate) const FRAME_COUNT: usize = 6;
/// The standing pose shown whenever the ratel is not walking.
pub(crate) const IDLE_FRAME: usize = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Transition {
    Start,
    Stop,
    Keep,
}

/// Walks only while runs are in progress, the preference is on, and the
/// system does not ask to reduce motion. `reduce_motion` is evaluated lazily
/// because reading it requires a hop to the main thread.
pub(crate) fn should_animate(
    active_runs: usize,
    enabled: bool,
    reduce_motion: impl FnOnce() -> bool,
) -> bool {
    active_runs > 0 && enabled && !reduce_motion()
}

pub(crate) fn transition(running: bool, wanted: bool) -> Transition {
    match (running, wanted) {
        (false, true) => Transition::Start,
        (true, false) => Transition::Stop,
        _ => Transition::Keep,
    }
}

pub(crate) fn next_frame(index: usize) -> usize {
    (index + 1) % FRAME_COUNT
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{sync, TrayAnimator};

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Mutex, OnceLock};

    use tauri::{async_runtime::JoinHandle, image::Image, AppHandle, Manager, Runtime};

    use super::{next_frame, should_animate, transition, Transition, FRAME_COUNT, IDLE_FRAME};

    pub(super) const FRAME_BYTES: [&[u8]; FRAME_COUNT] = [
        include_bytes!("../icons/tray-walk/walk-0@2x.png"),
        include_bytes!("../icons/tray-walk/walk-1@2x.png"),
        include_bytes!("../icons/tray-walk/walk-2@2x.png"),
        include_bytes!("../icons/tray-walk/walk-3@2x.png"),
        include_bytes!("../icons/tray-walk/walk-4@2x.png"),
        include_bytes!("../icons/tray-walk/walk-5@2x.png"),
    ];

    /// Owns the single walking task. Every start or stop bumps `generation`;
    /// a task or queued frame from an older generation never touches the icon.
    #[derive(Default)]
    pub(crate) struct TrayAnimator {
        inner: Mutex<AnimatorInner>,
    }

    #[derive(Default)]
    struct AnimatorInner {
        generation: u64,
        task: Option<JoinHandle<()>>,
    }

    impl TrayAnimator {
        fn is_current(&self, generation: u64) -> bool {
            self.inner
                .lock()
                .is_ok_and(|inner| inner.generation == generation)
        }
    }

    /// Starts or stops walking after each snapshot. Never call while holding
    /// the animator lock: queued main-thread work may need it.
    pub(crate) fn sync<R: Runtime>(app: &AppHandle<R>, active_runs: usize, enabled: bool) {
        let Some(animator) = app.try_state::<TrayAnimator>() else {
            return;
        };
        let wanted =
            frames().is_some() && should_animate(active_runs, enabled, || reduce_motion(app));
        let stopped_generation = {
            let mut inner = match animator.inner.lock() {
                Ok(inner) => inner,
                Err(error) => {
                    eprintln!("Could not lock the tray animation state: {error}");
                    return;
                }
            };
            // A task that ended on its own (the event loop went away) no longer counts as running.
            let running = inner
                .task
                .as_ref()
                .is_some_and(|task| !task.inner().is_finished());
            match transition(running, wanted) {
                Transition::Keep => None,
                Transition::Start => {
                    inner.generation = inner.generation.wrapping_add(1);
                    let generation = inner.generation;
                    inner.task = Some(tauri::async_runtime::spawn(walk(app.clone(), generation)));
                    None
                }
                Transition::Stop => {
                    inner.generation = inner.generation.wrapping_add(1);
                    if let Some(task) = inner.task.take() {
                        task.abort();
                    }
                    Some(inner.generation)
                }
            }
        };
        if let Some(generation) = stopped_generation {
            show_frame(app, generation, IDLE_FRAME);
        }
    }

    async fn walk<R: Runtime>(app: AppHandle<R>, generation: u64) {
        let mut frame = IDLE_FRAME;
        loop {
            tokio::time::sleep(super::FRAME_INTERVAL).await;
            let current = app
                .try_state::<TrayAnimator>()
                .is_some_and(|animator| animator.is_current(generation));
            if !current {
                return;
            }
            frame = next_frame(frame);
            if !show_frame(&app, generation, frame) {
                return;
            }
        }
    }

    /// Queues the icon change on the main thread. The generation is checked
    /// there, so a frame queued before a stop cannot land after the idle pose.
    fn show_frame<R: Runtime>(app: &AppHandle<R>, generation: u64, frame: usize) -> bool {
        let handle = app.clone();
        let queued = app.run_on_main_thread(move || {
            let current = handle
                .try_state::<TrayAnimator>()
                .is_some_and(|animator| animator.is_current(generation));
            if !current {
                return;
            }
            let frame = if frame != IDLE_FRAME && system_reduce_motion() {
                IDLE_FRAME
            } else {
                frame
            };
            apply_frame(&handle, frame);
        });
        if let Err(error) = queued {
            eprintln!("Could not schedule the tray animation frame: {error}");
            return false;
        }
        true
    }

    static FRAME_ERROR_LOGGED: AtomicBool = AtomicBool::new(false);

    fn apply_frame<R: Runtime>(app: &AppHandle<R>, frame: usize) {
        let (Some(image), Some(tray)) = (
            frames().and_then(|frames| frames.get(frame)),
            app.tray_by_id(crate::TRAY_ID),
        ) else {
            return;
        };
        // `set_icon` clears the template flag on macOS; set both atomically so
        // the glyph keeps following the menu-bar appearance without flicker.
        match tray.set_icon_with_as_template(Some(image.clone()), true) {
            Ok(()) => FRAME_ERROR_LOGGED.store(false, Ordering::Relaxed),
            // A failure repeats every frame; log it once until a frame succeeds again.
            Err(error) => {
                if !FRAME_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
                    eprintln!("Could not update the tray animation frame: {error}");
                }
            }
        }
    }

    /// Decoded once on first use. A decoding failure disables walking.
    fn frames() -> Option<&'static [Image<'static>]> {
        static FRAMES: OnceLock<Option<Vec<Image<'static>>>> = OnceLock::new();
        FRAMES
            .get_or_init(|| {
                match FRAME_BYTES
                    .iter()
                    .map(|bytes| Image::from_bytes(bytes))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(frames) => Some(frames),
                    Err(error) => {
                        eprintln!(
                            "Could not decode the walking tray frames; the icon stays still: {error}"
                        );
                        None
                    }
                }
            })
            .as_deref()
    }

    /// Reads the accessibility setting on the main thread. If it cannot be
    /// read, assume motion should be reduced.
    fn reduce_motion<R: Runtime>(app: &AppHandle<R>) -> bool {
        let (sender, receiver) = mpsc::channel();
        if let Err(error) = app.run_on_main_thread(move || {
            let _ = sender.send(system_reduce_motion());
        }) {
            eprintln!("Could not read the Reduce motion setting: {error}");
            return true;
        }
        receiver.recv().unwrap_or(true)
    }

    fn system_reduce_motion() -> bool {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_only_for_active_runs_with_the_preference_and_without_reduce_motion() {
        assert!(should_animate(1, true, || false));
        assert!(!should_animate(0, true, || false));
        assert!(!should_animate(3, false, || false));
        assert!(!should_animate(3, true, || true));
    }

    #[test]
    fn reduce_motion_is_not_read_when_nothing_would_walk() {
        assert!(!should_animate(0, true, || panic!(
            "read without active runs"
        )));
        assert!(!should_animate(2, false, || panic!("read while disabled")));
    }

    #[test]
    fn transitions_start_and_stop_only_on_changes() {
        assert_eq!(transition(false, true), Transition::Start);
        assert_eq!(transition(true, false), Transition::Stop);
        assert_eq!(transition(true, true), Transition::Keep);
        assert_eq!(transition(false, false), Transition::Keep);
    }

    #[test]
    fn frames_cycle_from_the_idle_pose() {
        let sequence: Vec<_> =
            std::iter::successors(Some(IDLE_FRAME), |frame| Some(next_frame(*frame)))
                .take(FRAME_COUNT + 2)
                .collect();
        assert_eq!(sequence, [0, 1, 2, 3, 4, 5, 0, 1]);
        assert_eq!(next_frame(FRAME_COUNT - 1), IDLE_FRAME);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn walking_frames_are_retina_template_glyphs() {
        for bytes in macos::FRAME_BYTES {
            let frame = tauri::image::Image::from_bytes(bytes).unwrap();
            assert_eq!((frame.width(), frame.height()), (84, 44));
            let (pixels, _) = frame.rgba().as_chunks::<4>();
            assert!(pixels
                .iter()
                .all(|pixel| pixel[3] == 0 || pixel[..3] == [0, 0, 0]));
            assert!(pixels.iter().any(|pixel| pixel[3] > 0));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_idle_frame_matches_the_static_menu_bar_icon() {
        let idle = tauri::image::Image::from_bytes(macos::FRAME_BYTES[IDLE_FRAME]).unwrap();
        let still =
            tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png")).unwrap();
        assert_eq!(
            (idle.width(), idle.height()),
            (still.width(), still.height())
        );
        assert_eq!(idle.rgba(), still.rgba());
    }
}
