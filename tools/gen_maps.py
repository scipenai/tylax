#!/usr/bin/env python3
"""
Generate Rust symbol mapping code for Tylax

This script generates the maps.rs file containing symbol mappings.
The mappings are based on tex2typst project but are now embedded directly.

Note: This script is only needed when you want to update the symbol mappings.
For normal usage, the pre-generated maps.rs is sufficient.

Usage:
    python gen_maps.py                    # Use embedded mappings
    python gen_maps.py path/to/map.ts     # Update from tex2typst source
"""

import re
import sys
from pathlib import Path

# ============================================================================
# EMBEDDED SYMBOL MAPPINGS
# These are extracted from tex2typst project for independence
# ============================================================================

SYMBOL_MAP = {
    # Greek lowercase
    "alpha": "alpha", "beta": "beta", "gamma": "gamma", "delta": "delta",
    "epsilon": "epsilon", "varepsilon": "epsilon.alt", "zeta": "zeta",
    "eta": "eta", "theta": "theta", "vartheta": "theta.alt",
    "iota": "iota", "kappa": "kappa", "lambda": "lambda", "mu": "mu",
    "nu": "nu", "xi": "xi", "pi": "pi", "varpi": "pi.alt",
    "rho": "rho", "varrho": "rho.alt", "sigma": "sigma", "varsigma": "sigma.alt",
    "tau": "tau", "upsilon": "upsilon", "phi": "phi.alt", "varphi": "phi",
    "chi": "chi", "psi": "psi", "omega": "omega",
    
    # Greek uppercase
    "Gamma": "Gamma", "Delta": "Delta", "Theta": "Theta", "Lambda": "Lambda",
    "Xi": "Xi", "Pi": "Pi", "Sigma": "Sigma", "Upsilon": "Upsilon",
    "Phi": "Phi", "Psi": "Psi", "Omega": "Omega",
    
    # Binary operators
    "pm": "plus.minus", "mp": "minus.plus", "times": "times", "div": "div",
    "cdot": "dot.op", "ast": "ast", "star": "star", "circ": "circle.small",
    "bullet": "bullet", "oplus": "plus.circle", "ominus": "minus.circle",
    "otimes": "times.circle", "oslash": "divides.circle", "odot": "dot.circle",
    "cap": "sect", "cup": "union", "sqcap": "sect.sq", "sqcup": "union.sq",
    "vee": "or", "wedge": "and", "setminus": "without",
    "wr": "wreath", "diamond": "diamond", "bigtriangleup": "triangle.t",
    "bigtriangledown": "triangle.b", "triangleleft": "triangle.l",
    "triangleright": "triangle.r", "lhd": "triangle.l", "rhd": "triangle.r",
    "unlhd": "triangle.l.eq", "unrhd": "triangle.r.eq",
    "amalg": "product.co", "dagger": "dagger", "ddagger": "dagger.double",
    
    # Relations
    "leq": "lt.eq", "le": "lt.eq", "geq": "gt.eq", "ge": "gt.eq",
    "prec": "prec", "succ": "succ", "preceq": "prec.eq", "succeq": "succ.eq",
    "ll": "lt.double", "gg": "gt.double", "subset": "subset", "supset": "supset",
    "subseteq": "subset.eq", "supseteq": "supset.eq", "sqsubset": "subset.sq",
    "sqsupset": "supset.sq", "sqsubseteq": "subset.sq.eq", "sqsupseteq": "supset.sq.eq",
    "in": "in", "ni": "in.rev", "notin": "in.not", "vdash": "tack.r",
    "dashv": "tack.l", "models": "models", "smile": "smile", "frown": "frown",
    "mid": "divides", "parallel": "parallel", "perp": "perp",
    "equiv": "equiv", "sim": "tilde.op", "simeq": "tilde.eq", "asymp": "asymp",
    "approx": "approx", "cong": "tilde.equiv", "neq": "eq.not", "ne": "eq.not",
    "doteq": "eq.dot", "propto": "prop",
    
    # Arrows
    "leftarrow": "arrow.l", "rightarrow": "arrow.r", "to": "arrow.r",
    "leftrightarrow": "arrow.l.r", "Leftarrow": "arrow.l.double",
    "Rightarrow": "arrow.r.double", "Leftrightarrow": "arrow.l.r.double",
    "mapsto": "arrow.r.bar", "hookleftarrow": "arrow.l.hook",
    "hookrightarrow": "arrow.r.hook", "leftharpoonup": "harpoon.lt",
    "leftharpoondown": "harpoon.lb", "rightharpoonup": "harpoon.rt",
    "rightharpoondown": "harpoon.rb", "uparrow": "arrow.t",
    "downarrow": "arrow.b", "updownarrow": "arrow.t.b",
    "Uparrow": "arrow.t.double", "Downarrow": "arrow.b.double",
    "Updownarrow": "arrow.t.b.double", "nearrow": "arrow.tr",
    "searrow": "arrow.br", "swarrow": "arrow.bl", "nwarrow": "arrow.tl",
    "leadsto": "arrow.r.squiggly", "longleftarrow": "arrow.l.long",
    "longrightarrow": "arrow.r.long", "longleftrightarrow": "arrow.l.r.long",
    "Longleftarrow": "arrow.l.double.long", "Longrightarrow": "arrow.r.double.long",
    "Longleftrightarrow": "arrow.l.r.double.long", "longmapsto": "arrow.r.long.bar",
    "iff": "arrow.l.r.double.long",
    
    # Misc symbols
    "infty": "infinity", "forall": "forall", "exists": "exists",
    "nexists": "exists.not", "neg": "not", "lnot": "not",
    "emptyset": "emptyset", "varnothing": "nothing",
    "nabla": "nabla", "partial": "diff", "surd": "sqrt",
    "top": "top", "bot": "bot", "angle": "angle",
    "triangle": "triangle.t", "backslash": "backslash",
    "prime": "prime", "flat": "flat", "natural": "natural",
    "sharp": "sharp", "ell": "ell", "hbar": "planck.reduce",
    "imath": "dotless.i", "jmath": "dotless.j",
    "wp": "weierstrass", "Re": "Re", "Im": "Im",
    "aleph": "aleph", "beth": "beth", "gimel": "gimel",
    
    # Dots
    "ldots": "dots.h", "cdots": "dots.c", "vdots": "dots.v", "ddots": "dots.down",
    "dots": "dots", "dotsc": "dots.c", "dotsb": "dots.c", "dotsm": "dots.c",
    
    # Delimiters
    "langle": "angle.l", "rangle": "angle.r",
    "lceil": "ceil.l", "rceil": "ceil.r",
    "lfloor": "floor.l", "rfloor": "floor.r",
    "lbrace": "brace.l", "rbrace": "brace.r",
    "lvert": "bar.v", "rvert": "bar.v",
    "lVert": "bar.v.double", "rVert": "bar.v.double",
    
    # Big operators
    "sum": "sum", "prod": "product", "coprod": "product.co",
    "int": "integral", "iint": "integral.double", "iiint": "integral.triple",
    "oint": "integral.cont", "bigcap": "sect.big", "bigcup": "union.big",
    "bigsqcup": "union.sq.big", "bigvee": "or.big", "bigwedge": "and.big",
    "bigoplus": "plus.circle.big", "bigotimes": "times.circle.big",
    "bigodot": "dot.circle.big",
    
    # Functions
    "sin": "sin", "cos": "cos", "tan": "tan", "cot": "cot",
    "sec": "sec", "csc": "csc", "arcsin": "arcsin", "arccos": "arccos",
    "arctan": "arctan", "sinh": "sinh", "cosh": "cosh", "tanh": "tanh",
    "coth": "coth", "log": "log", "ln": "ln", "lg": "lg",
    "exp": "exp", "lim": "lim", "limsup": "limsup", "liminf": "liminf",
    "sup": "sup", "inf": "inf", "min": "min", "max": "max",
    "arg": "arg", "det": "det", "dim": "dim", "gcd": "gcd",
    "hom": "hom", "ker": "ker", "Pr": "Pr", "deg": "deg",
    
    # Spacing
    "displaystyle": "display", "textstyle": "inline",
    "hspace": "#h", ",": "thin", ":": "med", ";": "thick",
    ">": "med", " ": "med", "~": "space.nobreak",
    
    # Accents and modifiers
    "hat": "hat", "widehat": "hat", "check": "caron", "tilde": "tilde",
    "widetilde": "tilde", "acute": "acute", "grave": "grave",
    "dot": "dot", "ddot": "dot.double", "dddot": "dot.triple",
    "breve": "breve", "bar": "macron", "vec": "arrow",
    "overline": "overline", "underline": "underline",
    "overbrace": "overbrace", "underbrace": "underbrace",
    
    # Misc
    "|": "bar.v.double",
    "blacktriangleleft": "triangle.filled.l",
    "blacktriangleright": "triangle.filled.r",
    "square": "square", "blacksquare": "square.filled",
    "lozenge": "lozenge", "blacklozenge": "lozenge.filled",
    "clubsuit": "suit.club", "diamondsuit": "suit.diamond",
    "heartsuit": "suit.heart", "spadesuit": "suit.spade",
}

# ============================================================================
# COMMANDS WITH ARGUMENTS
# These commands need explicit argument specifications for mitex-parser
# Format: "command_name": num_required_args
# ============================================================================

COMMANDS_WITH_ARGS = {
    # Document structure (1 arg)
    "part": 1, "chapter": 1, "section": 1, "subsection": 1, "subsubsection": 1,
    "paragraph": 1, "title": 1, "author": 1, "date": 1, "caption": 1, "label": 1,
    
    # Macro definitions (2 args)
    "newcommand": 2, "renewcommand": 2, "providecommand": 2, "DeclareMathOperator": 2,
    
    # Math formatting (1 arg)
    "mathbf": 1, "mathit": 1, "mathrm": 1, "mathcal": 1, "mathbb": 1,
    "mathfrak": 1, "mathsf": 1, "mathtt": 1, "text": 1, "textrm": 1,
    "textbf": 1, "textit": 1, "texttt": 1, "textsc": 1, "emph": 1,
    "boldsymbol": 1, "bm": 1,
    
    # Accents (1 arg) - these override the symbol-only definitions
    "hat": 1, "widehat": 1, "tilde": 1, "widetilde": 1, "bar": 1,
    "overline": 1, "underline": 1, "vec": 1, "dot": 1, "ddot": 1,
    "overbrace": 1, "underbrace": 1, "check": 1, "acute": 1, "grave": 1,
    "breve": 1,
    
    # Limits and stacking (2 args)
    "overset": 2, "underset": 2, "stackrel": 2,
    
    # Extensible arrows (1 arg, optional arg handled by parser)
    "xleftarrow": 1, "xrightarrow": 1, "xmapsto": 1, "xleftrightarrow": 1,
    
    # Math classes (1 arg)
    "mathrel": 1, "mathbin": 1, "mathop": 1, "mathord": 1,
    "mathopen": 1, "mathclose": 1, "mathpunct": 1, "mathinner": 1,
    
    # Misc math (1 arg)
    "pmod": 1, "pod": 1, "displaylines": 1, "set": 1, "Set": 1,
    "sqrt": 1, "not": 1, "phantom": 1, "cancel": 1, "bcancel": 1,
    "boxed": 1, "fbox": 1,
    
    # Fractions and roots (2 args)
    "frac": 2, "dfrac": 2, "tfrac": 2, "cfrac": 2, "binom": 2,
    
    # Colors (1-2 args)
    "textcolor": 2, "colorbox": 2, "color": 1,
    
    # Links (1-2 args)
    "url": 1, "href": 2,
    
    # References (1 arg)
    "ref": 1, "eqref": 1, "autoref": 1, "pageref": 1, "cref": 1, "Cref": 1,
    
    # Footnote (1 arg)
    "footnote": 1,
    
    # Citations (1 arg)
    "cite": 1, "citep": 1, "citet": 1, "autocite": 1, "textcite": 1,
    "parencite": 1, "footcite": 1,
    
    # Acronyms and glossaries (1 arg usage, 2-3 args definition)
    "ac": 1, "gls": 1, "Gls": 1, "acrshort": 1, "acrlong": 1, "acrfull": 1,
    "Acs": 1, "Acl": 1, "Acf": 1,
    "newacronym": 3, "newglossaryentry": 2,
    
    # Special cite command
    "typstcite": 1,
}

TYPST_TO_TEX = {
    # Greek lowercase
    "alpha": "alpha", "beta": "beta", "gamma": "gamma", "delta": "delta",
    "epsilon": "epsilon", "epsilon.alt": "varepsilon", "zeta": "zeta",
    "eta": "eta", "theta": "theta", "theta.alt": "vartheta",
    "iota": "iota", "kappa": "kappa", "lambda": "lambda", "mu": "mu",
    "nu": "nu", "xi": "xi", "pi": "pi", "pi.alt": "varpi",
    "rho": "rho", "rho.alt": "varrho", "sigma": "sigma", "sigma.alt": "varsigma",
    "tau": "tau", "upsilon": "upsilon", "phi": "varphi", "phi.alt": "phi",
    "chi": "chi", "psi": "psi", "omega": "omega",
    
    # Greek uppercase
    "Gamma": "Gamma", "Delta": "Delta", "Theta": "Theta", "Lambda": "Lambda",
    "Xi": "Xi", "Pi": "Pi", "Sigma": "Sigma", "Upsilon": "Upsilon",
    "Phi": "Phi", "Psi": "Psi", "Omega": "Omega",
    
    # Operators
    "plus.minus": "pm", "minus.plus": "mp", "times": "times", "div": "div",
    "dot.op": "cdot", "sect": "cap", "union": "cup",
    "lt.eq": "leq", "gt.eq": "geq", "eq.not": "neq",
    "approx": "approx", "equiv": "equiv", "tilde.op": "sim",
    "subset": "subset", "supset": "supset", "subset.eq": "subseteq",
    "supset.eq": "supseteq", "in": "in", "in.not": "notin",
    "forall": "forall", "exists": "exists", "not": "neg",
    
    # Arrows
    "arrow.r": "rightarrow", "arrow.l": "leftarrow",
    "arrow.l.r": "leftrightarrow", "arrow.r.double": "Rightarrow",
    "arrow.l.double": "Leftarrow", "arrow.l.r.double": "Leftrightarrow",
    
    # Misc
    "infinity": "infty", "emptyset": "emptyset",
    "diff": "partial", "nabla": "nabla",
    "sum": "sum", "product": "prod", "integral": "int",
    
    # Functions
    "sin": "sin", "cos": "cos", "tan": "tan", "log": "log", "ln": "ln",
    "exp": "exp", "lim": "lim", "max": "max", "min": "min",
    
    # Dots
    "dots.h": "ldots", "dots.c": "cdots", "dots.v": "vdots",
    
    # Special
    "oo": "infty",
}

def escape_rust_string(s):
    """Escape a string for use in Rust"""
    return s.replace('\\', '\\\\').replace('"', '\\"')

def generate_rust_code():
    """Generate Rust source code from embedded mappings
    
    Uses:
    - lazy_static for TEX_COMMAND_SPEC (requires runtime construction with mitex types)
    - phf::phf_map! for TYPST_TO_TEX (compile-time perfect hash)
    """
    lines = [
        "// Generated by tools/gen_maps.py",
        "// This file contains static symbol mappings from tex2typst project",
        "// Do not edit manually - regenerate using: python tools/gen_maps.py",
        "",
        "use mitex_spec::{CommandSpec, CommandSpecItem, CmdShape, ArgShape, ArgPattern};",
        "use fxhash::FxHashMap;",
        "use lazy_static::lazy_static;",
        "use phf::phf_map;",
        "",
        "// =============================================================================",
        "// TEX_COMMAND_SPEC: Runtime-constructed CommandSpec for mitex parser",
        "// Uses lazy_static because CommandSpec requires runtime construction",
        "// =============================================================================",
        "",
        "lazy_static! {",
        "    /// LaTeX command specification for Mitex",
        "    pub static ref TEX_COMMAND_SPEC: CommandSpec = {",
        "        let mut m = FxHashMap::default();",
    ]
    
    # Generate CommandSpec entries for symbols (no args)
    for cmd, alias in sorted(SYMBOL_MAP.items()):
        if not cmd or not alias:
            continue
        # Skip if this command has args defined (will be handled later)
        if cmd in COMMANDS_WITH_ARGS:
            continue
        cmd_esc = escape_rust_string(cmd)
        alias_esc = escape_rust_string(alias)
        lines.append(f'        m.insert("{cmd_esc}".to_string(), CommandSpecItem::Cmd(CmdShape {{')
        lines.append('            args: ArgShape::Right { pattern: ArgPattern::None },')
        lines.append(f'            alias: Some("{alias_esc}".to_string()),')
        lines.append('        }));')
    
    # Add special typstcite with alias
    lines.append('        m.insert("typstcite".to_string(), CommandSpecItem::Cmd(CmdShape {')
    lines.append('            args: ArgShape::Right { pattern: ArgPattern::FixedLenTerm { len: 1 } },')
    lines.append('            alias: Some("__typstcite__".to_string()),')
    lines.append('        }));')
    
    # Add aligned environment
    lines.append('        m.insert("aligned".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {')
    lines.append('            args: ArgPattern::None,')
    lines.append('            ctx_feature: mitex_spec::ContextFeature::None,')
    lines.append('            alias: None,')
    lines.append('        }));')
    
    lines.append('')
    lines.append('        // Commands with required arguments')
    
    # Generate CommandSpec entries for commands with args
    for cmd, num_args in sorted(COMMANDS_WITH_ARGS.items()):
        if cmd == "typstcite":  # Already handled above with alias
            continue
        cmd_esc = escape_rust_string(cmd)
        lines.append(f'        m.insert("{cmd_esc}".to_string(), CommandSpecItem::Cmd(CmdShape {{')
        lines.append(f'            args: ArgShape::Right {{ pattern: ArgPattern::FixedLenTerm {{ len: {num_args} }} }},')
        lines.append('            alias: None,')
        lines.append('        }));')
    
    lines.extend([
        "",
        "        CommandSpec::new(m)",
        "    };",
        "}",
        "",
        "// =============================================================================",
        "// TYPST_TO_TEX: Compile-time perfect hash map for Typst -> LaTeX conversion",
        "// Uses phf for O(1) lookup with zero runtime initialization cost",
        "// =============================================================================",
        "",
        "/// Typst to LaTeX symbol mapping (compile-time perfect hash)",
        "pub static TYPST_TO_TEX: phf::Map<&'static str, &'static str> = phf_map! {",
    ])
    
    # Generate phf_map entries
    for typst, tex in sorted(TYPST_TO_TEX.items()):
        if not typst or not tex:
            continue
        typst_esc = escape_rust_string(typst)
        tex_esc = escape_rust_string(tex)
        lines.append(f'    "{typst_esc}" => "{tex_esc}",')
    
    lines.extend([
        "};",
    ])
    
    return "\n".join(lines)

def parse_map_ts(filepath):
    """Parse map.ts file if provided (optional, for updating mappings)"""
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Extract maps using regex
    patterns = [
        r"\['([^']*)',\s*'([^']*)'\]",
        r'\["([^"]*)",\s*"([^"]*)"\]',
    ]
    
    mappings = {}
    for pattern in patterns:
        for match in re.finditer(pattern, content):
            key, value = match.groups()
            if key and value:
                mappings[key] = value
    
    return mappings

def main():
    # Output to the new data module location
    output_path = Path(__file__).parent.parent / "src" / "data" / "maps.rs"
    
    if len(sys.argv) > 1:
        # Update from external source
        map_ts_path = Path(sys.argv[1])
        if map_ts_path.exists():
            print(f"Updating mappings from: {map_ts_path}")
            external_maps = parse_map_ts(map_ts_path)
            SYMBOL_MAP.update(external_maps)
            print(f"Added {len(external_maps)} external mappings")
    
    print(f"Generating maps.rs with {len(SYMBOL_MAP)} tex->typst and {len(TYPST_TO_TEX)} typst->tex mappings")
    print(f"Using: lazy_static for TEX_COMMAND_SPEC, phf for TYPST_TO_TEX")
    
    rust_code = generate_rust_code()
    
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write(rust_code)
    
    print(f"Generated: {output_path}")

if __name__ == "__main__":
    main()
