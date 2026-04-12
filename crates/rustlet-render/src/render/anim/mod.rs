pub mod curve;
pub mod direction;
pub mod keyframe;
pub mod positioned;
pub mod transform;
pub mod transformation;

pub use curve::Curve;
pub use direction::{Direction, FillMode, Rounding};
pub use keyframe::Keyframe;
pub use positioned::AnimatedPositioned;
pub use transform::Transform;
pub use transformation::{Origin, Transformation};

pub fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t
}

pub fn rescale(from_min: f64, from_max: f64, to_min: f64, to_max: f64, v: f64) -> f64 {
    if (from_max - from_min).abs() < f64::EPSILON {
        return to_max;
    }
    to_min + (v - from_min) / (from_max - from_min) * (to_max - to_min)
}
