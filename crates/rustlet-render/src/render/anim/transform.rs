use super::{lerp, Rounding};
use tiny_skia::Transform as TsTransform;

/// Transform is a CSS-style transform that can be applied to a rendered child.
#[derive(Clone, Copy, Debug)]
pub enum Transform {
    Translate { x: f64, y: f64 },
    Rotate { angle: f64 },
    Scale { x: f64, y: f64 },
    Shear { x_angle: f64, y_angle: f64 },
}

impl Transform {
    /// Compose this transform into the accumulated affine matrix `acc`. The matrix is
    /// applied in order, so `apply` should be called in the same order the transforms
    /// appear in the keyframe.
    pub fn apply(&self, acc: TsTransform, origin: (f64, f64), rounding: Rounding) -> TsTransform {
        match *self {
            Transform::Translate { x, y } => {
                let dx = rounding.apply(x) as f32;
                let dy = rounding.apply(y) as f32;
                acc.pre_translate(dx, dy)
            }
            Transform::Rotate { angle } => {
                let (ox, oy) = (origin.0 as f32, origin.1 as f32);
                acc.pre_rotate_at(angle as f32, ox, oy)
            }
            Transform::Scale { x, y } => {
                let (ox, oy) = (origin.0 as f32, origin.1 as f32);
                acc.pre_translate(ox, oy)
                    .pre_scale(x as f32, y as f32)
                    .pre_translate(-ox, -oy)
            }
            Transform::Shear { x_angle, y_angle } => {
                let kx = (x_angle.to_radians()).tan() as f32;
                let ky = (y_angle.to_radians()).tan() as f32;
                let (ox, oy) = (origin.0 as f32, origin.1 as f32);
                let shear = TsTransform::from_row(1.0, ky, kx, 1.0, 0.0, 0.0);
                acc.pre_translate(ox, oy)
                    .pre_concat(shear)
                    .pre_translate(-ox, -oy)
            }
        }
    }

    pub fn kind(&self) -> TransformKind {
        match self {
            Transform::Translate { .. } => TransformKind::Translate,
            Transform::Rotate { .. } => TransformKind::Rotate,
            Transform::Scale { .. } => TransformKind::Scale,
            Transform::Shear { .. } => TransformKind::Shear,
        }
    }

    pub fn default_for(kind: TransformKind) -> Transform {
        match kind {
            TransformKind::Translate => Transform::Translate { x: 0.0, y: 0.0 },
            TransformKind::Rotate => Transform::Rotate { angle: 0.0 },
            TransformKind::Scale => Transform::Scale { x: 1.0, y: 1.0 },
            TransformKind::Shear => Transform::Shear {
                x_angle: 0.0,
                y_angle: 0.0,
            },
        }
    }

    /// Interpolate between two transforms of the same kind at progress `t`. Returns
    /// `None` if the two transforms are of different kinds. Pixlet's behavior here is
    /// to fall back to matrix-level interpolation, which we don't yet support.
    pub fn interpolate(&self, other: &Transform, t: f64) -> Option<Transform> {
        match (*self, *other) {
            (
                Transform::Translate { x: x1, y: y1 },
                Transform::Translate { x: x2, y: y2 },
            ) => Some(Transform::Translate {
                x: lerp(x1, x2, t),
                y: lerp(y1, y2, t),
            }),
            (Transform::Rotate { angle: a1 }, Transform::Rotate { angle: a2 }) => {
                Some(Transform::Rotate {
                    angle: lerp(a1, a2, t),
                })
            }
            (
                Transform::Scale { x: x1, y: y1 },
                Transform::Scale { x: x2, y: y2 },
            ) => Some(Transform::Scale {
                x: lerp(x1, x2, t),
                y: lerp(y1, y2, t),
            }),
            (
                Transform::Shear {
                    x_angle: xa1,
                    y_angle: ya1,
                },
                Transform::Shear {
                    x_angle: xa2,
                    y_angle: ya2,
                },
            ) => Some(Transform::Shear {
                x_angle: lerp(xa1, xa2, t),
                y_angle: lerp(ya1, ya2, t),
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformKind {
    Translate,
    Rotate,
    Scale,
    Shear,
}

/// Extend `lhs` with default transforms of the missing kinds in `rhs`, matching pixlet
/// behavior from `ExtendTransforms`.
fn extend_transforms(mut lhs: Vec<Transform>, rhs: &[Transform]) -> Vec<Transform> {
    for (i, tr) in rhs.iter().enumerate() {
        if i >= lhs.len() {
            lhs.push(Transform::default_for(tr.kind()));
        }
    }
    lhs
}

/// Interpolate two lists of transforms. Returns an empty list + false if interpolation
/// is not possible (e.g. mismatched transform kinds in the same position).
pub fn interpolate_transforms(
    lhs: &[Transform],
    rhs: &[Transform],
    progress: f64,
) -> (Vec<Transform>, bool) {
    if lhs.is_empty() && rhs.is_empty() {
        return (Vec::new(), true);
    }

    let (a, b) = if lhs.len() < rhs.len() {
        (extend_transforms(lhs.to_vec(), rhs), rhs.to_vec())
    } else if lhs.len() > rhs.len() {
        (lhs.to_vec(), extend_transforms(rhs.to_vec(), lhs))
    } else {
        (lhs.to_vec(), rhs.to_vec())
    };

    let mut result = Vec::with_capacity(a.len());
    for i in 0..a.len() {
        match a[i].interpolate(&b[i], progress) {
            Some(t) => result.push(t),
            None => return (Vec::new(), false),
        }
    }
    (result, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_interpolates() {
        let a = Transform::Translate { x: 0.0, y: 0.0 };
        let b = Transform::Translate { x: 10.0, y: 20.0 };
        let result = a.interpolate(&b, 0.5).unwrap();
        if let Transform::Translate { x, y } = result {
            assert!((x - 5.0).abs() < 1e-6);
            assert!((y - 10.0).abs() < 1e-6);
        } else {
            panic!("expected translate");
        }
    }

    #[test]
    fn mismatched_kinds_fail() {
        let a = Transform::Translate { x: 0.0, y: 0.0 };
        let b = Transform::Rotate { angle: 90.0 };
        assert!(a.interpolate(&b, 0.5).is_none());
    }

    #[test]
    fn interpolate_list_extends() {
        let lhs = vec![Transform::Rotate { angle: 0.0 }];
        let rhs = vec![
            Transform::Rotate { angle: 360.0 },
            Transform::Translate { x: 10.0, y: 0.0 },
        ];
        let (out, ok) = interpolate_transforms(&lhs, &rhs, 0.5);
        assert!(ok);
        assert_eq!(out.len(), 2);
        if let Transform::Rotate { angle } = out[0] {
            assert!((angle - 180.0).abs() < 1e-6);
        }
        if let Transform::Translate { x, .. } = out[1] {
            assert!((x - 5.0).abs() < 1e-6);
        }
    }
}
