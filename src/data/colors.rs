//! Color command support (xcolor, color packages)
//! Provides comprehensive color handling for LaTeX to Typst conversion

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

lazy_static! {
    /// Named colors from xcolor package (dvipsnames, svgnames, x11names)
    pub static ref NAMED_COLORS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();

        // Basic colors
        m.insert("black", "black");
        m.insert("white", "white");
        m.insert("red", "red");
        m.insert("green", "green");
        m.insert("blue", "blue");
        m.insert("yellow", "yellow");
        m.insert("cyan", "aqua");
        m.insert("magenta", "fuchsia");
        m.insert("orange", "orange");
        m.insert("purple", "purple");
        m.insert("pink", "rgb(\"#FFC0CB\")");
        m.insert("brown", "rgb(\"#A52A2A\")");
        m.insert("gray", "gray");
        m.insert("grey", "gray");
        m.insert("darkgray", "rgb(\"#A9A9A9\")");
        m.insert("darkgrey", "rgb(\"#A9A9A9\")");
        m.insert("lightgray", "rgb(\"#D3D3D3\")");
        m.insert("lightgrey", "rgb(\"#D3D3D3\")");
        m.insert("lime", "lime");
        m.insert("olive", "olive");
        m.insert("teal", "teal");
        m.insert("navy", "navy");
        m.insert("maroon", "maroon");
        m.insert("silver", "silver");
        m.insert("aqua", "aqua");
        m.insert("fuchsia", "fuchsia");

        // dvipsnames (68 colors)
        m.insert("Apricot", "rgb(\"#FBB982\")");
        m.insert("Aquamarine", "rgb(\"#00B5BE\")");
        m.insert("Bittersweet", "rgb(\"#C04F17\")");
        m.insert("Black", "black");
        m.insert("Blue", "blue");
        m.insert("BlueGreen", "rgb(\"#00B5BE\")");
        m.insert("BlueViolet", "rgb(\"#473992\")");
        m.insert("BrickRed", "rgb(\"#B6321C\")");
        m.insert("Brown", "rgb(\"#792500\")");
        m.insert("BurntOrange", "rgb(\"#F7921D\")");
        m.insert("CadetBlue", "rgb(\"#74729A\")");
        m.insert("CarnationPink", "rgb(\"#F282B4\")");
        m.insert("Cerulean", "rgb(\"#00A2E3\")");
        m.insert("CornflowerBlue", "rgb(\"#41B0E4\")");
        m.insert("Cyan", "aqua");
        m.insert("Dandelion", "rgb(\"#FDBC42\")");
        m.insert("DarkOrchid", "rgb(\"#A4538A\")");
        m.insert("Emerald", "rgb(\"#00A99D\")");
        m.insert("ForestGreen", "rgb(\"#009B55\")");
        m.insert("Fuchsia", "rgb(\"#8C368C\")");
        m.insert("Goldenrod", "rgb(\"#FFDF42\")");
        m.insert("Gray", "gray");
        m.insert("Green", "green");
        m.insert("GreenYellow", "rgb(\"#DFE674\")");
        m.insert("JungleGreen", "rgb(\"#00A99A\")");
        m.insert("Lavender", "rgb(\"#F49EC4\")");
        m.insert("LimeGreen", "rgb(\"#8DC73E\")");
        m.insert("Magenta", "fuchsia");
        m.insert("Mahogany", "rgb(\"#A9341F\")");
        m.insert("Maroon", "rgb(\"#AF3235\")");
        m.insert("Melon", "rgb(\"#F89E7B\")");
        m.insert("MidnightBlue", "rgb(\"#006795\")");
        m.insert("Mulberry", "rgb(\"#A93C93\")");
        m.insert("NavyBlue", "rgb(\"#006EB8\")");
        m.insert("OliveGreen", "rgb(\"#3C8031\")");
        m.insert("Orange", "orange");
        m.insert("OrangeRed", "rgb(\"#ED135A\")");
        m.insert("Orchid", "rgb(\"#AF72B0\")");
        m.insert("Peach", "rgb(\"#F7965A\")");
        m.insert("Periwinkle", "rgb(\"#7977B8\")");
        m.insert("PineGreen", "rgb(\"#008B72\")");
        m.insert("Plum", "rgb(\"#92268F\")");
        m.insert("ProcessBlue", "rgb(\"#00B0F0\")");
        m.insert("Purple", "purple");
        m.insert("RawSienna", "rgb(\"#974006\")");
        m.insert("Red", "red");
        m.insert("RedOrange", "rgb(\"#F26035\")");
        m.insert("RedViolet", "rgb(\"#A1246B\")");
        m.insert("Rhodamine", "rgb(\"#EF559F\")");
        m.insert("RoyalBlue", "rgb(\"#0071BC\")");
        m.insert("RoyalPurple", "rgb(\"#613F99\")");
        m.insert("RubineRed", "rgb(\"#ED017D\")");
        m.insert("Salmon", "rgb(\"#F69289\")");
        m.insert("SeaGreen", "rgb(\"#3FBC9D\")");
        m.insert("Sepia", "rgb(\"#671800\")");
        m.insert("SkyBlue", "rgb(\"#46C5DD\")");
        m.insert("SpringGreen", "rgb(\"#C6DC67\")");
        m.insert("Tan", "rgb(\"#DA9D76\")");
        m.insert("TealBlue", "rgb(\"#00AEB3\")");
        m.insert("Thistle", "rgb(\"#D883B7\")");
        m.insert("Turquoise", "rgb(\"#00B4CE\")");
        m.insert("Violet", "rgb(\"#58429B\")");
        m.insert("VioletRed", "rgb(\"#EF58A0\")");
        m.insert("White", "white");
        m.insert("WildStrawberry", "rgb(\"#EE2967\")");
        m.insert("Yellow", "yellow");
        m.insert("YellowGreen", "rgb(\"#98CC70\")");
        m.insert("YellowOrange", "rgb(\"#FAA21A\")");

        m
    };

    /// Typst color names to LaTeX color names/expressions
    pub static ref TYPST_TO_LATEX_COLORS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();

        // Standard colors (1:1 mapping)
        m.insert("black", "black");
        m.insert("white", "white");
        m.insert("red", "red");
        m.insert("green", "green");
        m.insert("blue", "blue");
        m.insert("yellow", "yellow");
        m.insert("cyan", "cyan");
        m.insert("magenta", "magenta");
        m.insert("orange", "orange");
        m.insert("purple", "purple");
        m.insert("pink", "pink");
        m.insert("brown", "brown");
        m.insert("gray", "gray");
        m.insert("grey", "gray");
        m.insert("lime", "lime");
        m.insert("olive", "olive");
        m.insert("teal", "teal");

        // Complex mappings (Typst name -> LaTeX xcolor expression)
        m.insert("navy", "blue!50!black");
        m.insert("aqua", "cyan");
        m.insert("maroon", "red!50!black");
        m.insert("silver", "gray!50");
        m.insert("fuchsia", "magenta");

        m
    };

    /// Regex for \textcolor{color}{text}
    static ref TEXTCOLOR_RE: Regex = Regex::new(
        r"\\textcolor(?:\[([^\]]*)\])?\{([^}]*)\}\{([^}]*)\}"
    ).unwrap();

    /// Regex for \color{color}
    static ref COLOR_RE: Regex = Regex::new(
        r"\\color(?:\[([^\]]*)\])?\{([^}]*)\}"
    ).unwrap();

    /// Regex for \colorbox{color}{text}
    static ref COLORBOX_RE: Regex = Regex::new(
        r"\\colorbox(?:\[([^\]]*)\])?\{([^}]*)\}\{([^}]*)\}"
    ).unwrap();

    /// Regex for \fcolorbox{frame}{bg}{text}
    static ref FCOLORBOX_RE: Regex = Regex::new(
        r"\\fcolorbox(?:\[([^\]]*)\])?\{([^}]*)\}\{([^}]*)\}\{([^}]*)\}"
    ).unwrap();

    /// Regex for \highlight{text} (soul package)
    static ref HIGHLIGHT_RE: Regex = Regex::new(
        r"\\(?:hl|highlight)\{([^}]*)\}"
    ).unwrap();

    /// Regex for RGB color definition: {RGB}{r,g,b}
    static ref RGB_DEF_RE: Regex = Regex::new(
        r"\{RGB\}\{(\d+),\s*(\d+),\s*(\d+)\}"
    ).unwrap();

    /// Regex for rgb color definition: {rgb}{r,g,b}
    static ref RGB_SMALL_DEF_RE: Regex = Regex::new(
        r"\{rgb\}\{([0-9.]+),\s*([0-9.]+),\s*([0-9.]+)\}"
    ).unwrap();

    /// Regex for HTML color definition: {HTML}{RRGGBB}
    static ref HTML_DEF_RE: Regex = Regex::new(
        r"\{HTML\}\{([0-9A-Fa-f]{6})\}"
    ).unwrap();
}

/// Convert color commands in the input
pub fn convert_color_commands(input: &str) -> String {
    let mut result = input.to_string();

    // Process \textcolor{color}{text}
    result = TEXTCOLOR_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let model = caps.get(1).map(|m| m.as_str());
            let color = &caps[2];
            let text = &caps[3];
            let typst_color = parse_color(color, model);
            format!("#text(fill: {})[{}]", typst_color, text)
        })
        .to_string();

    // Process \colorbox{color}{text}
    result = COLORBOX_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let model = caps.get(1).map(|m| m.as_str());
            let color = &caps[2];
            let text = &caps[3];
            let typst_color = parse_color(color, model);
            format!("#box(fill: {}, inset: 2pt)[{}]", typst_color, text)
        })
        .to_string();

    // Process \fcolorbox{frame}{bg}{text}
    result = FCOLORBOX_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let model = caps.get(1).map(|m| m.as_str());
            let frame_color = &caps[2];
            let bg_color = &caps[3];
            let text = &caps[4];
            let typst_frame = parse_color(frame_color, model);
            let typst_bg = parse_color(bg_color, model);
            format!(
                "#box(fill: {}, stroke: {}, inset: 2pt)[{}]",
                typst_bg, typst_frame, text
            )
        })
        .to_string();

    // Process \highlight{text}
    result = HIGHLIGHT_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let text = &caps[1];
            format!("#highlight[{}]", text)
        })
        .to_string();

    // Remove standalone \color{} commands (they affect subsequent text)
    // In Typst, we'd need a different approach - for now, just remove them
    result = COLOR_RE.replace_all(&result, "").to_string();

    result
}

/// Parse a color specification and return Typst color
fn parse_color(color: &str, model: Option<&str>) -> String {
    let color = color.trim();

    // Check for named colors first
    if let Some(typst_color) = NAMED_COLORS.get(color) {
        return typst_color.to_string();
    }

    // Check color model
    match model {
        Some("RGB") => {
            // RGB{r,g,b} where r,g,b are 0-255
            let parts: Vec<&str> = color.split(',').collect();
            if parts.len() == 3 {
                let r: u8 = parts[0].trim().parse().unwrap_or(0);
                let g: u8 = parts[1].trim().parse().unwrap_or(0);
                let b: u8 = parts[2].trim().parse().unwrap_or(0);
                return format!("rgb({}, {}, {})", r, g, b);
            }
        }
        Some("rgb") => {
            // rgb{r,g,b} where r,g,b are 0.0-1.0
            let parts: Vec<&str> = color.split(',').collect();
            if parts.len() == 3 {
                let r: f32 = parts[0].trim().parse().unwrap_or(0.0);
                let g: f32 = parts[1].trim().parse().unwrap_or(0.0);
                let b: f32 = parts[2].trim().parse().unwrap_or(0.0);
                return format!(
                    "rgb({}%, {}%, {}%)",
                    (r * 100.0) as i32,
                    (g * 100.0) as i32,
                    (b * 100.0) as i32
                );
            }
        }
        Some("HTML") => {
            // HTML{RRGGBB}
            return format!("rgb(\"#{}\")", color);
        }
        Some("gray") => {
            // gray{value} where value is 0.0-1.0
            let v: f32 = color.parse().unwrap_or(0.5);
            return format!("luma({}%)", (v * 100.0) as i32);
        }
        Some("cmyk") => {
            // cmyk{c,m,y,k} - convert to RGB approximation
            let parts: Vec<&str> = color.split(',').collect();
            if parts.len() == 4 {
                let c: f32 = parts[0].trim().parse().unwrap_or(0.0);
                let m: f32 = parts[1].trim().parse().unwrap_or(0.0);
                let y: f32 = parts[2].trim().parse().unwrap_or(0.0);
                let k: f32 = parts[3].trim().parse().unwrap_or(0.0);
                // CMYK to RGB conversion
                let r = ((1.0 - c) * (1.0 - k) * 255.0) as u8;
                let g = ((1.0 - m) * (1.0 - k) * 255.0) as u8;
                let b = ((1.0 - y) * (1.0 - k) * 255.0) as u8;
                return format!("rgb({}, {}, {})", r, g, b);
            }
        }
        _ => {}
    }

    // Try to parse as hex color
    if color.starts_with('#') || color.chars().all(|c| c.is_ascii_hexdigit()) {
        let hex = color.trim_start_matches('#');
        if hex.len() == 6 {
            return format!("rgb(\"#{}\")", hex);
        } else if hex.len() == 3 {
            // Short form: expand ABC to AABBCC
            let expanded: String = hex.chars().flat_map(|c| [c, c]).collect();
            return format!("rgb(\"#{}\")", expanded);
        }
    }

    // Try named color variations
    let lower = color.to_lowercase();
    if let Some(typst_color) = NAMED_COLORS.get(lower.as_str()) {
        return typst_color.to_string();
    }

    // Fallback: return as-is (might be a Typst color)
    color.to_string()
}

/// Parse color with possible modifiers (e.g., "blue!50!white")
pub fn parse_color_expression(expr: &str) -> String {
    let expr = expr.trim();

    // Check for xcolor mixing syntax: color1!percent!color2
    if expr.contains('!') {
        let parts: Vec<&str> = expr.split('!').collect();
        if parts.len() >= 2 {
            let color1 = parts[0];
            let percent: f32 = parts[1].parse().unwrap_or(50.0);
            let color2 = if parts.len() > 2 { parts[2] } else { "white" };

            let c1 = parse_color(color1, None);
            let c2 = parse_color(color2, None);

            // Typst color mixing
            return format!(
                "color.mix(({}, {}%), ({}, {}%))",
                c1,
                percent as i32,
                c2,
                (100.0 - percent) as i32
            );
        }
    }

    parse_color(expr, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_textcolor() {
        let input = r"\textcolor{red}{important text}";
        let result = convert_color_commands(input);
        assert!(result.contains("#text(fill: red)"));
        assert!(result.contains("important text"));
    }

    #[test]
    fn test_colorbox() {
        let input = r"\colorbox{yellow}{highlighted}";
        let result = convert_color_commands(input);
        assert!(result.contains("#box(fill: yellow"));
        assert!(result.contains("highlighted"));
    }

    #[test]
    fn test_fcolorbox() {
        let input = r"\fcolorbox{red}{yellow}{framed box}";
        let result = convert_color_commands(input);
        assert!(result.contains("stroke: red"));
        assert!(result.contains("fill: yellow"));
    }

    #[test]
    fn test_named_color() {
        assert_eq!(parse_color("ForestGreen", None), "rgb(\"#009B55\")");
        assert_eq!(parse_color("red", None), "red");
    }

    #[test]
    fn test_rgb_color() {
        let result = parse_color("255,128,0", Some("RGB"));
        assert!(result.contains("rgb(255, 128, 0)"));
    }

    #[test]
    fn test_html_color() {
        let result = parse_color("FF5733", Some("HTML"));
        assert!(result.contains("#FF5733"));
    }

    #[test]
    fn test_highlight() {
        let input = r"\hl{important}";
        let result = convert_color_commands(input);
        assert!(result.contains("#highlight[important]"));
    }
}
