//! The geometry of a timeline window. A window shows frames `[start, start + frames)` of a clip
//! over `width` pixels; every timeline and its zoom (the zoom is an enlarged part of the same
//! timeline, QNC rule 0.12) map frames and pixels through this one block. Pure.

/// Share of the window before and after an enlarged range (15 % / 70 % / 15 %).
pub const ENLARGE_SIDE: f64 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub start: u64,
    pub frames: u64,
    pub width: f32,
}

impl Axis {
    /// A window over the whole clip.
    pub fn of_clip(frames: u64, width: f32) -> Axis {
        Axis { start: 0, frames: frames.max(1), width: width.max(1.0) }
    }

    pub fn end(&self) -> u64 {
        self.start + self.frames
    }

    pub fn shows(&self, frame: u64) -> bool {
        frame >= self.start && frame < self.end()
    }

    pub fn pixels_per_frame(&self) -> f32 {
        self.width / self.frames as f32
    }

    /// The left edge of a frame, in pixels from the window's left; frames outside run past it.
    pub fn x_of(&self, frame: u64) -> f32 {
        (frame as f64 - self.start as f64) as f32 * self.pixels_per_frame()
    }

    /// The frame under a pixel, held inside the window.
    pub fn frame_at(&self, x: f32) -> u64 {
        let frame = (x / self.pixels_per_frame()).floor();
        let frame = frame.clamp(0.0, (self.frames - 1) as f32) as u64;
        self.start + frame
    }

    /// Pixels to the nearer of `frame` and the pointer `x`.
    pub fn distance(&self, frame: u64, x: f32) -> f32 {
        (self.x_of(frame) - x).abs()
    }

    /// The window that shows `[from, to)` of a clip of `clip_frames` frames at 70 % of the
    /// width with 15 % of its length before and after, inside the clip.
    pub fn enlarged(clip_frames: u64, from: u64, to: u64, width: f32) -> Axis {
        let clip_frames = clip_frames.max(1);
        let to = to.clamp(from + 1, clip_frames);
        let length = (to - from) as f64;
        let side = (length * ENLARGE_SIDE / (1.0 - 2.0 * ENLARGE_SIDE)).round().max(1.0) as u64;
        let start = from.saturating_sub(side);
        let end = (to + side).min(clip_frames);
        Axis { start, frames: (end - start).max(1), width: width.max(1.0) }
    }

    /// The zoom window: `reach` frames before and after `cut`, inside the clip.
    pub fn zoom(clip_frames: u64, cut: u64, reach: u64, width: f32) -> Axis {
        let clip_frames = clip_frames.max(1);
        let start = cut.saturating_sub(reach);
        let end = cut.saturating_add(reach).min(clip_frames).max(start + 1);
        Axis { start, frames: end - start, width: width.max(1.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_pixels_map_both_ways_inside_the_window() {
        let axis = Axis::of_clip(100, 200.0);
        assert_eq!(axis.x_of(50), 100.0);
        assert_eq!(axis.frame_at(100.0), 50);
        assert_eq!(axis.frame_at(-5.0), 0);
        assert_eq!(axis.frame_at(1000.0), 99);
        assert_eq!(axis.distance(50, 104.0), 4.0);
    }

    #[test]
    fn the_enlarged_window_is_15_70_15_and_the_zoom_sits_around_the_cut_inside_the_clip() {
        let axis = Axis::enlarged(1000, 100, 170, 700.0);
        assert_eq!((axis.start, axis.end()), (85, 185));
        assert_eq!(axis.x_of(100).round(), 105.0, "IN at 15 % of the width");
        assert_eq!(Axis::enlarged(1000, 0, 70, 700.0).start, 0, "never before the clip");
        assert_eq!(Axis::enlarged(1000, 950, 1000, 700.0).end(), 1000, "never past the clip");
        let zoom = Axis::zoom(1000, 500, 50, 900.0);
        assert_eq!((zoom.start, zoom.end()), (450, 550));
        assert_eq!(Axis::zoom(1000, 10, 50, 900.0).start, 0);
        assert_eq!(Axis::zoom(1000, 990, 50, 900.0).end(), 1000);
    }
}
