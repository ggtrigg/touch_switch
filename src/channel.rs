#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TouchState {
    Warmup,
    Idle,
    Short,
    Long,
}

pub struct Channel {
    warmup: u32,
    level_lo: u32,
    level_hi: u32,
    level: f32,
    last_state: bool,
    last_touch_state: TouchState,
    counter: u32,
}

impl Default for Channel {
    fn default() -> Self {
        Channel {
            warmup: 100,
            level_lo: u32::MAX,
            level_hi: 0,
            level: 0.0,
            last_state: false,
            last_touch_state: TouchState::Idle,
            counter: 0,
        }
    }
}

const LONG_THRESHOLD: u32 = 300;

impl Channel {
    pub fn new() -> Self {
        Self::default()
    }

    fn normalize(&mut self, raw_val: u32) -> Option<f32> {
        self.level_lo = self.level_lo.min(raw_val);
        self.level_hi = self.level_hi.max(raw_val);

        let window = self.level_hi - self.level_lo;
        if window > 24 {
            self.level = 1.0 - (raw_val - self.level_lo) as f32 / window as f32;
            Some(self.level)
        } else {
            None
        }
    }

    pub fn state(&mut self, raw_val: u32) -> TouchState {
        if self.warmup > 0 {
            self.normalize(raw_val);
            self.warmup -= 1;
            return TouchState::Warmup;
        }

        let level = self.normalize(raw_val);
        let new_state;

        match level {
            Some(lvl) => {
                if self.counter > 200 {
                    match lvl < 0.5 {
                        true => {
                            match self.last_state {
                                true => {
                                    new_state = match self.counter > LONG_THRESHOLD {
                                        true => TouchState::Long,
                                        false => TouchState::Idle,
                                    }
                                }
                                false => {
                                    new_state = TouchState::Idle;
                                }
                            }
                            self.last_state = true;
                            self.count();
                        }
                        false => {
                            match self.last_state {
                                true => {
                                    match self.counter != 0 && self.counter <= LONG_THRESHOLD {
                                        true => new_state = TouchState::Short,
                                        false => new_state = TouchState::Idle,
                                    }
                                    self.counter = 0;
                                }
                                false => {
                                    new_state = TouchState::Idle;
                                }
                            }
                            self.last_state = false;
                            self.counter = 0;
                        }
                    }
                } else {
                    match lvl < 0.5 {
                        true => {
                            self.last_state = true;
                            self.count();
                        }
                        false => {
                            if self.last_state {
                                // Brief tap: release before main branch
                                new_state = match self.counter > 5 {
                                    true => TouchState::Short,
                                    false => TouchState::Idle,
                                };
                                self.last_state = false;
                                self.counter = 0;
                                self.last_touch_state = new_state;
                                return new_state;
                            }
                            self.counter = 0;
                        }
                    }
                    new_state = match self.last_touch_state {
                        TouchState::Short | TouchState::Long | TouchState::Warmup => TouchState::Idle,
                        state => state,
                    };
                }
            }
            None => {
                new_state = TouchState::Idle;
            }
        }
        self.last_touch_state = new_state;
        new_state
    }

    fn count(&mut self) {
        self.counter = self.counter.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Warmup ---

    #[test]
    fn test_warmup_returns_warmup_for_100_calls() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            assert_eq!(ch.state(1000), TouchState::Warmup);
        }
    }

    #[test]
    fn test_after_warmup_state_is_idle_when_window_small() {
        let mut ch = Channel::new();
        for _ in 0..101 {
            ch.state(1000);
        }
        assert_eq!(ch.state(1000), TouchState::Idle);
    }

    // --- Normalize / window threshold ---

    #[test]
    fn test_state_is_idle_when_window_below_25() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }
        // values within 20 → window < 25 → normalize returns None → Idle
        for _ in 0..50 {
            assert_eq!(ch.state(1010), TouchState::Idle);
            assert_eq!(ch.state(1005), TouchState::Idle);
        }
    }

    #[test]
    fn test_state_returns_idle_after_window_exceeds_24() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }
        // Jump to 1100 → window = 100 > 24 → normalize returns Some
        // Brief hold → counter < 200 → returns Idle (debouncing)
        for _ in 0..10 {
            assert_eq!(ch.state(1100), TouchState::Idle);
        }
    }

    // --- Short touch ---

    #[test]
    fn test_short_touch_detected_on_release() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Touch: values jump to 1100, window = 100 → normalize returns level
        // Hold past debounce (200 samples)
        for i in 0..220 {
            assert_eq!(ch.state(1100), TouchState::Idle, "holding: {}", i);
        }

        // Release: values return to baseline → level >= 0.5 → Short
        assert_eq!(ch.state(1000), TouchState::Short);
    }

    #[test]
    fn test_short_touch_one_frame_is_idle() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Single noise frame — too short to trigger Short
        ch.state(1100);
        assert_eq!(ch.state(1000), TouchState::Idle);
    }

    #[test]
    fn test_short_touch_brief_is_short() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Brief but real tap — enough frames to exceed noise threshold
        for _ in 0..50 {
            ch.state(1100);
        }
        assert_eq!(ch.state(1000), TouchState::Short);
    }

    // --- Long touch ---

    #[test]
    fn test_long_touch_detected_after_300_samples() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Counter builds from 0. After 300 frames: counter = 301 → > 300 → Long
        for _ in 0..301 {
            ch.state(1100);
        }
        assert_eq!(ch.state(1100), TouchState::Long);
    }

    #[test]
    fn test_long_touch_releases_to_idle_not_short() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Hold well past Long threshold
        for _ in 0..400 {
            ch.state(1100);
        }

        // Release: counter > LONG_THRESHOLD → Short condition fails → Idle
        assert_eq!(ch.state(1000), TouchState::Idle);
    }

    #[test]
    fn test_long_touch_releases_and_then_short_on_next_tap() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Long touch hold
        for _ in 0..400 {
            ch.state(1100);
        }
        // Release (counter > LONG_THRESHOLD → Idle, not Short)
        assert_eq!(ch.state(1000), TouchState::Idle);

        // New short touch
        for _ in 0..250 {
            ch.state(1100);
        }
        // Release (counter <= LONG_THRESHOLD → Short)
        assert_eq!(ch.state(1000), TouchState::Short);
    }

    // --- Normalize level values ---

    #[test]
    fn test_normalize_level_is_0_at_max_touch() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Touch shifts raw to 1100 (max), window = 100
        // normalize returns level = 1.0 - (1100-1000)/100 = 0.0
        ch.state(1100);
        // level < 0.5 → touching branch
        // Wait for debounce
        for _ in 0..200 {
            ch.state(1100);
        }
        // After debounce: still touching, counter ≤ LONG_THRESHOLD → Idle
        assert_eq!(ch.state(1100), TouchState::Idle);
    }

    #[test]
    fn test_normalize_level_is_1_at_baseline_min() {
        let mut ch = Channel::new();
        for _ in 0..100 {
            ch.state(1000);
        }

        // Trigger window
        ch.state(1100);
        // Back to baseline min
        let s = ch.state(1000);
        // level = 1.0 - (1000-1000)/100 = 1.0
        // level >= 0.5 → not-touching branch
        assert_eq!(s, TouchState::Idle);
    }
}
