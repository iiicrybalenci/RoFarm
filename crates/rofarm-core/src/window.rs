use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ids::{ClientId, LayoutId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> i32 {
        self.x + self.width
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.height
    }
    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }

    pub fn inset(&self, padding: i32, gap: i32) -> Rect {
        Rect {
            x: self.x + padding,
            y: self.y + padding,
            width: (self.width - padding * 2 - gap).max(1),
            height: (self.height - padding * 2 - gap).max(1),
        }
    }

    pub fn clamp_within(&self, bounds: Rect, min_w: i32, min_h: i32) -> Rect {
        let width = self.width.max(min_w).min(bounds.width.max(min_w));
        let height = self.height.max(min_h).min(bounds.height.max(min_h));
        Rect {
            x: self.x.clamp(bounds.x, (bounds.right() - width).max(bounds.x)),
            y: self.y.clamp(bounds.y, (bounds.bottom() - height).max(bounds.y)),
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub bounds: Rect,
    pub work_area: Rect,
    pub scale_percent: u16,
    pub primary: bool,
}

impl MonitorInfo {
    pub fn new(name: &str, x: i32, y: i32, w: i32, h: i32, primary: bool) -> Self {
        Self {
            id: format!("mon:{name}:{x},{y}"),
            name: name.into(),
            bounds: Rect::new(x, y, w, h),
            work_area: Rect::new(x, y, w, h - 48),
            scale_percent: 100,
            primary,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlacementMode {
    #[default]
    Auto,
    Grid,
    Cascade,
    Micro,
    Free,
    SnapToLane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WindowMaterial {
    #[default]
    None,
    Mica,
    MicaAlt,
    Acrylic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowPlacement {
    pub mode: PlacementMode,
    pub monitor: Option<String>,
    pub bounds: Option<Rect>,
    pub always_on_top: bool,
    pub borderless: bool,
    pub virtual_desktop: Option<u32>,
    pub micro: bool,
}

impl Default for WindowPlacement {
    fn default() -> Self {
        Self {
            mode: PlacementMode::Auto,
            monitor: None,
            bounds: None,
            always_on_top: false,
            borderless: false,
            virtual_desktop: None,
            micro: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowLayout {
    pub id: LayoutId,
    pub name: String,
    pub kind: PlacementMode,
    pub monitor: Option<String>,
    pub slots: BTreeMap<ClientId, Rect>,
    pub created_at: DateTime<Utc>,
}

impl WindowLayout {
    pub fn new(name: impl Into<String>, kind: PlacementMode) -> Self {
        Self { id: LayoutId::new(), name: name.into(), kind, monitor: None, slots: BTreeMap::new(), created_at: Utc::now() }
    }
}

pub fn grid(bounds: Rect, count: usize, gap: i32, padding: i32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let cols = (count as f64).sqrt().ceil() as usize;
    let rows = count.div_ceil(cols);
    let avail_w = (bounds.width - padding * 2 - gap * (cols as i32 - 1)).max(1);
    let avail_h = (bounds.height - padding * 2 - gap * (rows as i32 - 1)).max(1);
    let cell_w = avail_w / cols as i32;
    let cell_h = avail_h / rows as i32;
    (0..count)
        .map(|i| {
            let col = (i % cols) as i32;
            let row = (i / cols) as i32;
            Rect::new(
                bounds.x + padding + col * (cell_w + gap),
                bounds.y + padding + row * (cell_h + gap),
                cell_w,
                cell_h,
            )
        })
        .collect()
}

pub fn cascade(bounds: Rect, count: usize, size: Rect, step: i32) -> Vec<Rect> {
    (0..count)
        .map(|i| {
            let offset = step * i as i32;
            let r = Rect::new(
                bounds.x + 40 + offset,
                bounds.y + 40 + offset,
                size.width,
                size.height,
            );
            r.clamp_within(bounds, 320, 200)
        })
        .collect()
}

pub fn snap_to_lane(bounds: Rect, lane_index: usize, lane_count: usize, gap: i32) -> Rect {
    if lane_count == 0 {
        return bounds;
    }
    let width = (bounds.width - gap * (lane_count as i32 - 1)) / lane_count as i32;
    Rect::new(bounds.x + (lane_index as i32) * (width + gap), bounds.y, width.max(1), bounds.height)
}

pub fn wall(monitors: &[MonitorInfo], count: usize) -> Vec<Rect> {
    if monitors.is_empty() || count == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    let per_monitor = count.div_ceil(monitors.len());
    for (idx, monitor) in monitors.iter().enumerate() {
        let remaining = count - out.len();
        if remaining == 0 {
            break;
        }
        let take = per_monitor.min(remaining);
        out.extend(grid(monitor.work_area, take, 0, 0));
        let _ = idx;
    }
    out.truncate(count);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> Rect {
        Rect::new(0, 0, 1920, 1080)
    }

    #[test]
    fn grid_tiles_all_windows_without_overlap() {
        let rects = grid(monitor(), 6, 4, 8);
        assert_eq!(rects.len(), 6);
        for r in &rects {
            assert!(r.width > 200 && r.height > 200);
            assert!(r.x >= 0 && r.right() <= 1920 && r.y >= 0 && r.bottom() <= 1080);
        }
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn cascade_stays_on_screen() {
        let rects = cascade(monitor(), 30, Rect::new(1000, 700, 0, 0), 32);
        for r in &rects {
            assert!(r.x >= 0 && r.y >= 0);
            assert!(r.right() <= 1920);
            assert!(r.bottom() <= 1080);
        }
    }

    #[test]
    fn micro_windows_are_small_and_clamped() {
        let r = Rect::new(500, 500, 260, 160);
        let clamped = r.clamp_within(monitor(), 240, 140);
        assert_eq!(clamped.width, 260);
        let huge = Rect::new(-100, -100, 5000, 5000).clamp_within(monitor(), 240, 140);
        assert_eq!(huge.width, 1920);
        assert_eq!(huge.x, 0);
    }

    #[test]
    fn lane_snap_divides_equally() {
        let a = snap_to_lane(monitor(), 0, 2, 10);
        let b = snap_to_lane(monitor(), 1, 2, 10);
        assert_eq!(a.width, b.width);
        assert!(!a.overlaps(&b));
        assert_eq!(b.right(), 1920);
    }

    #[test]
    fn wall_spreads_across_monitors() {
        let monitors = vec![
            MonitorInfo::new("1", 0, 0, 1920, 1080, true),
            MonitorInfo::new("2", 1920, 0, 2560, 1440, false),
        ];
        let rects = wall(&monitors, 4);
        assert_eq!(rects.len(), 4);
        assert_eq!(rects.iter().filter(|r| r.x < 1920).count(), 2);
        assert_eq!(rects.iter().filter(|r| r.x >= 1920).count(), 2);
    }

    #[test]
    fn layout_round_trips() {
        let mut layout = WindowLayout::new("Farming wall", PlacementMode::Grid);
        layout.slots.insert(ClientId::new(), Rect::new(0, 0, 640, 480));
        let json = serde_json::to_string(&layout).unwrap();
        let back: WindowLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, back);
    }
}
