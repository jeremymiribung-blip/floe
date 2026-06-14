pub trait AudioSample: Copy {
    fn to_mono_value(self) -> f32;
}

impl AudioSample for f32 {
    fn to_mono_value(self) -> f32 {
        self.clamp(-1.0, 1.0)
    }
}

impl AudioSample for f64 {
    fn to_mono_value(self) -> f32 {
        (self as f32).clamp(-1.0, 1.0)
    }
}

impl AudioSample for i16 {
    fn to_mono_value(self) -> f32 {
        signed_to_f32(self as f32, i16::MAX as f32)
    }
}

impl AudioSample for i8 {
    fn to_mono_value(self) -> f32 {
        signed_to_f32(self as f32, i8::MAX as f32)
    }
}

impl AudioSample for i32 {
    fn to_mono_value(self) -> f32 {
        signed_to_f32(self as f32, i32::MAX as f32)
    }
}

impl AudioSample for u16 {
    fn to_mono_value(self) -> f32 {
        unsigned_to_f32(self as f64, u16::MAX as f64)
    }
}

impl AudioSample for u8 {
    fn to_mono_value(self) -> f32 {
        unsigned_to_f32(self as f64, u8::MAX as f64)
    }
}

impl AudioSample for u32 {
    fn to_mono_value(self) -> f32 {
        unsigned_to_f32(self as f64, u32::MAX as f64)
    }
}

fn signed_to_f32(value: f32, max: f32) -> f32 {
    (value / max).clamp(-1.0, 1.0)
}

fn unsigned_to_f32(value: f64, max: f64) -> f32 {
    (((value / max) * 2.0) - 1.0).clamp(-1.0, 1.0) as f32
}

pub fn sanitize_sample(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}
