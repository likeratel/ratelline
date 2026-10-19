use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use tauri::{
    webview::{Color, WebviewWindowBuilder},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Rect, Theme, WebviewUrl,
};

pub(crate) const MAIN_LABEL: &str = "main";
pub(crate) const DASHBOARD_LABEL: &str = "dashboard";

const PANEL_GAP_LOGICAL: f64 = 8.0;
const DASHBOARD_WIDTH: f64 = 1_080.0;
const DASHBOARD_HEIGHT: f64 = 780.0;
const DASHBOARD_MIN_WIDTH: f64 = 720.0;
const DASHBOARD_MIN_HEIGHT: f64 = 520.0;
const TRAY_PRESS_NONE: u8 = 0;
const TRAY_PRESS_STARTED_HIDDEN: u8 = 1;
const TRAY_PRESS_STARTED_VISIBLE: u8 = 2;
const TRAY_PRESS_CANCELLED: u8 = 3;

#[derive(Default)]
pub(crate) struct PanelFocusState {
    hide_on_blur: AtomicBool,
    tray_press: AtomicU8,
    deferred_blur: AtomicBool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PanelClickAction {
    Show,
    Hide,
}

impl PanelFocusState {
    fn panel_shown(&self) {
        self.hide_on_blur.store(true, Ordering::SeqCst);
    }

    fn panel_hidden(&self) {
        self.hide_on_blur.store(false, Ordering::SeqCst);
    }

    pub(crate) fn should_hide_for_focus_change(
        &self,
        focused_event: bool,
        currently_focused: bool,
    ) -> bool {
        if focused_event || currently_focused {
            return false;
        }
        if matches!(
            self.tray_press.load(Ordering::SeqCst),
            TRAY_PRESS_STARTED_HIDDEN | TRAY_PRESS_STARTED_VISIBLE
        ) {
            self.deferred_blur.store(true, Ordering::SeqCst);
            return false;
        }
        self.hide_on_blur.swap(false, Ordering::SeqCst)
    }

    fn tray_press_started(&self, panel_visible: bool) {
        self.deferred_blur.store(false, Ordering::SeqCst);
        self.tray_press.store(
            if panel_visible {
                TRAY_PRESS_STARTED_VISIBLE
            } else {
                TRAY_PRESS_STARTED_HIDDEN
            },
            Ordering::SeqCst,
        );
    }

    fn tray_press_finished(&self, panel_visible: bool) -> Option<PanelClickAction> {
        self.deferred_blur.store(false, Ordering::SeqCst);
        match self.tray_press.swap(TRAY_PRESS_NONE, Ordering::SeqCst) {
            TRAY_PRESS_STARTED_HIDDEN => Some(PanelClickAction::Show),
            TRAY_PRESS_STARTED_VISIBLE => Some(PanelClickAction::Hide),
            TRAY_PRESS_CANCELLED => None,
            _ if panel_visible => Some(PanelClickAction::Hide),
            _ => Some(PanelClickAction::Show),
        }
    }

    fn tray_press_cancelled(&self, currently_focused: bool) -> bool {
        let was_active = self
            .tray_press
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |press| {
                matches!(
                    press,
                    TRAY_PRESS_STARTED_HIDDEN | TRAY_PRESS_STARTED_VISIBLE
                )
                .then_some(TRAY_PRESS_CANCELLED)
            })
            .is_ok();
        let blur_was_deferred = self.deferred_blur.swap(false, Ordering::SeqCst);
        was_active
            && blur_was_deferred
            && !currently_focused
            && self.hide_on_blur.swap(false, Ordering::SeqCst)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PixelRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

pub(crate) fn hide_panel(app: &AppHandle) -> Result<(), String> {
    let panel = app
        .get_webview_window(MAIN_LABEL)
        .ok_or_else(|| "The status panel is unavailable.".to_owned())?;
    if let Some(state) = app.try_state::<PanelFocusState>() {
        state.panel_hidden();
    }
    if panel.hide().is_err() {
        if let Some(state) = app.try_state::<PanelFocusState>() {
            state.panel_shown();
        }
        return Err("The status panel could not be hidden.".to_owned());
    }
    Ok(())
}

pub(crate) fn show_panel(app: &AppHandle, anchor: Option<(Rect, PhysicalPosition<f64>)>) {
    let Some(panel) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };

    // Accessory applications can remain hidden when launched in the background.
    // AppHandle::show calls NSApplication::unhide on macOS, so only use it while
    // no dashboard exists that could be restored as an unintended side effect.
    #[cfg(target_os = "macos")]
    if app.get_webview_window(DASHBOARD_LABEL).is_none() {
        let _ = app.show();
    }
    if let Some((rect, click_position)) = anchor {
        position_panel(app, &panel, rect, click_position);
    }
    let _ = panel.unminimize();
    if panel.show().is_ok() {
        let _ = panel.set_focus();
        if let Some(state) = app.try_state::<PanelFocusState>() {
            state.panel_shown();
        }
    }
}

pub(crate) fn begin_tray_panel_click(app: &AppHandle) {
    let Some(panel) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    let panel_visible = panel.is_visible().unwrap_or(false);
    if let Some(state) = app.try_state::<PanelFocusState>() {
        state.tray_press_started(panel_visible);
    }
}

pub(crate) fn finish_tray_panel_click(
    app: &AppHandle,
    rect: Rect,
    click_position: PhysicalPosition<f64>,
) {
    let Some(panel) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    let panel_visible = panel.is_visible().unwrap_or(false);
    let action = if let Some(state) = app.try_state::<PanelFocusState>() {
        state.tray_press_finished(panel_visible)
    } else if panel_visible {
        Some(PanelClickAction::Hide)
    } else {
        Some(PanelClickAction::Show)
    };
    let Some(action) = action else {
        return;
    };

    match action {
        PanelClickAction::Show => show_panel(app, Some((rect, click_position))),
        PanelClickAction::Hide => {
            let _ = hide_panel(app);
        }
    }
}

pub(crate) fn cancel_tray_panel_click(app: &AppHandle) {
    let Some(panel) = app.get_webview_window(MAIN_LABEL) else {
        return;
    };
    let currently_focused = panel.is_focused().unwrap_or(true);
    if app
        .try_state::<PanelFocusState>()
        .is_some_and(|state| state.tray_press_cancelled(currently_focused))
    {
        let _ = hide_panel(app);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn should_show_panel_on_reopen(app: &AppHandle) -> bool {
    should_show_panel_for_window_states(app.get_webview_window(DASHBOARD_LABEL).into_iter().map(
        |window| {
            (
                window.is_visible().unwrap_or(true),
                window.is_minimized().unwrap_or(false),
            )
        },
    ))
}

#[cfg(any(target_os = "macos", test))]
fn should_show_panel_for_window_states(states: impl IntoIterator<Item = (bool, bool)>) -> bool {
    !states
        .into_iter()
        .any(|(visible, minimized)| visible && !minimized)
}

pub(crate) fn open_dashboard(app: &AppHandle) -> Result<(), String> {
    if let Some(dashboard) = app.get_webview_window(DASHBOARD_LABEL) {
        dashboard
            .unminimize()
            .map_err(|_| "The dashboard could not be restored.".to_owned())?;
        dashboard
            .show()
            .map_err(|_| "The dashboard could not be shown.".to_owned())?;
        dashboard
            .set_focus()
            .map_err(|_| "The dashboard could not be focused.".to_owned())?;
        return hide_panel(app);
    }

    WebviewWindowBuilder::new(
        app,
        DASHBOARD_LABEL,
        WebviewUrl::App("index.html?view=dashboard".into()),
    )
    .title("ratelline dashboard")
    .inner_size(DASHBOARD_WIDTH, DASHBOARD_HEIGHT)
    .min_inner_size(DASHBOARD_MIN_WIDTH, DASHBOARD_MIN_HEIGHT)
    .resizable(true)
    .decorations(true)
    .always_on_top(false)
    .skip_taskbar(false)
    .theme(Some(Theme::Dark))
    .background_color(Color(13, 14, 16, 255))
    .center()
    .build()
    .map_err(|_| "The dashboard could not be opened.".to_owned())?;
    hide_panel(app)
}

#[cfg(target_os = "macos")]
fn position_panel(
    app: &AppHandle,
    panel: &tauri::WebviewWindow,
    _rect: Rect,
    _click: PhysicalPosition<f64>,
) {
    if let Err(error) = position_panel_macos(app, panel) {
        eprintln!("Failed to position the status panel: {error}");
    }
}

#[cfg(target_os = "macos")]
fn position_panel_macos(app: &AppHandle, panel: &tauri::WebviewWindow) -> Result<(), String> {
    // Tao scales cursor coordinates with the PRIMARY display; tray events instead
    // use the status item's display. Undo that scale before selecting a monitor.
    let primary = app
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("The primary display is unavailable.")?;
    let cursor = app
        .cursor_position()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(primary.scale_factor());
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(|e| e.to_string())?
        .ok_or("The clicked display is unavailable.")?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let work_origin = area.position.to_logical::<f64>(scale);
    let work_size = area.size.to_logical::<f64>(scale);
    let work = PixelRect {
        x: work_origin.x,
        y: work_origin.y,
        width: work_size.width,
        height: work_size.height,
    };
    // Replicated menu bars may report another display's status-item rectangle.
    // Anchor to the actual pointer and this display's menu bar instead.
    let tray = clicked_tray_rect(cursor.x, cursor.y, work.y);
    let old_scale = panel.scale_factor().map_err(|e| e.to_string())?;
    let size = panel
        .outer_size()
        .map_err(|e| e.to_string())?
        .to_logical::<u32>(old_scale);
    let point = anchored_position(
        tray,
        work,
        PhysicalSize::new(size.width, size.height),
        PANEL_GAP_LOGICAL,
    );
    // Physical positions would be divided by the panel's OLD display scale.
    panel
        .set_position(tauri::LogicalPosition::new(point.x, point.y))
        .map_err(|e| e.to_string())
}

#[cfg(any(target_os = "macos", test))]
fn clicked_tray_rect(x: f64, y: f64, menu_bottom: f64) -> PixelRect {
    PixelRect {
        x,
        y,
        width: 0.0,
        height: (menu_bottom - y).max(0.0),
    }
}

#[cfg(not(target_os = "macos"))]
fn position_panel(
    app: &AppHandle,
    panel: &tauri::WebviewWindow,
    tray_rect: Rect,
    click_position: PhysicalPosition<f64>,
) {
    let monitor = app
        .monitor_from_point(click_position.x, click_position.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let Ok(panel_size) = panel.outer_size() else {
        return;
    };

    let scale = monitor.scale_factor();
    let tray_position = tray_rect.position.to_physical::<f64>(scale);
    let tray_size = tray_rect.size.to_physical::<f64>(scale);
    let tray = PixelRect {
        x: tray_position.x,
        y: tray_position.y,
        width: tray_size.width,
        height: tray_size.height,
    };
    let work_area = monitor.work_area();
    let work = PixelRect {
        x: f64::from(work_area.position.x),
        y: f64::from(work_area.position.y),
        width: f64::from(work_area.size.width),
        height: f64::from(work_area.size.height),
    };
    let position = anchored_position(tray, work, panel_size, PANEL_GAP_LOGICAL * scale);
    let _ = panel.set_position(position);
}

fn anchored_position(
    tray: PixelRect,
    work: PixelRect,
    panel: PhysicalSize<u32>,
    gap: f64,
) -> PhysicalPosition<i32> {
    let panel_width = f64::from(panel.width);
    let panel_height = f64::from(panel.height);
    let work_right = work.x + work.width;
    let work_bottom = work.y + work.height;
    let max_x = (work_right - panel_width).max(work.x);
    let max_y = (work_bottom - panel_height).max(work.y);

    let centered_x = tray.x + (tray.width - panel_width) / 2.0;
    let below = tray.y + tray.height + gap;
    let above = tray.y - panel_height - gap;
    let preferred_y = if below + panel_height <= work_bottom {
        below
    } else if above >= work.y {
        above
    } else {
        below
    };

    PhysicalPosition::new(
        centered_x.clamp(work.x, max_x).round() as i32,
        preferred_y.clamp(work.y, max_y).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrored_tray_anchors_to_clicked_negative_display() {
        let tray = clicked_tray_rect(-600.0, 310.0, 330.0);
        let work = PixelRect {
            x: -1512.0,
            y: 330.0,
            width: 1512.0,
            height: 950.0,
        };
        assert_eq!(
            anchored_position(tray, work, PhysicalSize::new(420, 540), 8.0),
            PhysicalPosition::new(-810, 338)
        );
    }

    #[test]
    fn mixed_scale_panel_uses_logical_size_on_vertically_arranged_display() {
        let tray = PixelRect {
            x: 900.0,
            y: 1100.0,
            width: 24.0,
            height: 32.0,
        };
        let work = PixelRect {
            x: 0.0,
            y: 1132.0,
            width: 1512.0,
            height: 950.0,
        };
        for old_scale in [1.0, 2.0] {
            let physical = tauri::LogicalSize::new(420_u32, 540_u32).to_physical::<u32>(old_scale);
            let logical = physical.to_logical::<u32>(old_scale);
            assert_eq!(
                anchored_position(
                    tray,
                    work,
                    PhysicalSize::new(logical.width, logical.height),
                    8.0
                ),
                PhysicalPosition::new(702, 1140)
            );
        }
        assert_eq!(
            anchored_position(
                clicked_tray_rect(912.0, 1110.0, 1132.0),
                work,
                PhysicalSize::new(420, 540),
                8.0
            ),
            PhysicalPosition::new(702, 1140)
        );
    }

    #[test]
    fn panel_anchors_below_a_top_tray_and_clamps_to_work_area() {
        let position = anchored_position(
            PixelRect {
                x: 1_880.0,
                y: 0.0,
                width: 40.0,
                height: 24.0,
            },
            PixelRect {
                x: 0.0,
                y: 24.0,
                width: 1_920.0,
                height: 1_056.0,
            },
            PhysicalSize::new(420, 540),
            8.0,
        );

        assert_eq!(position, PhysicalPosition::new(1_500, 32));
    }

    #[test]
    fn panel_anchors_above_a_bottom_taskbar() {
        let position = anchored_position(
            PixelRect {
                x: 900.0,
                y: 1_040.0,
                width: 40.0,
                height: 40.0,
            },
            PixelRect {
                x: 0.0,
                y: 0.0,
                width: 1_920.0,
                height: 1_040.0,
            },
            PhysicalSize::new(420, 540),
            8.0,
        );

        assert_eq!(position, PhysicalPosition::new(710, 492));
    }

    #[test]
    fn panel_handles_negative_monitor_coordinates() {
        let position = anchored_position(
            PixelRect {
                x: -1_920.0,
                y: -24.0,
                width: 32.0,
                height: 24.0,
            },
            PixelRect {
                x: -1_920.0,
                y: 0.0,
                width: 1_920.0,
                height: 1_080.0,
            },
            PhysicalSize::new(840, 1_080),
            16.0,
        );

        assert_eq!(position, PhysicalPosition::new(-1_920, 0));
    }

    #[test]
    fn shown_panel_hides_on_blur_without_waiting_for_a_focus_event() {
        let state = PanelFocusState::default();

        assert!(!state.should_hide_for_focus_change(false, false));
        state.panel_shown();
        assert!(state.should_hide_for_focus_change(false, false));
        assert!(!state.should_hide_for_focus_change(false, false));
    }

    #[test]
    fn tray_click_uses_visibility_at_mouse_down() {
        let state = PanelFocusState::default();

        state.tray_press_started(false);
        assert_eq!(
            state.tray_press_finished(false),
            Some(PanelClickAction::Show)
        );

        state.panel_shown();
        state.tray_press_started(true);
        assert!(!state.should_hide_for_focus_change(false, false));
        assert_eq!(
            state.tray_press_finished(true),
            Some(PanelClickAction::Hide)
        );
        state.panel_hidden();
        assert!(!state.should_hide_for_focus_change(false, false));
    }

    #[test]
    fn abandoned_tray_press_applies_a_deferred_outside_blur() {
        let state = PanelFocusState::default();

        state.panel_shown();
        state.tray_press_started(true);
        assert!(!state.should_hide_for_focus_change(false, false));
        assert!(state.tray_press_cancelled(false));
        assert!(!state.should_hide_for_focus_change(false, false));
    }

    #[test]
    fn cancelled_tray_press_consumes_a_late_mouse_up() {
        let state = PanelFocusState::default();

        state.panel_shown();
        state.tray_press_started(true);
        assert!(!state.should_hide_for_focus_change(false, false));
        assert!(state.tray_press_cancelled(false));
        assert_eq!(state.tray_press_finished(false), None);
        assert!(!state.should_hide_for_focus_change(false, false));

        state.tray_press_started(false);
        assert_eq!(
            state.tray_press_finished(false),
            Some(PanelClickAction::Show)
        );
    }

    #[test]
    fn reopen_preserves_unminimized_windows_but_not_minimized_ones() {
        // A panel-only application can always retry showing its panel, even
        // when AppKit reports the panel visible but the app is backgrounded.
        assert!(should_show_panel_for_window_states([]));
        assert!(should_show_panel_for_window_states([(false, false)]));
        assert!(should_show_panel_for_window_states([
            (false, false),
            (true, true)
        ]));
        assert!(!should_show_panel_for_window_states([
            (false, false),
            (true, false)
        ]));
        assert!(!should_show_panel_for_window_states([(true, false)]));
    }

    #[test]
    fn explicit_hide_ignores_late_focus_events() {
        let state = PanelFocusState::default();

        state.panel_shown();
        state.panel_hidden();
        assert!(!state.should_hide_for_focus_change(true, true));
        assert!(!state.should_hide_for_focus_change(false, false));
    }

    #[test]
    fn stale_blur_does_not_disarm_a_focused_panel() {
        let state = PanelFocusState::default();

        state.panel_shown();
        assert!(!state.should_hide_for_focus_change(false, true));
        assert!(state.should_hide_for_focus_change(false, false));
    }

    #[test]
    fn reopened_panel_arms_the_next_blur() {
        let state = PanelFocusState::default();

        state.panel_shown();
        assert!(state.should_hide_for_focus_change(false, false));
        state.panel_shown();
        assert!(!state.should_hide_for_focus_change(true, true));
        assert!(state.should_hide_for_focus_change(false, false));
    }
}
