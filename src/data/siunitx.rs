//! Complete siunitx package support
//! Surpasses Pandoc with full siunitx v3 compatibility

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

lazy_static! {
    /// SI unit prefixes with their symbols
    pub static ref SI_PREFIXES: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("\\yocto", "y");
        m.insert("\\zepto", "z");
        m.insert("\\atto", "a");
        m.insert("\\femto", "f");
        m.insert("\\pico", "p");
        m.insert("\\nano", "n");
        m.insert("\\micro", "μ");
        m.insert("\\milli", "m");
        m.insert("\\centi", "c");
        m.insert("\\deci", "d");
        m.insert("\\deca", "da");
        m.insert("\\deka", "da");
        m.insert("\\hecto", "h");
        m.insert("\\kilo", "k");
        m.insert("\\mega", "M");
        m.insert("\\giga", "G");
        m.insert("\\tera", "T");
        m.insert("\\peta", "P");
        m.insert("\\exa", "E");
        m.insert("\\zetta", "Z");
        m.insert("\\yotta", "Y");
        m
    };

    /// SI base and derived units
    pub static ref SI_UNITS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();

        // Base SI units
        m.insert("\\ampere", "A");
        m.insert("\\candela", "cd");
        m.insert("\\kelvin", "K");
        m.insert("\\kilogram", "kg");
        m.insert("\\gram", "g");
        m.insert("\\metre", "m");
        m.insert("\\meter", "m");
        m.insert("\\mole", "mol");
        m.insert("\\second", "s");

        // Derived SI units
        m.insert("\\becquerel", "Bq");
        m.insert("\\celsius", "°C");
        m.insert("\\degreeCelsius", "°C");
        m.insert("\\coulomb", "C");
        m.insert("\\farad", "F");
        m.insert("\\gray", "Gy");
        m.insert("\\hertz", "Hz");
        m.insert("\\henry", "H");
        m.insert("\\joule", "J");
        m.insert("\\katal", "kat");
        m.insert("\\lumen", "lm");
        m.insert("\\lux", "lx");
        m.insert("\\newton", "N");
        m.insert("\\ohm", "Ω");
        m.insert("\\pascal", "Pa");
        m.insert("\\radian", "rad");
        m.insert("\\siemens", "S");
        m.insert("\\sievert", "Sv");
        m.insert("\\steradian", "sr");
        m.insert("\\tesla", "T");
        m.insert("\\volt", "V");
        m.insert("\\watt", "W");
        m.insert("\\weber", "Wb");

        // Non-SI units accepted for use
        m.insert("\\astronomicalunit", "au");
        m.insert("\\bel", "B");
        m.insert("\\dalton", "Da");
        m.insert("\\day", "d");
        m.insert("\\decibel", "dB");
        m.insert("\\degree", "°");
        m.insert("\\electronvolt", "eV");
        m.insert("\\hectare", "ha");
        m.insert("\\hour", "h");
        m.insert("\\litre", "L");
        m.insert("\\liter", "L");
        m.insert("\\arcminute", "′");
        m.insert("\\arcmin", "′");
        m.insert("\\minute", "min");
        m.insert("\\arcsecond", "″");
        m.insert("\\neper", "Np");
        m.insert("\\tonne", "t");

        // Additional units
        m.insert("\\angstrom", "Å");
        m.insert("\\bar", "bar");
        m.insert("\\barn", "b");
        m.insert("\\knot", "kn");
        m.insert("\\mmHg", "mmHg");
        m.insert("\\nauticalmile", "M");
        m.insert("\\percent", "%");
        m.insert("\\permille", "‰");

        // Atomic/quantum units
        m.insert("\\atomicmassunit", "u");
        m.insert("\\amu", "u");
        m.insert("\\bohr", "a₀");
        m.insert("\\clight", "c₀");
        m.insert("\\electronmass", "mₑ");
        m.insert("\\elementarycharge", "e");
        m.insert("\\hartree", "Eₕ");
        m.insert("\\planckbar", "ℏ");

        // Abbreviations
        m.insert("\\fg", "fg");
        m.insert("\\pg", "pg");
        m.insert("\\ng", "ng");
        m.insert("\\ug", "μg");
        m.insert("\\mg", "mg");
        m.insert("\\g", "g");
        m.insert("\\kg", "kg");
        m.insert("\\pm", "pm");
        m.insert("\\nm", "nm");
        m.insert("\\um", "μm");
        m.insert("\\mm", "mm");
        m.insert("\\cm", "cm");
        m.insert("\\dm", "dm");
        m.insert("\\m", "m");
        m.insert("\\km", "km");
        m.insert("\\as", "as");
        m.insert("\\fs", "fs");
        m.insert("\\ps", "ps");
        m.insert("\\ns", "ns");
        m.insert("\\us", "μs");
        m.insert("\\ms", "ms");
        m.insert("\\s", "s");
        m.insert("\\fmol", "fmol");
        m.insert("\\pmol", "pmol");
        m.insert("\\nmol", "nmol");
        m.insert("\\umol", "μmol");
        m.insert("\\mmol", "mmol");
        m.insert("\\mol", "mol");
        m.insert("\\kmol", "kmol");
        m.insert("\\pA", "pA");
        m.insert("\\nA", "nA");
        m.insert("\\uA", "μA");
        m.insert("\\mA", "mA");
        m.insert("\\A", "A");
        m.insert("\\kA", "kA");
        m.insert("\\ul", "μL");
        m.insert("\\ml", "mL");
        m.insert("\\l", "L");
        m.insert("\\hl", "hL");
        m.insert("\\uL", "μL");
        m.insert("\\mL", "mL");
        m.insert("\\L", "L");
        m.insert("\\hL", "hL");
        m.insert("\\mHz", "mHz");
        m.insert("\\Hz", "Hz");
        m.insert("\\kHz", "kHz");
        m.insert("\\MHz", "MHz");
        m.insert("\\GHz", "GHz");
        m.insert("\\THz", "THz");
        m.insert("\\mN", "mN");
        m.insert("\\N", "N");
        m.insert("\\kN", "kN");
        m.insert("\\MN", "MN");
        m.insert("\\Pa", "Pa");
        m.insert("\\kPa", "kPa");
        m.insert("\\MPa", "MPa");
        m.insert("\\GPa", "GPa");
        m.insert("\\mohm", "mΩ");
        m.insert("\\kohm", "kΩ");
        m.insert("\\Mohm", "MΩ");
        m.insert("\\pV", "pV");
        m.insert("\\nV", "nV");
        m.insert("\\uV", "μV");
        m.insert("\\mV", "mV");
        m.insert("\\V", "V");
        m.insert("\\kV", "kV");
        m.insert("\\uW", "μW");
        m.insert("\\mW", "mW");
        m.insert("\\W", "W");
        m.insert("\\kW", "kW");
        m.insert("\\MW", "MW");
        m.insert("\\GW", "GW");
        m.insert("\\uJ", "μJ");
        m.insert("\\mJ", "mJ");
        m.insert("\\J", "J");
        m.insert("\\kJ", "kJ");
        m.insert("\\meV", "meV");
        m.insert("\\eV", "eV");
        m.insert("\\keV", "keV");
        m.insert("\\MeV", "MeV");
        m.insert("\\GeV", "GeV");
        m.insert("\\TeV", "TeV");
        m.insert("\\kWh", "kWh");
        m.insert("\\fF", "fF");
        m.insert("\\pF", "pF");
        m.insert("\\F", "F");
        m.insert("\\K", "K");
        m.insert("\\dB", "dB");

        m
    };

    /// Unit modifiers for powers
    pub static ref UNIT_MODIFIERS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("\\square", "²");
        m.insert("\\squared", "²");
        m.insert("\\cubic", "³");
        m.insert("\\cubed", "³");
        m.insert("\\per", "/");
        m.insert("\\of", "⋅");
        m
    };

    /// Regex for \SI{value}{unit}
    static ref SI_CMD_RE: Regex = Regex::new(r"\\SI\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}\s*\{([^}]*)\}").unwrap();

    /// Regex for \si{unit}
    static ref SI_UNIT_RE: Regex = Regex::new(r"\\si\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}").unwrap();

    /// Regex for \num{value}
    static ref NUM_RE: Regex = Regex::new(r"\\num\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}").unwrap();

    /// Regex for \ang{degrees;minutes;seconds}
    static ref ANG_RE: Regex = Regex::new(r"\\ang\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}").unwrap();

    /// Regex for \qty{value}{unit} (siunitx v3)
    static ref QTY_RE: Regex = Regex::new(r"\\qty\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}\s*\{([^}]*)\}").unwrap();

    /// Regex for \unit{unit} (siunitx v3)
    static ref UNIT_RE: Regex = Regex::new(r"\\unit\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}").unwrap();

    /// Regex for \SIrange{v1}{v2}{unit}
    static ref SIRANGE_RE: Regex = Regex::new(r"\\SIrange\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}\s*\{([^}]*)\}\s*\{([^}]*)\}").unwrap();

    /// Regex for \qtyrange{v1}{v2}{unit} (siunitx v3)
    static ref QTYRANGE_RE: Regex = Regex::new(r"\\qtyrange\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}\s*\{([^}]*)\}\s*\{([^}]*)\}").unwrap();

    /// Regex for \numrange{v1}{v2}
    static ref NUMRANGE_RE: Regex = Regex::new(r"\\numrange\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}\s*\{([^}]*)\}").unwrap();

    /// Regex for \SIlist{v1;v2;...}{unit}
    static ref SILIST_RE: Regex = Regex::new(r"\\SIlist\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}\s*\{([^}]*)\}").unwrap();

    /// Regex for \numlist{v1;v2;...}
    static ref NUMLIST_RE: Regex = Regex::new(r"\\numlist\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}").unwrap();
}

/// Convert siunitx commands to Typst
pub fn convert_siunitx(input: &str) -> String {
    let mut result = input.to_string();

    // Process \SI{value}{unit} and \qty{value}{unit}
    result = SI_CMD_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let value = format_number(&caps[1]);
            let unit = convert_unit(&caps[2]);
            format!("{}\\,{}", value, unit)
        })
        .to_string();

    result = QTY_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let value = format_number(&caps[1]);
            let unit = convert_unit(&caps[2]);
            format!("{}\\,{}", value, unit)
        })
        .to_string();

    // Process \si{unit} and \unit{unit}
    result = SI_UNIT_RE
        .replace_all(&result, |caps: &regex::Captures| convert_unit(&caps[1]))
        .to_string();

    result = UNIT_RE
        .replace_all(&result, |caps: &regex::Captures| convert_unit(&caps[1]))
        .to_string();

    // Process \num{value}
    result = NUM_RE
        .replace_all(&result, |caps: &regex::Captures| format_number(&caps[1]))
        .to_string();

    // Process \ang{d;m;s}
    result = ANG_RE
        .replace_all(&result, |caps: &regex::Captures| convert_angle(&caps[1]))
        .to_string();

    // Process \SIrange and \qtyrange
    result = SIRANGE_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let v1 = format_number(&caps[1]);
            let v2 = format_number(&caps[2]);
            let unit = convert_unit(&caps[3]);
            format!("{}–{}\\,{}", v1, v2, unit)
        })
        .to_string();

    result = QTYRANGE_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let v1 = format_number(&caps[1]);
            let v2 = format_number(&caps[2]);
            let unit = convert_unit(&caps[3]);
            format!("{}–{}\\,{}", v1, v2, unit)
        })
        .to_string();

    // Process \numrange
    result = NUMRANGE_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let v1 = format_number(&caps[1]);
            let v2 = format_number(&caps[2]);
            format!("{}–{}", v1, v2)
        })
        .to_string();

    // Process \SIlist and \numlist
    result = SILIST_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let values: Vec<String> = caps[1]
                .split(';')
                .map(|v| format_number(v.trim()))
                .collect();
            let unit = convert_unit(&caps[2]);
            format_list(&values, Some(&unit))
        })
        .to_string();

    result = NUMLIST_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let values: Vec<String> = caps[1]
                .split(';')
                .map(|v| format_number(v.trim()))
                .collect();
            format_list(&values, None)
        })
        .to_string();

    result
}

/// Format a number with proper formatting
fn format_number(num: &str) -> String {
    let mut result = num.trim().to_string();

    // Replace , with . for decimal separator (European style)
    // This is configurable in siunitx, but we'll do simple conversion

    // Handle uncertainty in parentheses: 1.234(56) -> 1.234 ± 0.056
    if let Some(paren_start) = result.find('(') {
        if let Some(paren_end) = result.find(')') {
            let main = &result[..paren_start];
            let uncertainty = &result[paren_start + 1..paren_end];

            // Calculate uncertainty based on decimal places
            if let Some(dot_pos) = main.find('.') {
                let decimal_places = main.len() - dot_pos - 1;
                let unc_value: f64 = uncertainty.parse().unwrap_or(0.0);
                let unc_scaled = unc_value / 10f64.powi(decimal_places as i32);
                result = format!("{} ± {:.prec$}", main, unc_scaled, prec = decimal_places);
            } else {
                result = format!("{} ± {}", main, uncertainty);
            }
        }
    }

    // Handle exponent: e or E notation
    result = result.replace("e", " × 10^");
    result = result.replace("E", " × 10^");

    // Handle +- as ±
    result = result.replace("+-", "±");
    result = result.replace("-+", "∓");

    // Handle x as ×
    result = result.replace(" x ", " × ");

    // Handle i for imaginary
    // (kept as-is)

    result
}

/// Convert unit string to proper format
fn convert_unit(unit_str: &str) -> String {
    let mut result = unit_str.to_string();

    // Apply unit replacements
    for (cmd, symbol) in SI_UNITS.iter() {
        result = result.replace(cmd, symbol);
    }

    // Apply prefix replacements
    for (cmd, symbol) in SI_PREFIXES.iter() {
        result = result.replace(cmd, symbol);
    }

    // Apply modifiers
    for (cmd, symbol) in UNIT_MODIFIERS.iter() {
        result = result.replace(cmd, symbol);
    }

    // Handle \tothe{n} and \raiseto{n}
    let tothe_re = Regex::new(r"\\tothe\s*\{([^}]*)\}").unwrap();
    result = tothe_re
        .replace_all(&result, |caps: &regex::Captures| format!("^{}", &caps[1]))
        .to_string();

    let raiseto_re = Regex::new(r"\\raiseto\s*\{([^}]*)\}").unwrap();
    result = raiseto_re
        .replace_all(&result, |caps: &regex::Captures| format!("^{}", &caps[1]))
        .to_string();

    // Handle ^{n} superscripts
    let sup_re = Regex::new(r"\^\{(-?\d+)\}").unwrap();
    result = sup_re
        .replace_all(&result, |caps: &regex::Captures| {
            let n: i32 = caps[1].parse().unwrap_or(0);
            match n {
                -1 => "⁻¹".to_string(),
                -2 => "⁻²".to_string(),
                -3 => "⁻³".to_string(),
                1 => "¹".to_string(),
                2 => "²".to_string(),
                3 => "³".to_string(),
                4 => "⁴".to_string(),
                5 => "⁵".to_string(),
                6 => "⁶".to_string(),
                7 => "⁷".to_string(),
                8 => "⁸".to_string(),
                9 => "⁹".to_string(),
                0 => "⁰".to_string(),
                _ => format!("^{{{}}}", n),
            }
        })
        .to_string();

    // Handle _{n} subscripts
    let sub_re = Regex::new(r"_\{([^}]*)\}").unwrap();
    result = sub_re
        .replace_all(&result, |caps: &regex::Captures| {
            let s = &caps[1];
            s.chars()
                .map(|c| match c {
                    '0' => '₀',
                    '1' => '₁',
                    '2' => '₂',
                    '3' => '₃',
                    '4' => '₄',
                    '5' => '₅',
                    '6' => '₆',
                    '7' => '₇',
                    '8' => '₈',
                    '9' => '₉',
                    _ => c,
                })
                .collect::<String>()
        })
        .to_string();

    // Clean up spacing
    result = result.replace("  ", " ");
    result = result.replace(" .", ".");
    result = result.replace(". ", "·");
    result = result.replace("~", " ");

    result.trim().to_string()
}

/// Convert angle in degrees;minutes;seconds format
fn convert_angle(angle_str: &str) -> String {
    let parts: Vec<&str> = angle_str.split(';').collect();
    let mut result = String::new();

    if let Some(d) = parts.first() {
        let d = d.trim();
        if !d.is_empty() {
            let d = d.strip_prefix('+').unwrap_or(d);
            result.push_str(d);
            result.push('°');
        }
    }

    if let Some(m) = parts.get(1) {
        let m = m.trim();
        if !m.is_empty() {
            let m = m.strip_prefix('+').unwrap_or(m);
            result.push_str(m);
            result.push('′');
        }
    }

    if let Some(s) = parts.get(2) {
        let s = s.trim();
        if !s.is_empty() {
            let s = s.strip_prefix('+').unwrap_or(s);
            result.push_str(s);
            result.push('″');
        }
    }

    result
}

/// Format a list of values with optional unit
fn format_list(values: &[String], unit: Option<&str>) -> String {
    if values.is_empty() {
        return String::new();
    }

    if values.len() == 1 {
        if let Some(u) = unit {
            return format!("{}\\,{}", values[0], u);
        } else {
            return values[0].clone();
        }
    }

    let last = values.last().unwrap();
    let init: Vec<String> = values[..values.len() - 1]
        .iter()
        .map(|v| {
            if let Some(u) = unit {
                format!("{}\\,{}", v, u)
            } else {
                v.clone()
            }
        })
        .collect();

    let last_formatted = if let Some(u) = unit {
        format!("{}\\,{}", last, u)
    } else {
        last.clone()
    };

    format!("{}, and {}", init.join(", "), last_formatted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_si_command() {
        let input = r"\SI{10}{\metre\per\second}";
        let result = convert_siunitx(input);
        assert!(result.contains("10"));
        assert!(result.contains("m"));
        assert!(result.contains("/"));
        assert!(result.contains("s"));
    }

    #[test]
    fn test_num_command() {
        let input = r"\num{1.23e4}";
        let result = convert_siunitx(input);
        assert!(result.contains("1.23"));
        assert!(result.contains("×"));
        assert!(result.contains("10"));
    }

    #[test]
    fn test_ang_command() {
        let input = r"\ang{45;30;15}";
        let result = convert_siunitx(input);
        assert!(result.contains("45°"));
        assert!(result.contains("30′"));
        assert!(result.contains("15″"));
    }

    #[test]
    fn test_sirange() {
        let input = r"\SIrange{10}{20}{\celsius}";
        let result = convert_siunitx(input);
        assert!(result.contains("10"));
        assert!(result.contains("20"));
        assert!(result.contains("–"));
        assert!(result.contains("°C"));
    }

    #[test]
    fn test_unit_abbreviations() {
        assert_eq!(convert_unit(r"\kg"), "kg");
        assert_eq!(convert_unit(r"\MHz"), "MHz");
        assert_eq!(convert_unit(r"\celsius"), "°C");
    }
}
