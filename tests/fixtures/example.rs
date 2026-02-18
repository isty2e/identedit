pub struct Processor {
    value: i32,
}

pub enum Mode {
    Fast,
    Slow,
}

impl Processor {
    pub fn helper(&self) -> i32 {
        self.value + 1
    }
}

pub fn process_data(value: i32) -> i32 {
    value + 1
}
