use std::collections::HashMap;

use crate::smc::Smc;

pub struct Temps {
    keys: Vec<String>,
    /// Current plausible readings only; unavailable sensors are retried on refresh.
    pub temps: HashMap<String, f32>,
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
                if matches!(v.type_str().as_str(), "flt " | "sp78")
                    && let Some(t) = v.as_f32().filter(|t| plausible(*t))
                {
                    temps.insert(k, t);
                }
            }
        }
        let keys = temps.keys().cloned().collect();
        Temps { keys, temps }
    }

    pub fn avg(&self) -> f32 {
        if self.temps.is_empty() {
            return 0.0;
        }
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
        self.refresh_with(|key| smc.read(key).ok().and_then(|v| v.as_f32()));
    }

    fn refresh_with(&mut self, mut read: impl FnMut(&str) -> Option<f32>) {
        self.temps.clear();
        for key in &self.keys {
            if let Some(t) = read(key).filter(|t| plausible(*t)) {
                self.temps.insert(key.clone(), t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temperatures() -> Temps {
        Temps {
            keys: vec!["TC0P".into(), "TG0P".into()],
            temps: HashMap::from([("TC0P".into(), 40.0), ("TG0P".into(), 80.0)]),
        }
    }

    #[test]
    fn aggregates_use_only_current_readings() {
        let mut temps = temperatures();
        assert_eq!(temps.avg(), 60.0);
        assert_eq!(
            temps.hottest().map(|(k, t)| (k.as_str(), *t)),
            Some(("TG0P", 80.0))
        );
        temps.refresh_with(|key| (key == "TC0P").then_some(50.0));
        assert_eq!(temps.sensor_count(), 1);
        assert_eq!(temps.avg(), 50.0);
        assert_eq!(temps.hottest().map(|(_, t)| *t), Some(50.0));
        assert!(!temps.temps.contains_key("TG0P"));
    }

    #[test]
    fn failed_sensors_are_removed_and_retried() {
        let mut temps = temperatures();
        temps.refresh_with(|_| None);
        assert_eq!(temps.sensor_count(), 0);
        assert_eq!(temps.avg(), 0.0);
        assert!(temps.hottest().is_none());
        temps.refresh_with(|_| Some(65.0));
        assert_eq!(temps.sensor_count(), 2);
        assert_eq!(temps.avg(), 65.0);
    }

    #[test]
    fn invalid_readings_are_excluded_and_can_recover() {
        let mut temps = temperatures();
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 14.0, 116.0] {
            temps.refresh_with(|_| Some(invalid));
            assert!(temps.temps.is_empty());
            assert!(temps.hottest().is_none());
            assert_eq!(temps.avg(), 0.0);
        }
        temps.refresh_with(|key| Some(if key == "TC0P" { 15.0 } else { 115.0 }));
        assert_eq!(temps.sensor_count(), 2);
        assert_eq!(temps.avg(), 65.0);
        assert_eq!(temps.hottest().map(|(_, t)| *t), Some(115.0));
    }
}
