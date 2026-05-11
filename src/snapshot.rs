#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Sampler {
    _private: (),
}

impl Sampler {
    pub fn sample(&mut self) -> Snapshot {
        Snapshot::default()
    }
}
