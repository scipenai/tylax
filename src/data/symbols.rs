//! Comprehensive LaTeX symbol and command mappings
//! Based on Pandoc's LaTeX reader with additional extensions
//! This module provides mappings superior to Pandoc's coverage

use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    /// Special character commands (miscCommands in Pandoc)
    pub static ref MISC_SYMBOLS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Currency and common symbols
        m.insert("\\pounds", "£");
        m.insert("\\euro", "€");
        m.insert("\\yen", "¥");
        m.insert("\\copyright", "©");
        m.insert("\\registered", "®");
        m.insert("\\trademark", "™");
        m.insert("\\textregistered", "®");
        m.insert("\\textcopyright", "©");
        m.insert("\\texttrademark", "™");

        // Text symbols
        m.insert("\\textasciicircum", "^");
        m.insert("\\textasciitilde", "~");
        m.insert("\\textbaht", "฿");
        m.insert("\\textblank", "␢");
        m.insert("\\textbigcircle", "○");
        m.insert("\\textbrokenbar", "¦");
        m.insert("\\textbullet", "•");
        m.insert("\\textcentoldstyle", "¢");
        m.insert("\\textcent", "¢");
        m.insert("\\textdagger", "†");
        m.insert("\\textdaggerdbl", "‡");
        m.insert("\\textdegree", "°");
        m.insert("\\textdollar", "\\$");  // Dollar sign needs escaping in Typst
        m.insert("\\textdong", "₫");
        m.insert("\\textlira", "₤");
        m.insert("\\textmu", "μ");
        m.insert("\\textmusicalnote", "♪");
        m.insert("\\textonehalf", "½");
        m.insert("\\textonequarter", "¼");
        m.insert("\\textthreequarters", "¾");
        m.insert("\\textparagraph", "¶");
        m.insert("\\textpertenthousand", "‱");
        m.insert("\\textperthousand", "‰");
        m.insert("\\textpeso", "₱");
        m.insert("\\textquotesingle", "'");
        m.insert("\\textsection", "§");
        m.insert("\\textsterling", "£");
        m.insert("\\textthreesuperior", "³");
        m.insert("\\texttwosuperior", "²");
        m.insert("\\textonesuperior", "¹");
        m.insert("\\textyen", "¥");
        m.insert("\\textordfeminine", "ª");
        m.insert("\\textordmasculine", "º");
        m.insert("\\texteuro", "€");
        m.insert("\\textellipsis", "…");
        m.insert("\\textendash", "–");
        m.insert("\\textemdash", "—");
        m.insert("\\textexclamdown", "¡");
        m.insert("\\textquestiondown", "¿");
        m.insert("\\textleftarrow", "←");
        m.insert("\\textrightarrow", "→");
        m.insert("\\textuparrow", "↑");
        m.insert("\\textdownarrow", "↓");

        // Quotes
        m.insert("\\textquoteleft", "\u{2018}");  // '
        m.insert("\\textquoteright", "\u{2019}"); // '
        m.insert("\\textquotedblleft", "\u{201C}");  // "
        m.insert("\\textquotedblright", "\u{201D}"); // "
        m.insert("\\guillemotleft", "\u{00AB}");  // «
        m.insert("\\guillemotright", "\u{00BB}"); // »
        m.insert("\\guilsinglleft", "\u{2039}");  // ‹
        m.insert("\\guilsinglright", "\u{203A}"); // ›
        m.insert("\\quotedblbase", "\u{201E}");   // „
        m.insert("\\quotesinglbase", "\u{201A}"); // ‚

        // Card suits
        m.insert("\\textspade", "♠");
        m.insert("\\textheart", "♥");
        m.insert("\\textdiamond", "♦");
        m.insert("\\textclub", "♣");

        // Miscellaneous
        m.insert("\\checkmark", "✓");
        m.insert("\\textcheckmark", "✓");
        m.insert("\\textcross", "✗");
        m.insert("\\textinterrobang", "‽");
        m.insert("\\textreferencemark", "※");
        m.insert("\\textdied", "†");
        m.insert("\\textborn", "∗");
        m.insert("\\textmarried", "⚭");
        m.insert("\\textdivorced", "⚮");

        m
    };

    /// Character commands (charCommands in Pandoc)
    pub static ref CHAR_COMMANDS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("\\ldots", "…");
        m.insert("\\vdots", "⋮");
        m.insert("\\cdots", "⋯");
        m.insert("\\ddots", "⋱");
        m.insert("\\dots", "…");
        m.insert("\\mdots", "…");
        m.insert("\\textellipsis", "…");
        m.insert("\\sim", "~");
        m.insert("\\sep", ",");
        m.insert("\\P", "¶");
        m.insert("\\S", "§");
        // Note: These characters need escaping in Typst
        // $ starts math mode, # starts code, _ is emphasis in text
        m.insert("\\$", "\\$");  // Escaped dollar sign
        m.insert("\\%", "%");     // % is safe in Typst (not a comment start like in TeX)
        m.insert("\\&", "&");     // & is used for alignment, often safe in text
        m.insert("\\#", "\\#");  // Escaped hash (# starts code mode)
        m.insert("\\_", "\\_");  // Escaped underscore
        m.insert("\\{", "{");
        m.insert("\\}", "}");
        m.insert("\\-", "\u{00ad}"); // soft hyphen
        m.insert("\\qed", "∎");
        m.insert("\\lq", "'");
        m.insert("\\rq", "'");
        m.insert("\\/", ""); // italic correction
        m.insert("\\,", "\u{2006}"); // thin space
        m.insert("\\;", "\u{2009}"); // medium space
        m.insert("\\:", "\u{2005}"); // medium space
        m.insert("\\!", ""); // negative thin space
        m.insert("\\@", "");
        m.insert("\\ ", "\u{00a0}"); // non-breaking space
        m.insert("\\~", "\u{00a0}"); // non-breaking space
        m.insert("\\ps", "PS.");
        m.insert("\\TeX", "TeX");
        m.insert("\\LaTeX", "LaTeX");
        m.insert("\\LaTeXe", "LaTeX2ε");
        m.insert("\\XeTeX", "XeTeX");
        m.insert("\\XeLaTeX", "XeLaTeX");
        m.insert("\\LuaTeX", "LuaTeX");
        m.insert("\\LuaLaTeX", "LuaLaTeX");
        m.insert("\\pdfTeX", "pdfTeX");
        m.insert("\\pdfLaTeX", "pdfLaTeX");
        m.insert("\\BibTeX", "BibTeX");
        m.insert("\\bar", "|");
        m.insert("\\textless", "<");
        m.insert("\\textgreater", ">");
        m.insert("\\textbackslash", "\\");
        m.insert("\\backslash", "\\");
        m.insert("\\slash", "/");
        m.insert("\\textbar", "|");
        m.insert("\\textbraceleft", "{");
        m.insert("\\textbraceright", "}");
        m.insert("\\textunderscore", "_");
        m.insert("\\textvisiblespace", "␣");

        // FontAwesome (common ones)
        m.insert("\\faCheck", "✓");
        m.insert("\\faClose", "✗");
        m.insert("\\faTimes", "✗");
        m.insert("\\faPlus", "+");
        m.insert("\\faMinus", "−");
        m.insert("\\faSearch", "🔍");
        m.insert("\\faHome", "🏠");
        m.insert("\\faUser", "👤");
        m.insert("\\faEnvelope", "✉");
        m.insert("\\faPhone", "📞");
        m.insert("\\faStar", "★");
        m.insert("\\faHeart", "♥");
        m.insert("\\faThumbsUp", "👍");
        m.insert("\\faThumbsDown", "👎");
        m.insert("\\faWarning", "⚠");
        m.insert("\\faInfo", "ℹ");
        m.insert("\\faQuestion", "?");
        m.insert("\\faExclamation", "!");
        m.insert("\\faArrowRight", "→");
        m.insert("\\faArrowLeft", "←");
        m.insert("\\faArrowUp", "↑");
        m.insert("\\faArrowDown", "↓");

        // hyphenat package
        m.insert("\\bshyp", "\\\u{00ad}");
        m.insert("\\fshyp", "/\u{00ad}");
        m.insert("\\dothyp", ".\u{00ad}");
        m.insert("\\colonhyp", ":\u{00ad}");
        m.insert("\\hyp", "-");

        m
    };

    /// Accent commands mapping - the combining character approach
    pub static ref ACCENT_COMMANDS: HashMap<&'static str, char> = {
        let mut m = HashMap::new();
        m.insert("\\`", '\u{0300}'); // grave
        m.insert("\\'", '\u{0301}'); // acute
        m.insert("\\^", '\u{0302}'); // circumflex
        m.insert("\\~", '\u{0303}'); // tilde
        m.insert("\\\"", '\u{0308}'); // umlaut/diaeresis
        m.insert("\\=", '\u{0304}'); // macron
        m.insert("\\.", '\u{0307}'); // dot above
        m.insert("\\u", '\u{0306}'); // breve
        m.insert("\\v", '\u{030C}'); // caron/hacek
        m.insert("\\H", '\u{030B}'); // double acute (hungarumlaut)
        m.insert("\\c", '\u{0327}'); // cedilla
        m.insert("\\k", '\u{0328}'); // ogonek
        m.insert("\\d", '\u{0323}'); // dot below
        m.insert("\\b", '\u{0331}'); // macron below
        m.insert("\\t", '\u{0361}'); // tie (double inverted breve)
        m.insert("\\r", '\u{030A}'); // ring above
        m.insert("\\h", '\u{0309}'); // hook above
        m.insert("\\G", '\u{030F}'); // double grave
        m.insert("\\f", '\u{0311}'); // inverted breve
        m.insert("\\U", '\u{030E}'); // double vertical line above
        m.insert("\\textogonekcentered", '\u{0328}');
        m
    };

    /// Special letter commands (no arguments)
    pub static ref LETTER_COMMANDS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("\\aa", "å");
        m.insert("\\AA", "Å");
        m.insert("\\ae", "æ");
        m.insert("\\AE", "Æ");
        m.insert("\\oe", "œ");
        m.insert("\\OE", "Œ");
        m.insert("\\o", "ø");
        m.insert("\\O", "Ø");
        m.insert("\\l", "ł");
        m.insert("\\L", "Ł");
        m.insert("\\ss", "ß");
        m.insert("\\SS", "ẞ");
        m.insert("\\i", "ı"); // dotless i
        m.insert("\\j", "ȷ"); // dotless j
        m.insert("\\dh", "ð");
        m.insert("\\DH", "Ð");
        m.insert("\\th", "þ");
        m.insert("\\TH", "Þ");
        m.insert("\\dj", "đ");
        m.insert("\\DJ", "Đ");
        m.insert("\\ng", "ŋ");
        m.insert("\\NG", "Ŋ");
        m.insert("\\ij", "ĳ");
        m.insert("\\IJ", "Ĳ");
        m
    };

    /// Biblatex inline commands mapping to Typst
    pub static ref BIBLATEX_COMMANDS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // These map to Typst equivalents or plain text
        m.insert("\\mkbibquote", "\""); // Will wrap content in quotes
        m.insert("\\mkbibemph", "_");   // Will wrap in emphasis
        m.insert("\\mkbibitalic", "_"); // Will wrap in emphasis
        m.insert("\\mkbibbold", "*");   // Will wrap in bold
        m.insert("\\autocap", "");      // No-op in Typst
        m.insert("\\textnormal", "");   // No-op in Typst
        m.insert("\\adddot", ".");
        m.insert("\\adddotspace", ". ");
        m.insert("\\addabbrvspace", " ");
        m.insert("\\addcomma", ",");
        m.insert("\\addcolon", ":");
        m.insert("\\addsemicolon", ";");
        m.insert("\\addperiod", ".");
        m.insert("\\addspace", " ");
        m.insert("\\hyphen", "-");
        m.insert("\\textendash", "–");
        m.insert("\\textemdash", "—");
        m
    };

    /// Name/term commands (translations)
    pub static ref NAME_COMMANDS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("\\figurename", "Figure");
        m.insert("\\tablename", "Table");
        m.insert("\\prefacename", "Preface");
        m.insert("\\refname", "References");
        m.insert("\\bibname", "Bibliography");
        m.insert("\\chaptername", "Chapter");
        m.insert("\\partname", "Part");
        m.insert("\\contentsname", "Contents");
        m.insert("\\listfigurename", "List of Figures");
        m.insert("\\listtablename", "List of Tables");
        m.insert("\\indexname", "Index");
        m.insert("\\abstractname", "Abstract");
        m.insert("\\enclname", "Enclosure");
        m.insert("\\ccname", "CC");
        m.insert("\\headtoname", "To");
        m.insert("\\pagename", "Page");
        m.insert("\\seename", "see");
        m.insert("\\seealsoname", "see also");
        m.insert("\\proofname", "Proof");
        m.insert("\\glossaryname", "Glossary");
        m.insert("\\lstlistingname", "Listing");
        m.insert("\\appendixname", "Appendix");
        m.insert("\\acknowledgementname", "Acknowledgement");
        m.insert("\\algorithname", "Algorithm");
        m.insert("\\assumptionname", "Assumption");
        m.insert("\\axiomname", "Axiom");
        m.insert("\\casename", "Case");
        m.insert("\\claimname", "Claim");
        m.insert("\\conclusionname", "Conclusion");
        m.insert("\\conditionname", "Condition");
        m.insert("\\conjecturename", "Conjecture");
        m.insert("\\corollaryname", "Corollary");
        m.insert("\\criterionname", "Criterion");
        m.insert("\\definitionname", "Definition");
        m.insert("\\examplename", "Example");
        m.insert("\\exercisename", "Exercise");
        m.insert("\\hypothesisname", "Hypothesis");
        m.insert("\\lemmaname", "Lemma");
        m.insert("\\notationname", "Notation");
        m.insert("\\problemname", "Problem");
        m.insert("\\propertyname", "Property");
        m.insert("\\propositionname", "Proposition");
        m.insert("\\questionname", "Question");
        m.insert("\\remarkname", "Remark");
        m.insert("\\solutionname", "Solution");
        m.insert("\\summaryname", "Summary");
        m.insert("\\theoremname", "Theorem");
        m
    };

    /// Greek letters (both text mode and math mode)
    pub static ref GREEK_LETTERS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Lowercase
        m.insert("\\alpha", "α");
        m.insert("\\beta", "β");
        m.insert("\\gamma", "γ");
        m.insert("\\delta", "δ");
        m.insert("\\epsilon", "ε");
        m.insert("\\varepsilon", "ε");
        m.insert("\\zeta", "ζ");
        m.insert("\\eta", "η");
        m.insert("\\theta", "θ");
        m.insert("\\vartheta", "ϑ");
        m.insert("\\iota", "ι");
        m.insert("\\kappa", "κ");
        m.insert("\\lambda", "λ");
        m.insert("\\mu", "μ");
        m.insert("\\nu", "ν");
        m.insert("\\xi", "ξ");
        m.insert("\\pi", "π");
        m.insert("\\varpi", "ϖ");
        m.insert("\\rho", "ρ");
        m.insert("\\varrho", "ϱ");
        m.insert("\\sigma", "σ");
        m.insert("\\varsigma", "ς");
        m.insert("\\tau", "τ");
        m.insert("\\upsilon", "υ");
        m.insert("\\phi", "φ");
        m.insert("\\varphi", "φ");
        m.insert("\\chi", "χ");
        m.insert("\\psi", "ψ");
        m.insert("\\omega", "ω");
        // Uppercase
        m.insert("\\Alpha", "Α");
        m.insert("\\Beta", "Β");
        m.insert("\\Gamma", "Γ");
        m.insert("\\Delta", "Δ");
        m.insert("\\Epsilon", "Ε");
        m.insert("\\Zeta", "Ζ");
        m.insert("\\Eta", "Η");
        m.insert("\\Theta", "Θ");
        m.insert("\\Iota", "Ι");
        m.insert("\\Kappa", "Κ");
        m.insert("\\Lambda", "Λ");
        m.insert("\\Mu", "Μ");
        m.insert("\\Nu", "Ν");
        m.insert("\\Xi", "Ξ");
        m.insert("\\Pi", "Π");
        m.insert("\\Rho", "Ρ");
        m.insert("\\Sigma", "Σ");
        m.insert("\\Tau", "Τ");
        m.insert("\\Upsilon", "Υ");
        m.insert("\\Phi", "Φ");
        m.insert("\\Chi", "Χ");
        m.insert("\\Psi", "Ψ");
        m.insert("\\Omega", "Ω");
        m
    };

    /// Text formatting commands that take an argument
    pub static ref TEXT_FORMAT_COMMANDS: HashMap<&'static str, (&'static str, &'static str)> = {
        let mut m = HashMap::new();
        // (prefix, suffix) for Typst
        m.insert("\\textbf", ("*", "*"));
        m.insert("\\textit", ("_", "_"));
        m.insert("\\emph", ("_", "_"));
        m.insert("\\texttt", ("`", "`"));
        m.insert("\\textsc", ("#smallcaps[", "]"));
        m.insert("\\textsf", ("", "")); // sans-serif - no direct Typst equiv
        m.insert("\\textrm", ("", "")); // roman - no direct Typst equiv
        m.insert("\\textup", ("", "")); // upright
        m.insert("\\textsl", ("_", "_")); // slanted ≈ italic
        m.insert("\\underline", ("#underline[", "]"));
        m.insert("\\uline", ("#underline[", "]")); // ulem package
        m.insert("\\sout", ("#strike[", "]")); // strikeout
        m.insert("\\st", ("#strike[", "]")); // soul package
        m.insert("\\hl", ("#highlight[", "]")); // highlight
        m.insert("\\textsubscript", ("#sub[", "]"));
        m.insert("\\textsuperscript", ("#super[", "]"));
        m.insert("\\enquote", ("\"", "\"")); // csquotes
        m
    };
}

/// Replace LaTeX command only if followed by non-letter (word boundary)
fn replace_command_safe(input: &str, cmd: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut remaining = input;

    while let Some(pos) = remaining.find(cmd) {
        // Add everything before the match
        result.push_str(&remaining[..pos]);

        // Check if this is a complete command (not followed by a letter)
        let after_cmd = &remaining[pos + cmd.len()..];
        let next_char = after_cmd.chars().next();

        if next_char.map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
            // Followed by a letter - not a complete command, keep original
            result.push_str(cmd);
        } else {
            // Complete command - replace it
            result.push_str(replacement);
        }

        remaining = after_cmd;
    }

    result.push_str(remaining);
    result
}

/// Apply simple symbol replacements
pub fn apply_symbol_replacements(input: &str) -> String {
    let mut result = input.to_string();

    // Apply all symbol maps
    for (latex, typst) in MISC_SYMBOLS.iter() {
        result = result.replace(latex, typst);
    }
    for (latex, typst) in CHAR_COMMANDS.iter() {
        result = result.replace(latex, typst);
    }
    // Use safe replacement for LETTER_COMMANDS to avoid \th matching \theta
    for (latex, typst) in LETTER_COMMANDS.iter() {
        result = replace_command_safe(&result, latex, typst);
    }
    for (latex, typst) in BIBLATEX_COMMANDS.iter() {
        result = result.replace(latex, typst);
    }
    for (latex, typst) in NAME_COMMANDS.iter() {
        result = result.replace(latex, typst);
    }
    for (latex, typst) in GREEK_LETTERS.iter() {
        result = result.replace(latex, typst);
    }

    result
}

/// Apply accent to a character
pub fn apply_accent(base: char, accent_cmd: &str) -> Option<String> {
    if let Some(&combining) = ACCENT_COMMANDS.get(accent_cmd) {
        let mut result = String::new();
        result.push(base);
        result.push(combining);
        // Try to normalize to composed form
        Some(unicode_normalization_nfc(&result))
    } else {
        None
    }
}

/// Simple NFC normalization (compose combining characters)
fn unicode_normalization_nfc(s: &str) -> String {
    // For common cases, we can handle them directly
    // This is a simplified version - a full implementation would use the unicode-normalization crate
    let common_compositions: HashMap<&str, &str> = [
        ("à", "à"),
        ("á", "á"),
        ("â", "â"),
        ("ã", "ã"),
        ("ä", "ä"),
        ("å", "å"),
        ("è", "è"),
        ("é", "é"),
        ("ê", "ê"),
        ("ë", "ë"),
        ("ì", "ì"),
        ("í", "í"),
        ("î", "î"),
        ("ï", "ï"),
        ("ò", "ò"),
        ("ó", "ó"),
        ("ô", "ô"),
        ("õ", "õ"),
        ("ö", "ö"),
        ("ù", "ù"),
        ("ú", "ú"),
        ("û", "û"),
        ("ü", "ü"),
        ("ñ", "ñ"),
        ("ç", "ç"),
        ("À", "À"),
        ("Á", "Á"),
        ("Â", "Â"),
        ("Ã", "Ã"),
        ("Ä", "Ä"),
        ("Å", "Å"),
        ("È", "È"),
        ("É", "É"),
        ("Ê", "Ê"),
        ("Ë", "Ë"),
        ("Ì", "Ì"),
        ("Í", "Í"),
        ("Î", "Î"),
        ("Ï", "Ï"),
        ("Ò", "Ò"),
        ("Ó", "Ó"),
        ("Ô", "Ô"),
        ("Õ", "Õ"),
        ("Ö", "Ö"),
        ("Ù", "Ù"),
        ("Ú", "Ú"),
        ("Û", "Û"),
        ("Ü", "Ü"),
        ("Ñ", "Ñ"),
        ("Ç", "Ç"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut result = s.to_string();
    for (decomposed, composed) in common_compositions.iter() {
        result = result.replace(decomposed, composed);
    }
    result
}

/// Convert text formatting command with its argument
pub fn convert_text_format(cmd: &str, content: &str) -> Option<String> {
    if let Some((prefix, suffix)) = TEXT_FORMAT_COMMANDS.get(cmd) {
        Some(format!("{}{}{}", prefix, content, suffix))
    } else {
        None
    }
}

/// Process accented characters like \'e -> é, \"o -> ö
pub fn process_accent_commands(input: &str) -> String {
    let mut result = input.to_string();

    // Common pre-composed accented characters
    let accent_map: &[(&str, &str)] = &[
        // Acute accent
        ("\\'a", "á"),
        ("\\'e", "é"),
        ("\\'i", "í"),
        ("\\'o", "ó"),
        ("\\'u", "ú"),
        ("\\'A", "Á"),
        ("\\'E", "É"),
        ("\\'I", "Í"),
        ("\\'O", "Ó"),
        ("\\'U", "Ú"),
        ("\\'y", "ý"),
        ("\\'Y", "Ý"),
        ("\\'c", "ć"),
        ("\\'C", "Ć"),
        ("\\'n", "ń"),
        ("\\'N", "Ń"),
        ("\\'s", "ś"),
        ("\\'S", "Ś"),
        ("\\'z", "ź"),
        ("\\'Z", "Ź"),
        ("\\'r", "ŕ"),
        ("\\'R", "Ŕ"),
        ("\\'l", "ĺ"),
        ("\\'L", "Ĺ"),
        // Also handle with braces
        ("\\'{a}", "á"),
        ("\\'{e}", "é"),
        ("\\'{i}", "í"),
        ("\\'{o}", "ó"),
        ("\\'{u}", "ú"),
        ("\\'{A}", "Á"),
        ("\\'{E}", "É"),
        ("\\'{I}", "Í"),
        ("\\'{O}", "Ó"),
        ("\\'{U}", "Ú"),
        // Grave accent
        ("\\`a", "à"),
        ("\\`e", "è"),
        ("\\`i", "ì"),
        ("\\`o", "ò"),
        ("\\`u", "ù"),
        ("\\`A", "À"),
        ("\\`E", "È"),
        ("\\`I", "Ì"),
        ("\\`O", "Ò"),
        ("\\`U", "Ù"),
        ("\\`{a}", "à"),
        ("\\`{e}", "è"),
        ("\\`{i}", "ì"),
        ("\\`{o}", "ò"),
        ("\\`{u}", "ù"),
        // Circumflex
        ("\\^a", "â"),
        ("\\^e", "ê"),
        ("\\^i", "î"),
        ("\\^o", "ô"),
        ("\\^u", "û"),
        ("\\^A", "Â"),
        ("\\^E", "Ê"),
        ("\\^I", "Î"),
        ("\\^O", "Ô"),
        ("\\^U", "Û"),
        ("\\^{a}", "â"),
        ("\\^{e}", "ê"),
        ("\\^{i}", "î"),
        ("\\^{o}", "ô"),
        ("\\^{u}", "û"),
        // Tilde
        ("\\~a", "ã"),
        ("\\~o", "õ"),
        ("\\~n", "ñ"),
        ("\\~A", "Ã"),
        ("\\~O", "Õ"),
        ("\\~N", "Ñ"),
        ("\\~{a}", "ã"),
        ("\\~{o}", "õ"),
        ("\\~{n}", "ñ"),
        // Umlaut/Diaeresis
        ("\\\"a", "ä"),
        ("\\\"e", "ë"),
        ("\\\"i", "ï"),
        ("\\\"o", "ö"),
        ("\\\"u", "ü"),
        ("\\\"A", "Ä"),
        ("\\\"E", "Ë"),
        ("\\\"I", "Ï"),
        ("\\\"O", "Ö"),
        ("\\\"U", "Ü"),
        ("\\\"y", "ÿ"),
        ("\\\"Y", "Ÿ"),
        ("\\\"{a}", "ä"),
        ("\\\"{e}", "ë"),
        ("\\\"{i}", "ï"),
        ("\\\"{o}", "ö"),
        ("\\\"{u}", "ü"),
        // Cedilla
        ("\\c{c}", "ç"),
        ("\\c{C}", "Ç"),
        ("\\c c", "ç"),
        ("\\c C", "Ç"),
        ("\\c{s}", "ş"),
        ("\\c{S}", "Ş"),
        // Caron/Hacek
        ("\\v{c}", "č"),
        ("\\v{C}", "Č"),
        ("\\v{s}", "š"),
        ("\\v{S}", "Š"),
        ("\\v{z}", "ž"),
        ("\\v{Z}", "Ž"),
        ("\\v{e}", "ě"),
        ("\\v{E}", "Ě"),
        ("\\v{r}", "ř"),
        ("\\v{R}", "Ř"),
        ("\\v{n}", "ň"),
        ("\\v{N}", "Ň"),
        ("\\v{d}", "ď"),
        ("\\v{D}", "Ď"),
        ("\\v{t}", "ť"),
        ("\\v{T}", "Ť"),
        // Macron
        ("\\=a", "ā"),
        ("\\=e", "ē"),
        ("\\=i", "ī"),
        ("\\=o", "ō"),
        ("\\=u", "ū"),
        ("\\=A", "Ā"),
        ("\\=E", "Ē"),
        ("\\=I", "Ī"),
        ("\\=O", "Ō"),
        ("\\=U", "Ū"),
        ("\\={a}", "ā"),
        ("\\={e}", "ē"),
        ("\\={i}", "ī"),
        ("\\={o}", "ō"),
        ("\\={u}", "ū"),
        // Breve
        ("\\u{a}", "ă"),
        ("\\u{A}", "Ă"),
        ("\\u{g}", "ğ"),
        ("\\u{G}", "Ğ"),
        ("\\u{i}", "ĭ"),
        ("\\u{I}", "Ĭ"),
        // Dot above
        ("\\.{z}", "ż"),
        ("\\.{Z}", "Ż"),
        ("\\.{e}", "ė"),
        ("\\.{E}", "Ė"),
        ("\\.{c}", "ċ"),
        ("\\.{C}", "Ċ"),
        ("\\.{g}", "ġ"),
        ("\\.{G}", "Ġ"),
        ("\\.{I}", "İ"),
        // Ring above
        ("\\r{a}", "å"),
        ("\\r{A}", "Å"),
        ("\\r{u}", "ů"),
        ("\\r{U}", "Ů"),
        // Ogonek
        ("\\k{a}", "ą"),
        ("\\k{A}", "Ą"),
        ("\\k{e}", "ę"),
        ("\\k{E}", "Ę"),
        // Double acute (Hungarian umlaut)
        ("\\H{o}", "ő"),
        ("\\H{O}", "Ő"),
        ("\\H{u}", "ű"),
        ("\\H{U}", "Ű"),
    ];

    for (latex, unicode) in accent_map {
        result = result.replace(latex, unicode);
    }

    result
}

// ============================================================================
// Big Delimiter Commands
// ============================================================================

lazy_static! {
    /// Commands that modify delimiter size in LaTeX
    /// These should just pass through the delimiter to Typst
    pub static ref BIG_DELIMITER_COMMANDS: std::collections::HashSet<&'static str> = {
        let mut s = std::collections::HashSet::new();
        // \big variants
        s.insert("big");
        s.insert("Big");
        s.insert("bigg");
        s.insert("Bigg");
        // \bigl/\bigr variants (left/right)
        s.insert("bigl");
        s.insert("bigr");
        s.insert("Bigl");
        s.insert("Bigr");
        s.insert("biggl");
        s.insert("biggr");
        s.insert("Biggl");
        s.insert("Biggr");
        // \bigm variants (middle)
        s.insert("bigm");
        s.insert("Bigm");
        s.insert("biggm");
        s.insert("Biggm");
        s
    };

    /// Mapping from LaTeX delimiters to Typst equivalents
    pub static ref DELIMITER_TO_TYPST: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Basic brackets
        m.insert("[", "[");
        m.insert("]", "]");
        m.insert("(", "(");
        m.insert(")", ")");
        m.insert("\\lbrack", "[");
        m.insert("\\rbrack", "]");
        m.insert("\\lparen", "(");
        m.insert("\\rparen", ")");
        // Braces
        m.insert("\\{", "{");
        m.insert("\\}", "}");
        m.insert("\\lbrace", "{");
        m.insert("\\rbrace", "}");
        // Vertical bars
        m.insert("|", "bar.v");
        m.insert("\\vert", "bar.v");
        m.insert("\\lvert", "bar.v");
        m.insert("\\rvert", "bar.v");
        m.insert("\\|", "bar.v.double");
        m.insert("\\Vert", "bar.v.double");
        m.insert("\\lVert", "bar.v.double");
        m.insert("\\rVert", "bar.v.double");
        // Angle brackets
        m.insert("\\langle", "angle.l");
        m.insert("\\rangle", "angle.r");
        // Floor and ceiling
        m.insert("\\lfloor", "floor.l");
        m.insert("\\rfloor", "floor.r");
        m.insert("\\lceil", "ceil.l");
        m.insert("\\rceil", "ceil.r");
        // Invisible delimiter
        m.insert(".", "");
        m
    };
}

/// Check if a command is a big delimiter sizing command
pub fn is_big_delimiter_command(cmd: &str) -> bool {
    BIG_DELIMITER_COMMANDS.contains(cmd)
}

/// Convert a LaTeX delimiter to Typst equivalent
pub fn convert_delimiter(delim: &str) -> Option<&'static str> {
    DELIMITER_TO_TYPST.get(delim).copied()
}

// ============================================================================
// Caption/Title Text Formatting Commands
// ============================================================================

lazy_static! {
    /// Text formatting commands that take a braced argument
    /// Used for converting formatting in captions, titles, etc.
    pub static ref CAPTION_TEXT_COMMANDS: std::collections::HashSet<&'static str> = {
        let mut s = std::collections::HashSet::new();
        s.insert("textbf");
        s.insert("textit");
        s.insert("texttt");
        s.insert("textrm");
        s.insert("textsc");
        s.insert("textsf");
        s.insert("emph");
        s.insert("underline");
        s.insert("text");
        s.insert("mbox");
        s.insert("hbox");
        s
    };
}

/// Check if a command is a caption text formatting command
pub fn is_caption_text_command(cmd: &str) -> bool {
    CAPTION_TEXT_COMMANDS.contains(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_replacements() {
        assert!(apply_symbol_replacements("\\pounds").contains("£"));
        assert!(apply_symbol_replacements("\\euro").contains("€"));
        assert!(apply_symbol_replacements("\\ldots").contains("…"));
    }

    #[test]
    fn test_accent_processing() {
        assert_eq!(process_accent_commands("\\'e"), "é");
        assert_eq!(process_accent_commands("\\\"o"), "ö");
        assert_eq!(process_accent_commands("\\~n"), "ñ");
        assert_eq!(process_accent_commands("\\c{c}"), "ç");
    }

    #[test]
    fn test_text_format() {
        assert_eq!(
            convert_text_format("\\textbf", "bold"),
            Some("*bold*".to_string())
        );
        assert_eq!(
            convert_text_format("\\emph", "italic"),
            Some("_italic_".to_string())
        );
        assert_eq!(
            convert_text_format("\\texttt", "code"),
            Some("`code`".to_string())
        );
    }

    #[test]
    fn test_greek_letters() {
        let result = apply_symbol_replacements("\\alpha \\beta \\gamma");
        assert!(result.contains("α"));
        assert!(result.contains("β"));
        assert!(result.contains("γ"));
    }
}
