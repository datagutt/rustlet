use super::{Curve, Transform};

/// Keyframe at a specific `percentage` in the animation with a list of transforms
/// to apply and an easing curve that governs the interpolation *from* this keyframe
/// to the next one. Mirrors pixlet's `animation.Keyframe`.
#[derive(Clone, Debug)]
pub struct Keyframe {
    pub percentage: f64,
    pub transforms: Vec<Transform>,
    pub curve: Curve,
}

impl Keyframe {
    pub fn new(percentage: f64, transforms: Vec<Transform>, curve: Curve) -> Self {
        Self {
            percentage,
            transforms,
            curve,
        }
    }
}

/// Sort and pad keyframes to ensure 0% and 100% entries exist, matching pixlet's
/// `processKeyframes`. If `arr` is empty, default bounding keyframes are inserted so
/// the animation is well-defined.
pub fn process_keyframes(mut arr: Vec<Keyframe>) -> Vec<Keyframe> {
    let default_from = Keyframe::new(0.0, Vec::new(), Curve::Linear);
    let default_to = Keyframe::new(1.0, Vec::new(), Curve::Linear);

    if arr.is_empty() {
        return vec![default_from, default_to];
    }

    arr.sort_by(|a, b| a.percentage.partial_cmp(&b.percentage).unwrap_or(std::cmp::Ordering::Equal));

    if arr[0].percentage != 0.0 {
        let mut prepended = Vec::with_capacity(arr.len() + 1);
        prepended.push(default_from);
        prepended.extend(arr);
        arr = prepended;
    }

    if arr.last().map(|k| k.percentage).unwrap_or(1.0) != 1.0 {
        arr.push(default_to);
    }

    arr
}

/// Return the adjacent keyframes bracketing the given `p` in [0, 1].
pub fn find_keyframes(arr: &[Keyframe], p: f64) -> Option<(&Keyframe, &Keyframe)> {
    if arr.len() < 2 {
        return None;
    }
    if !(0.0..=1.0).contains(&p) {
        return None;
    }
    for i in 0..arr.len() - 1 {
        let p0 = arr[i].percentage;
        let p1 = arr[i + 1].percentage;
        if p0 <= p && p <= p1 {
            return Some((&arr[i], &arr[i + 1]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_empty() {
        let out = process_keyframes(Vec::new());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].percentage, 0.0);
        assert_eq!(out[1].percentage, 1.0);
    }

    #[test]
    fn process_adds_missing_boundaries() {
        let out = process_keyframes(vec![Keyframe::new(0.5, Vec::new(), Curve::Linear)]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].percentage, 0.0);
        assert_eq!(out[1].percentage, 0.5);
        assert_eq!(out[2].percentage, 1.0);
    }

    #[test]
    fn find_returns_bracket() {
        let arr = vec![
            Keyframe::new(0.0, Vec::new(), Curve::Linear),
            Keyframe::new(0.5, Vec::new(), Curve::Linear),
            Keyframe::new(1.0, Vec::new(), Curve::Linear),
        ];
        let (a, b) = find_keyframes(&arr, 0.25).unwrap();
        assert_eq!(a.percentage, 0.0);
        assert_eq!(b.percentage, 0.5);
    }
}
