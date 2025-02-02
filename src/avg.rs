/// General purpose O(1) rolling average calculator.
/// Keeps track of the last L values and provides a rolling average of them.
pub struct Hysterysiser<const L: usize> {
    values: [f32; L],
    index: usize,
    sum: usize,
    ready: bool,
}

impl<const L: usize> Hysterysiser<L> {
    pub fn new() -> Self {
        Self {
            values: [0.0; L],
            index: 0,
            sum: 0,
            ready: false,
        }
    }

    /// Push a new value into the readings to be avaeraged.
    pub fn push(&mut self, value: f32) {
        // Subtract the oldest value from the sum, then swap it for the new value and re-add it to the sum.
        // This way we don't have to iterate over the whole array to calculate the average every time.
        self.sum -= self.values[self.index] as usize;
        self.values[self.index] = value;
        self.sum += value as usize;

        // Set the ready flag if we've filled the array for the first time.
        // This means that the average will be of good quality from now on.
        if !self.ready && self.index == L - 1 {
            self.ready = true;
        }

        self.index = (self.index + 1) % L;
    }

    /// Get the rolling average of the last L values, or
    /// None if there aren't enough readings yet.
    pub fn average(&self) -> Option<f32> {
        if !self.ready {
            return None;
        }

        Some(self.sum as f32 / L as f32)
    }
}
