use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Params {
    pub tiling: bool,
    pub window_size: f64,
    pub overlap: f64,
    pub threshold: f64,
    pub iou: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            tiling: true,
            window_size: 0.3,
            overlap: 0.1,
            threshold: 0.3,
            iou: 0.5,
        }
    }
}

impl Params {
    /// Returns Err(message) matching the Python error strings, or Ok(()).
    /// Note: the `tiling` type check and "Missing {k}" errors are handled at the
    /// HTTP layer via serde deserialization before `validate` runs.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.window_size > 0.0 && self.window_size <= 1.0) {
            return Err("window_size should be within (0, 1]".into());
        }
        if !(self.overlap >= 0.0 && self.overlap < 1.0) {
            return Err("overlap should be within [0, 1)".into());
        }
        if !(self.threshold >= 0.0 && self.threshold <= 1.0) {
            return Err("threshold should be within [0, 1]".into());
        }
        if !(self.iou >= 0.0 && self.iou <= 1.0) {
            return Err("iou should be within [0, 1]".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> Params {
        Params::default()
    }

    #[test]
    fn default_is_valid() {
        assert!(ok().validate().is_ok());
    }

    #[test]
    fn window_size_must_be_in_half_open() {
        let mut p = ok();
        p.window_size = 0.0;
        assert_eq!(
            p.validate().unwrap_err(),
            "window_size should be within (0, 1]"
        );
        let mut p = ok();
        p.window_size = 1.0; // upper bound allowed
        assert!(p.validate().is_ok());
        let mut p = ok();
        p.window_size = 1.1;
        assert!(p.validate().is_err());
    }

    #[test]
    fn overlap_excludes_one() {
        let mut p = ok();
        p.overlap = 0.0; // lower bound allowed
        assert!(p.validate().is_ok());
        let mut p = ok();
        p.overlap = 1.0;
        assert_eq!(p.validate().unwrap_err(), "overlap should be within [0, 1)");
    }

    #[test]
    fn threshold_and_iou_inclusive() {
        let mut p = ok();
        p.threshold = 0.0;
        assert!(p.validate().is_ok());
        let mut p = ok();
        p.threshold = 1.0;
        assert!(p.validate().is_ok());
        let mut p = ok();
        p.threshold = 1.01;
        assert_eq!(
            p.validate().unwrap_err(),
            "threshold should be within [0, 1]"
        );
        let mut p = ok();
        p.iou = -0.1;
        assert_eq!(p.validate().unwrap_err(), "iou should be within [0, 1]");
    }
}
