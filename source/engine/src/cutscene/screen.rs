//! Screen fade layers of the event player (private screen-spec 1).
//!
//! The exe keeps three fade layers (A, B, C), each a state machine run once
//! per update; one event frame is one update at dt 2 (2 ticks), so a
//! duration of `d` event frames is `2d` ticks and the ramp is linear:
//! * set_in  (FUN_140156c90): opaque for two updates, then
//!   `alpha = 1 - (t - 1) / d`;
//! * set_out (FUN_140156ce0): `alpha = t / d`, then latched opaque.
//!
//! Layer of a `SeqFade` type [O FUN_1402da730]: 0,1,2,5 = A; 3,4 = B;
//! 6,7 = C. Types 2,5,6,7 are additive white, the others black.
use event::timeline::Fade;

/// One layer's colour at a frame: alpha (0..=1) and whether it is the
/// additive white of the exe's GS blend 0x48 (else black, normal blend).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerColour {
    pub alpha: f32,
    pub white: bool,
}

/// `(fade-in?, white?)` for a `SeqFade` type.
fn kind(kind: i16) -> Option<(bool, bool)> {
    Some(match kind {
        0 | 3 => (true, false),
        1 | 4 => (false, false),
        2 | 6 => (false, true),
        5 | 7 => (true, true),
        _ => return None,
    })
}

/// Which layer a type draws on: 0 = A, 1 = B, 2 = C.
pub fn layer_of(kind: i16) -> Option<usize> {
    match kind {
        0 | 1 | 2 | 5 => Some(0),
        3 | 4 => Some(1),
        6 | 7 => Some(2),
        _ => None,
    }
}

fn alpha(fade_in: bool, t: f32, duration: f32) -> f32 {
    let d = duration.max(1.);
    if fade_in {
        if t < 1. {
            1.
        } else {
            (1. - (t - 1.) / d).clamp(0., 1.)
        }
    } else {
        (t / d).clamp(0., 1.)
    }
}

/// The colour of `layer` (see [`layer_of`]) at event frame `frame`: the
/// latest `SeqFade` on it; layer C (2) starts with the event's own black
/// fade-in of `start_fade` frames (set_in from frame 0).
pub fn layer_colour(
    fades: &[Fade],
    layer: usize,
    start_fade: i32,
    frame: f32,
) -> Option<LayerColour> {
    let mut current = (layer == 2 && start_fade > 0).then(|| LayerColour {
        alpha: alpha(true, frame, start_fade as f32),
        white: false,
    });
    let mut latest = f32::NEG_INFINITY;
    for fade in fades {
        let (Some(l), Some((fade_in, white))) = (layer_of(fade.kind), kind(fade.kind)) else {
            continue;
        };
        let start = fade.frame as f32;
        if l == layer && start <= frame && start >= latest {
            latest = start;
            current = Some(LayerColour {
                alpha: alpha(fade_in, frame - start, fade.duration as f32),
                white,
            });
        }
    }
    current.filter(|c| c.alpha > 0.)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fade(frame: i32, duration: i32, kind: i16) -> Fade {
        Fade {
            frame,
            duration,
            kind,
        }
    }

    #[test]
    fn event_start_holds_two_updates_then_ramps_over_twenty() {
        let at = |f| layer_colour(&[], 2, 20, f).map(|c| c.alpha);
        assert_eq!(at(0.), Some(1.));
        assert_eq!(at(1.), Some(1.));
        assert!((at(2.).unwrap() - 0.95).abs() < 1e-6);
        assert!((at(11.).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(at(21.), None);
        assert_eq!(at(500.), None);
    }

    #[test]
    fn fade_out_is_linear_then_latches() {
        let fades = [fade(1064, 20, 1)];
        let at = |f| layer_colour(&fades, 0, 20, f).map(|c| c.alpha);
        assert_eq!(at(1063.), None);
        assert!((at(1074.).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(at(1084.), Some(1.));
        assert_eq!(at(1093.), Some(1.));
        // Nothing of layer A reaches layer C.
        assert_eq!(layer_colour(&fades, 2, 0, 1074.), None);
    }

    #[test]
    fn white_types_are_additive_and_the_latest_fade_wins() {
        let fades = [fade(10, 4, 2), fade(40, 10, 5)];
        let c = layer_colour(&fades, 0, 0, 20.).unwrap();
        assert!(c.white && c.alpha == 1.);
        let c = layer_colour(&fades, 0, 0, 41.).unwrap();
        assert!(c.white && c.alpha == 1.);
        assert!((layer_colour(&fades, 0, 0, 46.).unwrap().alpha - 0.5).abs() < 1e-6);
    }
}
