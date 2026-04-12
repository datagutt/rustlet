/// Direction controls how animation progress sweeps through keyframes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Direction {
    pub alternate: bool,
    pub reverse: bool,
}

impl Direction {
    pub const NORMAL: Direction = Direction {
        alternate: false,
        reverse: false,
    };
    pub const REVERSE: Direction = Direction {
        alternate: false,
        reverse: true,
    };
    pub const ALTERNATE: Direction = Direction {
        alternate: true,
        reverse: false,
    };
    pub const ALTERNATE_REVERSE: Direction = Direction {
        alternate: true,
        reverse: true,
    };

    pub fn from_str(s: &str) -> Option<Direction> {
        match s {
            "" | "normal" => Some(Direction::NORMAL),
            "reverse" => Some(Direction::REVERSE),
            "alternate" => Some(Direction::ALTERNATE),
            "alternate-reverse" => Some(Direction::ALTERNATE_REVERSE),
            _ => None,
        }
    }

    pub fn frame_count(&self, delay: i32, duration: i32) -> i32 {
        if self.alternate {
            2 * (delay + duration)
        } else {
            delay + duration + delay
        }
    }

    pub fn progress(&self, delay: i32, duration: i32, fill: f64, frame_idx: i32) -> f64 {
        let idx1 = delay;
        let idx2 = delay + duration;
        let idx3 = delay + duration + delay;
        let idx4 = delay + duration + delay + duration;

        let mut progress = if frame_idx < idx1 {
            0.0
        } else if frame_idx < idx2 {
            if duration <= 1 {
                1.0
            } else {
                (frame_idx - idx1) as f64 / (duration - 1) as f64
            }
        } else if frame_idx < idx3 {
            1.0
        } else if self.alternate && frame_idx < idx4 {
            let p = if duration <= 1 {
                1.0
            } else {
                (frame_idx - idx3) as f64 / (duration - 1) as f64
            };
            1.0 - p
        } else {
            fill
        };

        if self.reverse {
            progress = 1.0 - progress;
        }
        progress
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FillMode {
    Forwards,
    Backwards,
}

impl FillMode {
    pub fn value(&self) -> f64 {
        match self {
            FillMode::Forwards => 1.0,
            FillMode::Backwards => 0.0,
        }
    }

    pub fn from_str(s: &str) -> Option<FillMode> {
        match s {
            "" | "forwards" => Some(FillMode::Forwards),
            "backwards" => Some(FillMode::Backwards),
            _ => None,
        }
    }
}

impl Default for FillMode {
    fn default() -> Self {
        FillMode::Forwards
    }
}

/// Rounding applied to translate transforms (not scale/rotate).
#[derive(Clone, Copy, Debug)]
pub enum Rounding {
    Round,
    Floor,
    Ceil,
    None,
}

impl Rounding {
    pub fn apply(&self, v: f64) -> f64 {
        match self {
            Rounding::Round => v.round(),
            Rounding::Floor => v.floor(),
            Rounding::Ceil => v.ceil(),
            Rounding::None => v,
        }
    }

    pub fn from_str(s: &str) -> Option<Rounding> {
        match s {
            "" | "round" => Some(Rounding::Round),
            "floor" => Some(Rounding::Floor),
            "ceil" => Some(Rounding::Ceil),
            "none" => Some(Rounding::None),
            _ => None,
        }
    }
}

impl Default for Rounding {
    fn default() -> Self {
        Rounding::Round
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_normal_progress() {
        let d = Direction::NORMAL;
        // delay=2, duration=4, frames: 0..=8 -> 0,0,0..0.33..1,1,1
        assert_eq!(d.progress(2, 4, 1.0, 0), 0.0);
        assert_eq!(d.progress(2, 4, 1.0, 2), 0.0);
        // idx = delay + duration - 1 = 5 -> (5-2)/(4-1) = 1.0
        assert!((d.progress(2, 4, 1.0, 5) - 1.0).abs() < 1e-6);
        // After animation: fill value
        assert_eq!(d.progress(2, 4, 1.0, 20), 1.0);
    }

    #[test]
    fn direction_reverse_inverts() {
        let d = Direction::REVERSE;
        assert_eq!(d.progress(0, 4, 1.0, 0), 1.0);
    }

    #[test]
    fn rounding_round_parses() {
        assert!(matches!(Rounding::from_str("round"), Some(Rounding::Round)));
        assert!(matches!(Rounding::from_str("floor"), Some(Rounding::Floor)));
        assert!(Rounding::from_str("???").is_none());
    }
}
