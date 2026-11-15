use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Result, ensure};

/// An opaque sRGB color. Palette data cannot carry template code or alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl FromStr for Color {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        ensure!(
            s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid color {s:?}: expected #RRGGBB"
        );
        Ok(Self(
            u8::from_str_radix(&s[1..3], 16)?,
            u8::from_str_radix(&s[3..5], 16)?,
            u8::from_str_radix(&s[5..7], 16)?,
        ))
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

fn linear(v: u8) -> f64 {
    let v = f64::from(v) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(v: f64) -> u8 {
    let v = if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Color {
    pub fn luminance(self) -> f64 {
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2)
    }

    pub fn contrast(self, other: Self) -> f64 {
        let a = self.luminance();
        let b = other.luminance();
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Björn Ottosson's OKLab conversion, D65 sRGB.
    pub fn oklab(self) -> [f64; 3] {
        let (r, g, b) = (linear(self.0), linear(self.1), linear(self.2));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        [
            0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        ]
    }

    fn from_lab(l: f64, a: f64, b: f64) -> [f64; 3] {
        let ll = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let mm = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let ss = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
        [
            4.0767416621 * ll - 3.3077115913 * mm + 0.2309699292 * ss,
            -1.2684380046 * ll + 2.6097574011 * mm - 0.3413193965 * ss,
            -0.0041960863 * ll - 0.7034186147 * mm + 1.7076147010 * ss,
        ]
    }

    /// Reduce chroma until the color fits sRGB, preserving hue and lightness.
    pub fn lch(l: f64, mut c: f64, h: f64) -> Self {
        let h = h.to_radians();
        for _ in 0..100 {
            let rgb = Self::from_lab(l, c * h.cos(), c * h.sin());
            if rgb.iter().all(|v| (0.0..=1.0).contains(v)) || c < 0.00001 {
                return Self(srgb(rgb[0]), srgb(rgb[1]), srgb(rgb[2]));
            }
            c *= 0.9;
        }
        unreachable!("chroma reduction converges within 100 iterations")
    }

    pub fn hue(self) -> f64 {
        let [_, a, b] = self.oklab();
        b.atan2(a).to_degrees().rem_euclid(360.0)
    }

    pub fn distance(self, other: Self) -> f64 {
        distance(self.oklab(), other.oklab()).sqrt()
    }

    pub fn mix(self, other: Self, amount: f64) -> Self {
        let mix =
            |a: u8, b: u8| (f64::from(a) * (1.0 - amount) + f64::from(b) * amount).round() as u8;
        Self(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }
}

pub fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum()
}
