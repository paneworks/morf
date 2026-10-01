//! Stateful display filtering for any numeric frequency-band source.
//! Scratch arrays stay native; neighbour spread is two O(n) sweeps rather
//! than an O(n²) scan with a power calculation for every pair of bars.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub bars: usize,
    pub rate_hz: f64,
    pub noise: f64,
    pub smoothing: f64,
    pub gravity: f64,
    pub spread: f64,
    pub attack: f64,
    pub release: f64,
    pub auto: bool,
    pub sensitivity: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            bars: 24,
            rate_hz: 60.0,
            noise: 0.002,
            smoothing: 0.55,
            gravity: 3.2,
            spread: 1.5,
            attack: 4.0,
            release: 0.35,
            auto: true,
            sensitivity: 1.0,
        }
    }
}
impl Options {
    pub fn validate(self) -> Result<Self, String> {
        if !(1..=512).contains(&self.bars)
            || !self.rate_hz.is_finite()
            || self.rate_hz <= 0.0
            || self.rate_hz > 1000.0
            || [
                self.noise,
                self.smoothing,
                self.gravity,
                self.spread,
                self.attack,
                self.release,
                self.sensitivity,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 1e6)
            || self.smoothing > 1.0
        {
            return Err("invalid spectrum filter options".into());
        }
        Ok(self)
    }
}
fn resample_into(bands: &[f64], out: &mut [f64]) {
    let n = bands.len();
    let count = out.len();
    if n == 0 {
        out.fill(0.0);
        return;
    }
    for (i, v) in out.iter_mut().enumerate() {
        let from = i as f64 * n as f64 / count as f64;
        let to = (i + 1) as f64 * n as f64 / count as f64;
        *v = if to - from >= 1.0 {
            bands[from.floor() as usize..(to.ceil() as usize).min(n)]
                .iter()
                .copied()
                .fold(0.0, f64::max)
        } else {
            let x = (from + to) / 2.0 - 0.5;
            let lo = ((x.floor() + 1.0).clamp(1.0, n as f64) as usize) - 1;
            let hi = (lo + 1).min(n - 1);
            let t = x - x.floor();
            bands[lo] * (1.0 - t) + bands[hi] * t
        };
    }
}
fn check_bands(bands: &[f64]) -> Result<(), String> {
    if bands.len() > 4096 || bands.iter().any(|v| !v.is_finite()) {
        Err("spectrum needs at most 4096 finite bands".into())
    } else {
        Ok(())
    }
}
pub fn resample(bands: &[f64], count: usize) -> Result<Vec<f64>, String> {
    check_bands(bands)?;
    if !(1..=512).contains(&count) {
        return Err("spectrum needs 1..512 bars".into());
    }
    let mut out = vec![0.0; count];
    resample_into(bands, &mut out);
    Ok(out)
}
pub struct Filter {
    o: Options,
    shown: Vec<f64>,
    fall: Vec<f64>,
    target: Vec<f64>,
    gain: f64,
}
impl Filter {
    pub fn new(o: Options) -> Result<Self, String> {
        let o = o.validate()?;
        Ok(Self {
            shown: vec![0.0; o.bars],
            fall: vec![0.0; o.bars],
            target: vec![0.0; o.bars],
            gain: if o.auto { 8.0 } else { o.sensitivity },
            o,
        })
    }
    pub fn gain(&self) -> f64 {
        self.gain
    }
    pub fn step(&mut self, bands: &[f64], dt: Option<f64>) -> Result<&[f64], String> {
        check_bands(bands)?;
        let dt = dt.unwrap_or(1.0 / self.o.rate_hz);
        if !dt.is_finite() {
            return Err("spectrum dt must be finite".into());
        }
        let dt = dt.max(1e-3);
        resample_into(bands, &mut self.target);
        let mut overshoot = false;
        for value in &mut self.target {
            *value = (*value - self.o.noise).max(0.0) * self.gain;
            overshoot |= *value > 1.0;
        }
        if self.o.auto {
            self.gain = if overshoot {
                self.gain / (1.0 + self.o.attack * dt)
            } else {
                self.gain * (1.0 + self.o.release * dt)
            };
            self.gain = self.gain.clamp(0.05, 400.0);
        }
        if self.o.spread > 1.0 {
            for i in 1..self.target.len() {
                self.target[i] = self.target[i].max(self.target[i - 1] / self.o.spread);
            }
            for i in (0..self.target.len() - 1).rev() {
                self.target[i] = self.target[i].max(self.target[i + 1] / self.o.spread);
            }
        }
        let keep = self.o.smoothing.powf(dt * 60.0);
        for i in 0..self.o.bars {
            let t = self.target[i].min(1.0);
            let s = self.shown[i];
            self.shown[i] = if t >= s {
                self.fall[i] = 0.0;
                s * keep + t * (1.0 - keep)
            } else {
                self.fall[i] += self.o.gravity * dt;
                t.max(s - self.fall[i] * dt)
            };
        }
        Ok(&self.shown)
    }
}
