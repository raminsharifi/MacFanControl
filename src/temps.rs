use std::collections::HashMap;

use crate::smc::Smc;

pub struct Temps {
    temps: HashMap<String, f32>,
}

fn plausible(t: f32) -> bool {
    (15.0..=115.0).contains(&t)
}

impl Temps {
    pub fn discover(smc: &Smc) -> Temps {
        let mut temps: HashMap<String, f32> = Default::default();
        if let Ok(count) = smc.key_count() {
            for i in 0..count {
                let Ok(k) = smc.key_at(i) else { continue };
                if !k.starts_with('T') {
                    continue;
                }
                let Ok(v) = smc.read(&k) else { continue };
                if matches!(v.type_str().as_str(), "flt " | "sp78") {
                    if let Some(v) = v.as_f32() {
                        if plausible(v) {
                            temps.insert(k, v);
                        }
                    }
                }
            }
        }
        Temps { temps }
    }

    pub fn avg(&self) -> f32 {
        let sum: f32 = self.temps.values().copied().sum();
        sum / self.temps.len() as f32
    }

    pub fn hottest(&self) -> Option<(&String, &f32)> {
        self.temps.iter().max_by(|a, b| a.1.total_cmp(b.1))
    }

    pub fn sensor_count(&self) -> usize {
        self.temps.len()
    }

    pub fn refresh(&mut self, smc: &Smc) {
        for (k, v) in self.temps.iter_mut() {
            let Some(t) = smc.read(k).ok().and_then(|v| v.as_f32()) else {
                continue;
            };
            if !plausible(t) {
                continue;
            }
            *v = t;
        }
    }
}
