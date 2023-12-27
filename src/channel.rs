
#[derive(Clone, Copy)]
pub enum TouchState {
    Warmup,
    Idle,
    Short,
    Long
}

pub struct Channel {
  warmup: u32,
  level_lo: u32,
  level_hi: u32,
  level: f32,
  last_state: bool,
  last_touch_state: TouchState,
  counter: u32
}

impl Channel {
  pub fn new() -> Self {
      Channel {
          warmup: 100,
          level_lo: u32::MAX,
          level_hi: 0,
          level: 0.0,
          last_state: false,
          last_touch_state: TouchState::Idle,
          counter: 0
      }
  }

  fn normalize(&mut self, raw_val: u32) -> Option<f32> {
      if self.warmup > 0 {
          self.warmup -= 1;
          None
      } else {
          self.level_lo = self.level_lo.min(raw_val);
          self.level_hi = self.level_hi.max(raw_val);

          let window = self.level_hi - self.level_lo;
          if window > 64 {
              self.level = 1.0 - (raw_val - self.level_lo) as f32 / window as f32;
              Some(self.level)
          } else {
              None
          }
      }
  }

  pub fn state(&mut self, raw_val: u32) -> TouchState {
      let level = self.normalize(raw_val);
      let new_state;

      if self.warmup > 0 {
          new_state = TouchState::Warmup;
      } else {
          match level {
              Some(lvl) => {
                  if self.counter > 100 {      // Debounce
                      match lvl < 0.5 {
                          true => {
                              match self.last_state {
                                  true => {
                                      new_state = match self.counter > 2_000 {
                                          true => TouchState::Long,
                                          false => TouchState::Idle
                                      }
                                  }
                                  false => {
                                      // Finger touched, start counting
                                      new_state = TouchState::Idle;
                                      self.counter = 0;
                                  }
                              }
                              self.last_state = true;
                              self.count();
                          }
                          false => {
                              match self.last_state {
                                  true => {   // Finger lifted
                                      match self.counter != 0 && self.counter <= 2_000 {
                                          true => { new_state = TouchState::Short }
                                          false => { new_state = TouchState::Idle }
                                      }
                                      self.counter = 0;
                                  }
                                  false => {
                                      new_state = TouchState::Idle;
                                  }
                              }
                              self.last_state = false;
                              self.count();
                          }
                      }
                  } else {
                      self.count();
                      new_state = self.last_touch_state;
                  }
              }
              None => { new_state = TouchState::Idle; }
          };
      }
      self.last_touch_state = new_state;
      new_state
  }

  fn count(&mut self) {
      self.counter = match self.counter.checked_add(1) {
          Some(val) => val,
          None => u32::MAX
      };
  }
}
