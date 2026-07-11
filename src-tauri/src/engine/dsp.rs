const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];
const FREEVERB_GAIN: f32 = 0.015;
const SCALE_ROOM: f32 = 0.28;
const OFFSET_ROOM: f32 = 0.7;

pub(crate) struct BiquadState {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    last_freq: f32,
    last_sample_rate: f32,
}

impl BiquadState {
    pub fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
            last_freq: 0.0,
            last_sample_rate: 0.0,
        }
    }

    pub fn update_highpass(&mut self, freq_hz: f32, sample_rate: f32) {
        if freq_hz == self.last_freq && sample_rate == self.last_sample_rate {
            return;
        }
        self.last_freq = freq_hz;
        self.last_sample_rate = sample_rate;

        let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * std::f32::consts::FRAC_1_SQRT_2);

        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 + cos_w0) / 2.0) / a0;
        self.b1 = (-(1.0 + cos_w0)) / a0;
        self.b2 = ((1.0 + cos_w0) / 2.0) / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    pub fn set_lowshelf(&mut self, freq_hz: f32, gain_db: f32, sample_rate: f32) {
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        self.b0 = (a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0;
        self.b1 = (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0;
        self.b2 = (a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0;
        self.a1 = (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0;
        self.a2 = ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0;
    }

    pub fn set_peaking(&mut self, freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) {
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha / a;
        self.b0 = (1.0 + alpha * a) / a0;
        self.b1 = (-2.0 * cos_w0) / a0;
        self.b2 = (1.0 - alpha * a) / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha / a) / a0;
    }

    pub fn set_highshelf(&mut self, freq_hz: f32, gain_db: f32, sample_rate: f32) {
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        self.b0 = (a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0;
        self.b1 = (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0;
        self.b2 = (a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0;
        self.a1 = (2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0;
        self.a2 = ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0;
    }

    pub fn set_lowpass(&mut self, freq_hz: f32, q: f32, sample_rate: f32) {
        let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 - cos_w0) / 2.0) / a0;
        self.b1 = (1.0 - cos_w0) / a0;
        self.b2 = ((1.0 - cos_w0) / 2.0) / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

pub(crate) struct AmpSim {
    bass: BiquadState,
    mid: BiquadState,
    treble: BiquadState,
    cabinet: BiquadState,
    output_level: f32,
    last_preset: u32,
    last_sample_rate: f32,
}

impl AmpSim {
    pub fn new() -> Self {
        Self {
            bass: BiquadState::new(),
            mid: BiquadState::new(),
            treble: BiquadState::new(),
            cabinet: BiquadState::new(),
            output_level: 1.0,
            last_preset: 0,
            last_sample_rate: 0.0,
        }
    }

    pub fn configure(&mut self, preset: u32, sample_rate: f32) {
        if preset == self.last_preset && sample_rate == self.last_sample_rate {
            return;
        }
        self.last_preset = preset;
        self.last_sample_rate = sample_rate;

        self.bass.reset();
        self.mid.reset();
        self.treble.reset();
        self.cabinet.reset();

        match preset {
            1 => {
                self.bass.set_lowshelf(200.0, -2.0, sample_rate);
                self.mid.set_peaking(1000.0, 3.0, 0.8, sample_rate);
                self.treble.set_highshelf(3000.0, 4.0, sample_rate);
                self.cabinet.set_lowpass(5000.0, 0.7, sample_rate);
                self.output_level = 0.7;
            }
            _ => {}
        }
    }

    pub fn process(&mut self, sample: f32, drive: f32) -> f32 {
        let pre_gain = 1.0 + drive * drive * 30.0;
        let clipped = (sample * pre_gain).tanh();
        let s = self.bass.process(clipped);
        let s = self.mid.process(s);
        let s = self.treble.process(s);
        let s = self.cabinet.process(s);
        s * self.output_level
    }
}

struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    filterstore: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            index: 0,
            filterstore: 0.0,
            feedback: 0.0,
            damp1: 0.0,
            damp2: 1.0,
        }
    }

    fn set_damp(&mut self, val: f32) {
        self.damp1 = val;
        self.damp2 = 1.0 - val;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filterstore = output * self.damp2 + self.filterstore * self.damp1;
        self.buffer[self.index] = input + self.filterstore * self.feedback;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        output
    }
}

struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + buffered * 0.5;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        output
    }
}

pub(crate) struct Freeverb {
    combs: Vec<CombFilter>,
    allpasses: Vec<AllpassFilter>,
    last_room_size: f32,
    last_damping: f32,
}

impl Freeverb {
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44100.0;
        let combs = COMB_TUNINGS
            .iter()
            .map(|&t| CombFilter::new(((t as f32) * scale) as usize))
            .collect();
        let allpasses = ALLPASS_TUNINGS
            .iter()
            .map(|&t| AllpassFilter::new(((t as f32) * scale) as usize))
            .collect();
        let mut rv = Self {
            combs,
            allpasses,
            last_room_size: -1.0,
            last_damping: -1.0,
        };
        rv.set_params(0.5, 0.5);
        rv
    }

    pub fn set_params(&mut self, room_size: f32, damping: f32) {
        if room_size == self.last_room_size && damping == self.last_damping {
            return;
        }
        self.last_room_size = room_size;
        self.last_damping = damping;
        let feedback = room_size * SCALE_ROOM + OFFSET_ROOM;
        for comb in &mut self.combs {
            comb.feedback = feedback;
            comb.set_damp(damping);
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let input_scaled = input * FREEVERB_GAIN;
        let mut out = 0.0;
        for comb in &mut self.combs {
            out += comb.process(input_scaled);
        }
        for allpass in &mut self.allpasses {
            out = allpass.process(out);
        }
        out
    }
}
