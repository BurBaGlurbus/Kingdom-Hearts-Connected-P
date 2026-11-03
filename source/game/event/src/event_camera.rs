//! Event camera key evaluation. Key times are 1/512 s ticks; a cut's local
//! time runs from its start frame.
use data::event_script::{CameraData, CameraKey};

pub const KEY_TICKS_PER_SECOND: f32 = 512.;

/// One channel at local time `seconds`.
pub fn channel(keys: &[CameraKey], seconds: f32) -> f32 {
    let Some(first) = keys.first() else {
        return 0.;
    };
    if keys.len() == 1 {
        return first.value;
    }
    // Lookup uses the truncated tick (FUN_1402d4580); extrapolation past
    // either end uses the key's tangent per tick quirk, or the linear
    // slope when the end segment is linear.
    let tick = (seconds * KEY_TICKS_PER_SECOND).trunc();
    let last = keys[keys.len() - 1];
    if tick <= first.time as f32 {
        let span = (keys[1].time - first.time) as f32;
        let slope = if first.interpolation == 1 && span != 0. {
            (keys[1].value - first.value) / span
        } else {
            first.tangent_in
        };
        return first.value - (first.time as f32 - tick) * slope;
    }
    if tick >= last.time as f32 {
        let prev = keys[keys.len() - 2];
        let span = (last.time - prev.time) as f32;
        let slope = if last.interpolation == 1 && span != 0. {
            (last.value - prev.value) / span
        } else {
            last.tangent_out
        };
        return last.value + (tick - last.time as f32) * slope;
    }
    let i = keys
        .windows(2)
        .position(|w| tick >= w[0].time as f32 && tick < w[1].time as f32)
        .unwrap_or(keys.len() - 2);
    let (a, b) = (keys[i], keys[i + 1]);
    let span = (b.time - a.time) as f32 / KEY_TICKS_PER_SECOND;
    if span <= 0. {
        return b.value;
    }
    let t = seconds - a.time as f32 / KEY_TICKS_PER_SECOND;
    match a.interpolation {
        0 => a.value,
        1 => a.value + (b.value - a.value) * t / span,
        _ => {
            let s = t / span;
            let (s2, s3) = (s * s, s * s * s);
            a.value * (2. * s3 - 3. * s2 + 1.)
                + a.tangent_out * span * (s3 - 2. * s2 + s)
                + b.value * (-2. * s3 + 3. * s2)
                + b.tangent_in * span * (s3 - s2)
        }
    }
}

/// Eye, target, roll and field of view (as stored) at local `seconds`.
pub struct View {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub roll: f32,
    pub fov: f32,
}

pub fn view(camera: &CameraData, seconds: f32) -> View {
    let c = |i: usize| channel(&camera.channels[i], seconds);
    View {
        eye: [c(0), c(1), c(2)],
        target: [c(3), c(4), c(5)],
        roll: c(6),
        fov: c(7),
    }
}
