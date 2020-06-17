#[derive(Copy, Clone, Debug)]
pub struct Color(u32);

impl Color {
    pub fn new(value: u32) -> Color {
        Color(value)
    }

    pub const fn r(self) -> u8 {
        ((self.0 >> 16) & 255) as u8
    }

    pub const fn g(self) -> u8 {
        ((self.0 >> 8) & 255) as u8
    }

    pub const fn b(self) -> u8 {
        (self.0 & 255) as u8
    }

    pub fn as_8bit(self) -> u8 {
        let r = (u16::from(self.r()) * 5 / 255) as u8;
        let g = (u16::from(self.g()) * 5 / 255) as u8;
        let b = (u16::from(self.b()) * 5 / 255) as u8;
        16 + 36 * r + 6 * g + b
    }
}
