/// Animation easing curve. Mirrors pixlet's `animation.Curve` interface. `linear`,
/// `ease_in`, `ease_out`, and `ease_in_out` are preset cubic beziers; `CubicBezier`
/// accepts raw control points, matching pixlet's `cubic-bezier(a,b,c,d)` syntax.
#[derive(Clone, Copy, Debug)]
pub enum Curve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f64, f64, f64, f64),
}

impl Default for Curve {
    fn default() -> Self {
        Curve::Linear
    }
}

impl Curve {
    pub fn transform(&self, t: f64) -> f64 {
        match self {
            Curve::Linear => t,
            Curve::EaseIn => cubic_bezier(0.3, 0.0, 1.0, 1.0, t),
            Curve::EaseOut => cubic_bezier(0.0, 0.0, 0.0, 1.0, t),
            Curve::EaseInOut => cubic_bezier(0.65, 0.0, 0.35, 1.0, t),
            Curve::CubicBezier(a, b, c, d) => cubic_bezier(*a, *b, *c, *d, t),
        }
    }

    pub fn parse(s: &str) -> Option<Curve> {
        if let Some(rest) = s
            .strip_prefix("cubic-bezier(")
            .and_then(|r| r.strip_suffix(')'))
        {
            let parts: Option<Vec<f64>> = rest
                .split(',')
                .map(|p| p.trim().parse::<f64>().ok())
                .collect();
            if let Some(parts) = parts {
                if parts.len() == 4 {
                    return Some(Curve::CubicBezier(parts[0], parts[1], parts[2], parts[3]));
                }
            }
            return None;
        }
        match s {
            "linear" => Some(Curve::Linear),
            "ease_in" => Some(Curve::EaseIn),
            "ease_out" => Some(Curve::EaseOut),
            "ease_in_out" => Some(Curve::EaseInOut),
            _ => None,
        }
    }
}

fn cubic_bezier(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    let epsilon = 0.0001;
    let (mut start, mut end) = (0.0_f64, 1.0_f64);
    for _ in 0..100 {
        let mid = start + (end - start) / 2.0;
        let x = bezier_value(mid, a, c);
        if (x - t).abs() < epsilon {
            return bezier_value(mid, b, d);
        }
        if x < t {
            start = mid;
        } else {
            end = mid;
        }
    }
    bezier_value((start + end) / 2.0, b, d)
}

fn bezier_value(t: f64, e: f64, f: f64) -> f64 {
    3.0 * e * (1.0 - t) * (1.0 - t) * t + 3.0 * f * (1.0 - t) * t * t + t * t * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_identity() {
        assert!((Curve::Linear.transform(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn ease_in_below_line() {
        // EaseIn should produce values below t near the start.
        let v = Curve::EaseIn.transform(0.25);
        assert!(v < 0.25, "EaseIn at 0.25 = {v}");
    }

    #[test]
    fn parse_variants() {
        assert!(matches!(Curve::parse("linear"), Some(Curve::Linear)));
        assert!(matches!(Curve::parse("ease_in_out"), Some(Curve::EaseInOut)));
        assert!(matches!(
            Curve::parse("cubic-bezier(0.1, 0.2, 0.3, 0.4)"),
            Some(Curve::CubicBezier(_, _, _, _))
        ));
        assert!(Curve::parse("bad").is_none());
    }
}
