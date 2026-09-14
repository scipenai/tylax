#!/usr/bin/env python3
"""
Generate Rust symbol mapping code for Tylax

This script generates a baseline for the maps.rs symbol mappings.
The mappings are based on tex2typst project but are now embedded directly.

The checked-in map has accumulated mappings outside this legacy embedded data.
Before writing, the script verifies that its output would not discard any
existing public map or known key. If it would, it exits without modifying the
file. Update the generator's source data first instead of overwriting maps.rs.

Note: This script is only needed when you want to update the symbol mappings.
For normal usage, the pre-generated maps.rs is sufficient.

Usage:
    python gen_maps.py                    # Regenerate only when lossless
    python gen_maps.py path/to/map.ts     # Add mappings from tex2typst, then regenerate
"""

import re
import sys
from pathlib import Path

# ============================================================================
# EMBEDDED SYMBOL MAPPINGS
# These are extracted from tex2typst project for independence
# ============================================================================

SYMBOL_MAP = {
    # Multi-integral symbols
    "iiiint": "integral.quad",
    "oiiint": "integral.vol",
    "oiint": "integral.surf",
    # Greek lowercase
    "alpha": "alpha", "beta": "beta", "gamma": "gamma", "delta": "delta",
    "epsilon": "epsilon.alt", "varepsilon": "epsilon", "zeta": "zeta",
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
    # newtxmath/txfonts slanted capital Greek. Typst has no distinct slanted
    # capital glyphs, so preserve the mathematical symbol with its plain form.
    "varGamma": "Gamma", "varDelta": "Delta", "varTheta": "Theta",
    "varLambda": "Lambda", "varXi": "Xi", "varPi": "Pi",
    "varSigma": "Sigma", "varUpsilon": "Upsilon", "varPhi": "Phi",
    "varPsi": "Psi", "varOmega": "Omega",
    
    # Binary operators
    "pm": "plus.minus", "mp": "minus.plus", "times": "times", "div": "div",
    "cdot": "dot.op", "ast": "ast", "star": "star", "circ": "circle.small",
    "bullet": "bullet", "oplus": "plus.o", "ominus": "minus.o",
    "otimes": "times.o", "oslash": "slash.o", "odot": "dot.o",
    "cap": "inter", "cup": "union", "sqcap": "sect.sq", "sqcup": "union.sq",
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
    "langle": "chevron.l", "rangle": "chevron.r",
    "lceil": "ceil.l", "rceil": "ceil.r",
    "lfloor": "floor.l", "rfloor": "floor.r",
    "lbrace": "brace.l", "rbrace": "brace.r",
    "lvert": "bar.v", "rvert": "bar.v",
    "lVert": "bar.v.double", "rVert": "bar.v.double",
    
    # Big operators
    "sum": "sum", "prod": "product", "coprod": "product.co",
    "int": "integral", "iint": "integral.double", "iiint": "integral.triple",
    "oint": "integral.cont", "bigcap": "inter.big", "bigcup": "union.big",
    "bigsqcup": "union.sq.big", "bigvee": "or.big", "bigwedge": "and.big",
    "bigoplus": "plus.o.big", "bigotimes": "times.o.big",
    "bigodot": "dot.o.big",
    
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
    "displaystyle": "display", "textstyle": "inline", "cal": "cal",
    "hspace": "#h", ",": "thin", ":": "med", ";": "thick",
    ">": "med", " ": "med", "~": "space.nobreak",
    
    # Accents and modifiers
    "hat": "hat", "widehat": "hat", "check": "caron", "tilde": "tilde",
    "widetilde": "tilde", "acute": "acute", "grave": "grave",
    "dot": "dot", "ddot": "dot.double", "dddot": "dot.triple",
    "breve": "breve", "bar": "macron", "vec": "arrow",
    "overrightarrow": "arrow", "overleftarrow": "arrow.l",
    "overleftrightarrow": "arrow.l.r",
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

# Argument signature of each environment's `\begin{env}` header, in the same
# glob language the commands use: `b` is an optional `[..]` slot, `t` a required
# `{..}` one, and `{,X}` makes a slot optional.
#
# Without a signature the header's arguments are not bound and leak into the
# body — and a "skip one leading bracket" heuristic cannot express a shape like
# minipage's three optional slots, or multicols' required-before-optional order.
# An environment absent from this table consumes nothing, which is correct:
# after `\begin{center}` a `[x]` really is the text LaTeX prints.
ENVIRONMENT_SIGNATURES = {
    # Floats: `[htbp]` placement.
    "figure": "{,b}", "figure*": "{,b}", "table": "{,b}", "table*": "{,b}",
    "wrapfigure": "{,b}",
    # enumitem and friends: `[label=.., itemsep=..]`.
    "enumerate": "{,b}", "itemize": "{,b}", "description": "{,b}", "list": "{,b}",
    # Layout / listing-style boxes.
    "adjustbox": "{,b}", "tcolorbox": "{,b}",
    "algorithm": "{,b}", "algorithmic": "{,b}",
    "lstlisting": "{,b}",
    # `\begin{minipage}[pos][height][inner-pos]{width}`
    "minipage": "{,b}{,b}{,b}t",
    # `\begin{multicols}{2}[Header]` — required slot BEFORE the optional one.
    "multicols": "t{,b}", "multicols*": "t{,b}",
    # `\begin{tabular}[pos]{cols}`; the starred/x forms take a width first.
    "tabular": "{,b}t", "longtable": "{,b}t", "longtabu": "{,b}t", "array": "{,b}t",
    "tabular*": "t{,b}t", "tabularx": "t{,b}t",
}

# Commands shaped `\cmd[optional]{required}`. A fixed arity cannot express the
# optional argument: the parser then binds nothing at all and BOTH arguments
# degrade to body text. They are emitted with a glob pattern instead.
OPTIONAL_ARG_COMMANDS = [
    "part", "chapter", "section", "subsection", "subsubsection",
    "paragraph", "subparagraph",
    # `\caption[short]{long}` — the short form is a list-of-figures entry.
    "caption",
    # `\sqrt[n]{x}` has the same shape; a fixed arity of 1 would drop the index.
    "sqrt",
    "dd",
    "differential",
    "hyperref",
]

# Commands whose glob shape is neither a fixed arity nor `{,b}t`:
# `\cmd[opt]{a}{b}` and `\cmd[opt]{a}{b}{c}`.
GLOB_ARG_COMMANDS = {
    "derivative": "{,b}tt",
    "dv": "{,b}tt",
    "dv*": "{,b}tt",
    "fderivative": "{,b}tt",
    "fdv": "{,b}tt",
    "fdv*": "{,b}tt",
    "functionalderivative": "{,b}tt",
    "partialderivative": "{,b}ttt",
    "pderivative": "{,b}ttt",
    "pdv": "{,b}ttt",
    "pdv*": "{,b}ttt",
}

# Zero-argument commands with no Typst alias: known to the parser so
# they are not mistaken for text, rendered by the converter.
BARE_COMMANDS = [
    "cp",
    "cross",
    "crossproduct",
    "divisionsymbol",
    "dotproduct",
    "injlim",
    "projlim",
    "qall",
    "qand",
    "qas",
    "qassume",
    "qc",
    "qcc",
    "qcomma",
    "qelse",
    "qeven",
    "qfor",
    "qgiven",
    "qif",
    "qin",
    "qinteger",
    "qlet",
    "qodd",
    "qor",
    "qotherwise",
    "qsince",
    "qthen",
    "qunless",
    "qusing",
    "varinjlim",
    "varprojlim",
    "vdot",
]

COMMANDS_WITH_ARGS = {
    # Document structure (1 arg)
    # NOTE: the sectioning commands are NOT here — they take an optional
    # `[short title]` that a fixed arity cannot express. See OPTIONAL_ARG_COMMANDS.
    "title": 1, "author": 1, "date": 1, "label": 1,
    
    # Macro definitions (2 args)
    "newcommand": 2, "renewcommand": 2, "providecommand": 2, "DeclareMathOperator": 2,
    
    # Math formatting (1 arg)
    "mathbf": 1, "mathit": 1, "mathrm": 1, "mathcal": 1, "mathbb": 1,
    "mathfrak": 1, "mathsf": 1, "mathtt": 1, "text": 1, "textrm": 1,
    "textbf": 1, "textit": 1, "texttt": 1, "textsc": 1, "emph": 1,
    "boldsymbol": 1, "bm": 1,
    
    # Accents (1 arg) - these override the symbol-only definitions
    "hat": 1, "widehat": 1, "tilde": 1, "widetilde": 1, "bar": 1,
    "overline": 1, "underline": 1, "vec": 1, "overleftarrow": 1,
    "overleftrightarrow": 1, "overrightarrow": 1, "dot": 1, "ddot": 1,
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
    # `textcircled` must stay here: without a pattern mitex leaves `{..}` as a
    # following sibling instead of binding it, and the converter can no longer
    # see the operator it wraps, so `\textcircled{\cdot}` loses its circle.
    "pmod": 1, "pod": 1, "displaylines": 1, "set": 1, "Set": 1,
    "not": 1, "phantom": 1, "cancel": 1, "bcancel": 1,
    "boxed": 1, "fbox": 1, "hspace": 1, "hspace*": 1, "vspace": 1, "vspace*": 1,
    "textcircled": 1,
    
    # Fractions and roots (2 args)
    "frac": 2, "dfrac": 2, "tfrac": 2, "cfrac": 2, "binom": 2,
    
    # Colors (1-3 args)
    "textcolor": 2, "colorbox": 2, "color": 1, "fcolorbox": 3,
    "highlight": 1, "hl": 1,

    # Table cell spans (3 args)
    "multicolumn": 3, "multirow": 3,

    # Bibliography entry: `ibitem{key}`
    "bibitem": 1,

    # Extensible arrow variants (1 arg)
    "xLeftarrow": 1, "xLeftrightarrow": 1, "xRightarrow": 1,
    "xhookleftarrow": 1, "xhookrightarrow": 1, "xleftharpoondown": 1,
    "xleftharpoonup": 1, "xleftrightharpoons": 1, "xlongequal": 1,
    "xrightharpoondown": 1, "xrightharpoonup": 1, "xrightleftharpoons": 1,
    "xtofrom": 1, "xtwoheadleftarrow": 1, "xtwoheadrightarrow": 1,
    
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
    # Physics / mathtools commands that `maps.rs` carried only through its
    # local `cmd1()`/`cmd2()`/`cmd3()` shorthands.
    "Bqty": 1, "PV": 1, "Pmqty": 1, "Res": 1, "Residue": 1, "abs": 1, "abs*": 1, "absolutevalue": 1, "admat": 1, "antidiagonalmatrix": 1, "bmqty": 1, "bqty": 1, "bra": 1, "bra*": 1, "curl": 1, "diagonalmatrix": 1, "divergence": 1, "dmat": 1, "eval": 1, "eval*": 1, "evaluated": 1, "grad": 1, "gradient": 1, "identitymatrix": 1, "imat": 1, "ket": 1, "ket*": 1, "laplacian": 1, "matrixdeterminant": 1, "matrixquantity": 1, "mdet": 1, "mqty": 1, "norm": 1, "norm*": 1, "order": 1, "order*": 1, "paulimatrix": 1, "pmat": 1, "pmqty": 1, "pqty": 1, "principalvalue": 1, "pv": 1, "qq": 1, "qqtext": 1, "sPmqty": 1, "sbmqty": 1, "smallmatrixdeterminant": 1, "smallmatrixquantity": 1, "smdet": 1, "smqty": 1, "spmqty": 1, "svmqty": 1, "va": 1, "var": 1, "variation": 1, "vb": 1, "vectorarrow": 1, "vectorbold": 1, "vectorunit": 1, "vev": 1, "vmqty": 1, "vqty": 1, "vu": 1,
    "acomm": 2, "acomm*": 2, "acommutator": 2, "anticommutator": 2, "braket": 2, "braket*": 2, "comm": 2, "comm*": 2, "commutator": 2, "dyad": 2, "dyad*": 2, "ev": 2, "ev*": 2, "expectationvalue": 2, "expval": 2, "expval*": 2, "flatfrac": 2, "innerproduct": 2, "ip": 2, "ketbra": 2, "op": 2, "outerproduct": 2, "pb": 2, "pb*": 2, "poissonbracket": 2, "zeromatrix": 2, "zmat": 2,
    "matrixel": 3, "matrixelement": 3, "mel": 3, "mel*": 3, "xmat": 3, "xmatrix": 3,
}

DELIMITER_MAP = {
    # Typst delimiter -> LaTeX, for lr() conversion.
    # Parentheses
    '(': '(',
    ')': ')',
    'paren.l': '(',
    'paren.r': ')',
    # Brackets
    '[': '[',
    ']': ']',
    'bracket.l': '[',
    'bracket.r': ']',
    # Braces
    '{': '\\{',
    '}': '\\}',
    'brace.l': '\\{',
    'brace.r': '\\}',
    # Single bars
    '|': '|',
    'bar.v': '|',
    'vert': '|',
    # Double bars
    '||': '\\|',
    'bar.v.double': '\\|',
    'vert.double': '\\|',
    # Angle brackets (Unicode)
    '⟨': '\\langle',
    '⟩': '\\rangle',
    '〈': '\\langle',
    '〉': '\\rangle',
    # Angle brackets (Typst names)
    'angle.l': '\\langle',
    'angle.r': '\\rangle',
    'chevron.l': '\\langle',
    'chevron.r': '\\rangle',
    # Floor (Unicode)
    '⌊': '\\lfloor',
    '⌋': '\\rfloor',
    # Floor (Typst names)
    'floor.l': '\\lfloor',
    'floor.r': '\\rfloor',
    # Ceiling (Unicode)
    '⌈': '\\lceil',
    '⌉': '\\rceil',
    # Ceiling (Typst names)
    'ceil.l': '\\lceil',
    'ceil.r': '\\rceil',
}

TYPST_TO_TEX = {
    # Typst -> LaTeX symbol mapping, emitted as a phf map.
    # =========================================================================
    'Delta': 'Delta',
    'Gamma': 'Gamma',
    'Lambda': 'Lambda',
    'Omega': 'Omega',
    'Phi': 'Phi',
    'Pi': 'Pi',
    'Psi': 'Psi',
    'Sigma': 'Sigma',
    'Theta': 'Theta',
    'Upsilon': 'Upsilon',
    'Xi': 'Xi',
    # =========================================================================
    'alpha': 'alpha',
    'beta': 'beta',
    'gamma': 'gamma',
    'delta': 'delta',
    'epsilon': 'varepsilon',
    'epsilon.alt': 'epsilon',
    'zeta': 'zeta',
    'eta': 'eta',
    'theta': 'theta',
    'theta.alt': 'vartheta',
    'iota': 'iota',
    'kappa': 'kappa',
    'kappa.alt': 'varkappa',
    'lambda': 'lambda',
    'mu': 'mu',
    'nu': 'nu',
    'xi': 'xi',
    'pi': 'pi',
    'pi.alt': 'varpi',
    'rho': 'rho',
    'rho.alt': 'varrho',
    'sigma': 'sigma',
    'sigma.alt': 'varsigma',
    'tau': 'tau',
    'upsilon': 'upsilon',
    'phi': 'varphi',
    'phi.alt': 'phi',
    'chi': 'chi',
    'psi': 'psi',
    'omega': 'omega',
    # =========================================================================
    'arrow.r': 'rightarrow',
    'arrow.l': 'leftarrow',
    'arrow.t': 'uparrow',
    'arrow.b': 'downarrow',
    'arrow.l.r': 'leftrightarrow',
    'arrow.t.b': 'updownarrow',
    'arrow.r.double': 'Rightarrow',
    'arrow.l.double': 'Leftarrow',
    'arrow.t.double': 'Uparrow',
    'arrow.b.double': 'Downarrow',
    'arrow.l.r.double': 'Leftrightarrow',
    'arrow.t.b.double': 'Updownarrow',
    'arrow.r.long': 'longrightarrow',
    'arrow.l.long': 'longleftarrow',
    'arrow.l.r.long': 'longleftrightarrow',
    'arrow.r.double.long': 'Longrightarrow',
    'arrow.l.double.long': 'Longleftarrow',
    'arrow.l.r.double.long': 'Longleftrightarrow',
    'arrow.r.tail': 'rightarrowtail',
    'arrow.l.tail': 'leftarrowtail',
    'arrow.r.hook': 'hookrightarrow',
    'arrow.l.hook': 'hookleftarrow',
    'arrow.r.squiggly': 'rightsquigarrow',
    'arrow.l.squiggly': 'leftsquigarrow',
    'arrow.r.twohead': 'twoheadrightarrow',
    'arrow.l.twohead': 'twoheadleftarrow',
    'arrow.r.bar': 'mapsto',
    'arrow.l.bar': 'mapsfrom',
    'arrow.r.long.bar': 'longmapsto',
    'harpoon.rt': 'rightharpoonup',
    'harpoon.rb': 'rightharpoondown',
    'harpoon.lt': 'leftharpoonup',
    'harpoon.lb': 'leftharpoondown',
    'harpoons.ltrb': 'leftrightharpoons',
    'harpoons.rtlb': 'rightleftharpoons',
    'arrows.rr': 'rightrightarrows',
    'arrows.ll': 'leftleftarrows',
    'arrows.lr': 'leftrightarrows',
    'arrows.rl': 'rightleftarrows',
    'arrow.ne': 'nearrow',
    'arrow.se': 'searrow',
    'arrow.sw': 'swarrow',
    'arrow.nw': 'nwarrow',
    # =========================================================================
    'plus.minus': 'pm',
    'minus.plus': 'mp',
    'times': 'times',
    'div': 'div',
    'ast': 'ast',
    'star': 'star',
    'circle.small': 'circ',
    'bullet': 'bullet',
    'dot.op': 'cdot',
    'dot.c': 'cdot',
    'circle.plus': 'oplus',
    'circle.minus': 'ominus',
    'circle.times': 'otimes',
    'circle.div': 'oslash',
    'circle.dot': 'odot',
    'square.plus': 'boxplus',
    'square.minus': 'boxminus',
    'square.times': 'boxtimes',
    'square.dot': 'boxdot',
    'wreath': 'wr',
    'diamond.op': 'diamond',
    'triangle.t': 'bigtriangleup',
    'triangle.b': 'bigtriangledown',
    'triangle.l': 'triangleleft',
    'triangle.r': 'triangleright',
    'dagger': 'dagger',
    'dagger.double': 'ddagger',
    'amalg': 'amalg',
    'sect': 'cap',
    'union': 'cup',
    'sect.sq': 'sqcap',
    'union.sq': 'sqcup',
    'sect.big': 'bigcap',
    'union.big': 'bigcup',
    'union.sq.big': 'bigsqcup',
    'sect.sq.big': 'bigsqcap',
    'and': 'wedge',
    'or': 'vee',
    'and.big': 'bigwedge',
    'or.big': 'bigvee',
    'plus.circle': 'oplus',
    'plus.circle.big': 'bigoplus',
    'times.circle': 'otimes',
    'times.circle.big': 'bigotimes',
    'dot.circle.big': 'bigodot',
    # pre-0.12 deprecated form kept for backward compatibility.
    'plus.o': '\\oplus',
    'plus.o.big': '\\bigoplus',
    'minus.o': '\\ominus',
    'times.o': '\\otimes',
    'times.o.big': '\\bigotimes',
    'dot.o': '\\odot',
    'dot.o.big': '\\bigodot',
    'slash.o': '\\oslash',
    # =========================================================================
    'lt': 'lt',
    'gt': 'gt',
    'lt.eq': 'leq',
    'gt.eq': 'geq',
    'lt.eq.slant': 'leqslant',
    'gt.eq.slant': 'geqslant',
    'lt.double': 'll',
    'gt.double': 'gg',
    'lt.triple': 'lll',
    'gt.triple': 'ggg',
    'lt.not': 'nless',
    'gt.not': 'ngtr',
    'lt.eq.not': 'nleq',
    'gt.eq.not': 'ngeq',
    'eq': 'eq',
    'eq.not': 'neq',
    'equiv': 'equiv',
    'equiv.not': 'nequiv',
    'approx': 'approx',
    'approx.not': 'napprox',
    'tilde.op': 'sim',
    'tilde.eq': 'simeq',
    'tilde.eq.not': 'nsimeq',
    'tilde.equiv': 'cong',
    'tilde.equiv.not': 'ncong',
    'prop': 'propto',
    'prec': 'prec',
    'succ': 'succ',
    'prec.eq': 'preceq',
    'succ.eq': 'succeq',
    'prec.not': 'nprec',
    'succ.not': 'nsucc',
    'subset': 'subset',
    'supset': 'supset',
    'subset.eq': 'subseteq',
    'supset.eq': 'supseteq',
    'subset.not': 'nsubset',
    'supset.not': 'nsupset',
    'subset.eq.not': 'nsubseteq',
    'supset.eq.not': 'nsupseteq',
    'subset.sq': 'sqsubset',
    'supset.sq': 'sqsupset',
    'subset.eq.sq': 'sqsubseteq',
    'supset.eq.sq': 'sqsupseteq',
    'in': 'in',
    'in.not': 'notin',
    'in.rev': 'ni',
    'in.rev.not': 'notni',
    'divides': 'mid',
    'divides.not': 'nmid',
    'parallel': 'parallel',
    'parallel.not': 'nparallel',
    'perp': 'perp',
    'models': 'models',
    'forces': 'Vdash',
    'tack.r': 'vdash',
    'tack.l': 'dashv',
    'tack.t': 'top',
    'tack.b': 'bot',
    'tack.r.double': 'vDash',
    'tack.l.double': 'Dashv',
    'tack.r.not': 'nvdash',
    'tack.r.double.not': 'nvDash',
    'colon': 'colon',
    'coloneq': 'coloneqq',
    'eqcolon': 'eqqcolon',
    'doteq': 'doteq',
    'asymp': 'asymp',
    'bowtie': 'bowtie',
    'smile': 'smile',
    'frown': 'frown',
    # =========================================================================
    'sum': 'sum',
    'product': 'prod',
    'coproduct': 'coprod',
    'integral': 'int',
    'integral.double': 'iint',
    'integral.triple': 'iiint',
    'integral.quad': 'iiiint',
    'integral.cont': 'oint',
    'integral.surf': 'oiint',
    'integral.vol': 'oiiint',
    # =========================================================================
    'sin': 'sin',
    'cos': 'cos',
    'tan': 'tan',
    'cot': 'cot',
    'sec': 'sec',
    'csc': 'csc',
    'arcsin': 'arcsin',
    'arccos': 'arccos',
    'arctan': 'arctan',
    'sinh': 'sinh',
    'cosh': 'cosh',
    'tanh': 'tanh',
    'coth': 'coth',
    'exp': 'exp',
    'log': 'log',
    'ln': 'ln',
    'lg': 'lg',
    'lim': 'lim',
    'limsup': 'limsup',
    'liminf': 'liminf',
    'max': 'max',
    'min': 'min',
    'sup': 'sup',
    'inf': 'inf',
    'arg': 'arg',
    'det': 'det',
    'dim': 'dim',
    'gcd': 'gcd',
    'lcm': 'operatorname{lcm}',
    'deg': 'deg',
    'hom': 'hom',
    'ker': 'ker',
    'Pr': 'Pr',
    'Im': 'Im',
    'Re': 'Re',
    'argmin': 'argmin',
    'argmax': 'argmax',
    # =========================================================================
    'paren.l': '(',
    'paren.r': ')',
    'bracket.l': '[',
    'bracket.r': ']',
    'brace.l': '\\{',
    'brace.r': '\\}',
    'chevron.l': 'langle',
    'chevron.r': 'rangle',
    'angle.l': 'langle',
    'angle.r': 'rangle',
    'floor.l': 'lfloor',
    'floor.r': 'rfloor',
    'ceil.l': 'lceil',
    'ceil.r': 'rceil',
    'vert': 'vert',
    'vert.double': 'Vert',
    'bar.v': '|',
    'bar.v.double': '\\|',
    # =========================================================================
    'infinity': 'infty',
    'oo': 'infty',
    'diff': 'partial',
    'partial': 'partial',
    'nabla': 'nabla',
    'gradient': 'nabla',
    'laplace': 'Delta',
    'emptyset': 'emptyset',
    'nothing': 'varnothing',
    'aleph': 'aleph',
    'beth': 'beth',
    'gimel': 'gimel',
    'daleth': 'daleth',
    'ell': 'ell',
    'planck': 'hbar',
    'planck.reduce': 'hbar',
    'hbar': 'hbar',
    'imath': 'imath',
    'jmath': 'jmath',
    'wp': 'wp',
    'prime': 'prime',
    'forall': 'forall',
    'exists': 'exists',
    'exists.not': 'nexists',
    'not': 'neg',
    'complement': 'complement',
    'circle': 'circ',
    'degree': 'degree',
    'angle': 'angle',
    'angle.arc': 'measuredangle',
    'angle.spheric': 'sphericalangle',
    'diameter': 'diameter',
    'therefore': 'therefore',
    'because': 'because',
    'qed': 'square',
    'square': 'square',
    'square.stroked': 'square',
    'square.filled': 'blacksquare',
    'checkmark': 'checkmark',
    'ballot': 'times',
    # =========================================================================
    'dots': 'ldots',
    'dots.h': 'ldots',
    'dots.c': 'cdots',
    'dots.v': 'vdots',
    'dots.down': 'ddots',
    'dots.up': 'iddots',
    # =========================================================================
    'space': '\\ ',
    'space.thin': '\\,',
    'space.med': '\\:',
    'space.thick': '\\;',
    'space.quad': '\\quad',
    'space.wide': '\\qquad',
    'space.neg': '\\!',
    # =========================================================================
    'hat': 'hat',
    'grave': 'grave',
    'acute': 'acute',
    'tilde': 'tilde',
    'macron': 'bar',
    'breve': 'breve',
    'dot': 'cdot',
    'diaer': 'ddot',
    'caron': 'check',
    'circle.stroked.tiny': 'mathring',
    'vec': 'vec',
    # =========================================================================
    'suit.club': 'clubsuit',
    'suit.diamond': 'diamondsuit',
    'suit.heart': 'heartsuit',
    'suit.spade': 'spadesuit',
    # =========================================================================
    'sharp': 'sharp',
    'flat': 'flat',
    'natural': 'natural',
    # =========================================================================
    'dollar': '\\$',
    'euro': 'euro',
    'pound': 'pounds',
    'yen': 'textyen',
    'percent': '\\%',
    'copyright': 'copyright',
    'trademark': 'texttrademark',
    'registered': 'textregistered',
    'section': 'S',
    'paragraph': 'P',
    'star.filled': 'bigstar',
    # =========================================================================
    'dif': 'mathrm{d}',
    'RR': 'mathbb{R}',
    'NN': 'mathbb{N}',
    'ZZ': 'mathbb{Z}',
    'QQ': 'mathbb{Q}',
    'CC': 'mathbb{C}',
    'AA': 'forall',
    'EE': 'exists',
    # Shorthand arrows
    '->': 'rightarrow',
    '<-': 'leftarrow',
    '=>': 'Rightarrow',
    '<=>': 'Leftrightarrow',
    '<->': 'leftrightarrow',
    '-->': 'longrightarrow',
    '<--': 'longleftarrow',
    '==>': 'Longrightarrow',
    '<==': 'Longleftarrow',
    '<==>': 'Longleftrightarrow',
    '<-->': 'longleftrightarrow',
    '|->': 'mapsto',
    '|=>': 'Mapsto',
    '~>': 'rightsquigarrow',
    '<~': 'leftsquigarrow',
    '~~>': 'leadsto',
    '->>': 'twoheadrightarrow',
    '<<-': 'twoheadleftarrow',
    '>->': 'rightarrowtail',
    '<-<': 'leftarrowtail',
    # Shorthand operators
    '!=': 'neq',
    '>=': 'geq',
    '<=': 'leq',
    '>>': 'gg',
    '<<': 'll',
    '>>>': 'ggg',
    '<<<': 'lll',
    '||': '|',
    ':=': 'coloneqq',
    '=:': 'eqqcolon',
    '::=': 'Coloneqq',
    '...': 'ldots',
    # Additional Typst symbols
    'hyph': '-',
    'hyph.minus': '-',
    'comma': ',',
    'thin': ',',
    'med': ':',
    'thick': ';',
    'space.nobreak': '~',
    'eq.def': 'overset{\\text{def}}{=}',
    'eq.delta': 'triangleq',
    'eq.star': 'stackrel{*}{=}',
    'eq.quest': 'stackrel{?}{=}',
    # Additional unique symbols
    'star.op': '*',
    # Greek uppercase (trivial — rendered as Roman letters in LaTeX)
    'Alpha': 'A',
    'Beta': 'B',
    'Chi': 'X',
    'Digamma': '\\Digamma',
    'Epsilon': 'E',
    'Eta': 'H',
    'Iota': 'I',
    'Kappa': 'K',
    'Mu': 'M',
    'Nu': 'N',
    'Omicron': 'O',
    'Rho': 'P',
    'Tau': 'T',
    'Zeta': 'Z',
    'digamma': '\\digamma',
    # Blackboard bold (missing letters)
    'BB': '\\mathbb{B}',
    'DD': '\\mathbb{D}',
    'FF': '\\mathbb{F}',
    'GG': '\\mathbb{G}',
    'HH': '\\mathbb{H}',
    'II': '\\mathbb{I}',
    'JJ': '\\mathbb{J}',
    'KK': '\\mathbb{K}',
    'LL': '\\mathbb{L}',
    'MM': '\\mathbb{M}',
    'OO': '\\mathbb{O}',
    'PP': '\\mathbb{P}',
    'SS': '\\mathbb{S}',
    'TT': '\\mathbb{T}',
    'UU': '\\mathbb{U}',
    'VV': '\\mathbb{V}',
    'WW': '\\mathbb{W}',
    'XX': '\\mathbb{X}',
    'YY': '\\mathbb{Y}',
    # Arrows (negated/curved variants)
    'arrow.r.not': '\\nrightarrow',
    'arrow.l.not': '\\nleftarrow',
    'arrow.r.double.not': '\\nRightarrow',
    'arrow.l.double.not': '\\nLeftarrow',
    'arrow.l.r.not': '\\nleftrightarrow',
    'arrow.l.r.double.not': '\\nLeftrightarrow',
    'arrow.l.r.wave': '\\leftrightsquigarrow',
    'arrow.ccw': '\\curvearrowleft',
    'arrow.cw': '\\curvearrowright',
    # Comparison variants
    'lt.approx': '\\lessapprox',
    'lt.equiv': '\\leqq',
    'lt.gt': '\\lessgtr',
    'lt.tilde': '\\lesssim',
    'lt.tri': '\\vartriangleleft',
    'lt.tri.eq': '\\trianglelefteq',
    'lt.tri.not': '\\ntriangleleft',
    'lt.tri.eq.not': '\\ntrianglelefteq',
    'gt.approx': '\\gtrapprox',
    'gt.equiv': '\\geqq',
    'gt.lt': '\\gtrless',
    'gt.tilde': '\\gtrsim',
    'gt.tri': '\\vartriangleright',
    'gt.tri.eq': '\\trianglerighteq',
    'gt.tri.not': '\\ntriangleright',
    'gt.tri.eq.not': '\\ntrianglerighteq',
    # Precedence / Succession variants
    'prec.approx': '\\precapprox',
    'prec.curly.eq': '\\preccurlyeq',
    'prec.curly.eq.not': '\\npreccurlyeq',
    'prec.tilde': '\\precsim',
    'succ.approx': '\\succapprox',
    'succ.curly.eq': '\\succcurlyeq',
    'succ.curly.eq.not': '\\nsucccurlyeq',
    'succ.tilde': '\\succsim',
    # Tilde / Approx variants
    'tilde.not': '\\nsim',
    'tilde.rev': '\\backsim',
    'tilde.rev.equiv': '\\backcong',
    'approx.eq': '\\approxeq',
    'forces.not': '\\nVdash',
    # Set operation variants
    'subset.double': '\\Subset',
    'subset.neq': '\\subsetneq',
    'supset.double': '\\Supset',
    'supset.neq': '\\supsetneq',
    'union.dot': '\\cupdot',
    'union.double': '\\Cup',
    'union.plus': '\\uplus',
    'inter': '\\cap',
    'inter.big': '\\bigcap',
    'inter.double': '\\Cap',
    'inter.sq': '\\sqcap',
    'without': '\\setminus',
    # Binary operator variants
    'plus': '+',
    'plus.square': '\\boxplus',
    'minus': '-',
    'minus.circle': '\\ominus',
    'minus.square': '\\boxminus',
    'times.square': '\\boxtimes',
    'dot.circle': '\\odot',
    'ast.op': '\\ast',
    'xor': '\\oplus',
    'xor.big': '\\bigoplus',
    'product.co': '\\coprod',
    'dots.h.c': '\\cdots',
    # Triangle variants
    'triangle.stroked.t': '\\triangle',
    'triangle.stroked.b': '\\triangledown',
    'triangle.stroked.r': '\\triangleright',
    'triangle.stroked.l': '\\triangleleft',
    'triangle.stroked.small.t': '\\vartriangle',
    'triangle.filled.t': '\\blacktriangle',
    'triangle.filled.b': '\\blacktriangledown',
    'triangle.filled.r': '\\blacktriangleright',
    'triangle.filled.l': '\\blacktriangleleft',
    # Delimiter variants
    'angle.l.double': '\\lAngle',
    'angle.r.double': '\\rAngle',
    'shell.l': '\\lgroup',
    'shell.r': '\\rgroup',
    # Shapes
    'circle.stroked': '\\circ',
    'circle.stroked.small': '\\circ',
    'diamond.stroked': '\\diamond',
    'diamond.stroked.small': '\\diamond',
    # Dotless letters
    'dotless.i': '\\imath',
    'dotless.j': '\\jmath',
    # Suits
    'suit.club.filled': '\\clubsuit',
    'suit.spade.filled': '\\spadesuit',
    'suit.heart.stroked': '\\heartsuit',
    'suit.diamond.stroked': '\\diamondsuit',
    # Misc
    'join': '\\bowtie',
    'maltese': '\\maltese',
    'pilcrow': '\\P',
    'prime.double': '\\prime\\prime',
    'harpoons.rtlt': '\\upharpoonright\\!\\upharpoonleft',
    'space.en': '\\;',
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
        "// Static symbol mappings derived from tex2typst and project-specific additions.",
        "// tools/gen_maps.py refuses lossy overwrites; update both sources when it becomes",
        "// complete enough to regenerate this file.",
        "",
        "use mitex_spec::{CommandSpec, CommandSpecItem, CmdShape, ArgShape, ArgPattern, GlobStr};",
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

    # `\\item` has no required argument, but its optional `[...]` label must be
    # parsed for description lists. This cannot be represented by the fixed-arity
    # COMMANDS_WITH_ARGS table.
    lines.append('        m.insert("item".to_string(), CommandSpecItem::Cmd(CmdShape {')
    lines.append('            args: ArgShape::Right { pattern: ArgPattern::Glob { pattern: GlobStr::from("{,b}") } },')
    lines.append('            alias: None,')
    lines.append('        }));')

    # These take an optional `[..]` before the required argument, so a fixed
    # arity would leave the `[..]` unconsumed: the parser then binds no argument
    # at all and the whole construct degrades to body text.
    for cmd in OPTIONAL_ARG_COMMANDS:
        lines.append(f'        m.insert("{cmd}".to_string(), CommandSpecItem::Cmd(CmdShape {{')
        lines.append('            args: ArgShape::Right { pattern: ArgPattern::Glob { pattern: GlobStr::from("{,b}t") } },')
        lines.append('            alias: None,')
        lines.append('        }));')

    # Commands with an optional argument followed by more than one required one
    for cmd, pattern in sorted(GLOB_ARG_COMMANDS.items()):
        cmd_esc = escape_rust_string(cmd)
        lines.append(f'        m.insert("{cmd_esc}".to_string(), CommandSpecItem::Cmd(CmdShape {{')
        lines.append(f'            args: ArgShape::Right {{ pattern: ArgPattern::Glob {{ pattern: GlobStr::from("{pattern}") }} }},')
        lines.append('            alias: None,')
        lines.append('        }));')

    # Zero-argument commands the parser must know but that have no alias
    for cmd in sorted(BARE_COMMANDS):
        cmd_esc = escape_rust_string(cmd)
        lines.append(f'        m.insert("{cmd_esc}".to_string(), CommandSpecItem::Cmd(CmdShape {{')
        lines.append('            args: ArgShape::Right { pattern: ArgPattern::None },')
        lines.append('            alias: None,')
        lines.append('        }));')
    
    # Add aligned environment
    lines.append('        m.insert("aligned".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {')
    lines.append('            args: ArgPattern::None,')
    lines.append('            ctx_feature: mitex_spec::ContextFeature::None,')
    lines.append('            alias: None,')
    lines.append('        }));')

    # Environment header signatures (see ENVIRONMENT_SIGNATURES).
    for env, pattern in ENVIRONMENT_SIGNATURES.items():
        env_esc = escape_rust_string(env)
        lines.append(f'        m.insert("{env_esc}".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {{')
        lines.append(f'            args: ArgPattern::Glob {{ pattern: GlobStr::from("{pattern}") }},')
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
        "",
        "// =============================================================================",
        "// DELIMITER_MAP: Single source of truth for lr() delimiter conversion",
        "// Maps Typst delimiter representations to LaTeX output",
        "// =============================================================================",
        "",
        "/// Map from Typst delimiter names/chars to LaTeX",
        "/// Key: Typst representation (symbol name or Unicode char)",
        "/// Value: LaTeX output string",
        "pub static DELIMITER_MAP: phf::Map<&'static str, &'static str> = phf_map! {",
    ])

    for typst, latex in sorted(DELIMITER_MAP.items()):
        if not typst or not latex:
            continue
        lines.append(f'    "{escape_rust_string(typst)}" => "{escape_rust_string(latex)}",')

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


def extract_public_map_names(rust_code):
    """Return public static map names declared by a generated Rust source."""
    return set(re.findall(r"pub static(?: ref)? ([A-Z][A-Z0-9_]*):", rust_code))


def extract_mapping_keys(rust_code):
    """Return command and PHF keys from the generated Rust source.

    This intentionally uses the output's simple generated forms. It is a
    safety check, not a Rust parser: a false positive merely stops a lossy
    overwrite, which is safer than silently deleting a conversion mapping.
    """
    command_keys = set(re.findall(r'm\.insert\("([^"]+)"\.to_string\(\)', rust_code))
    phf_keys = set(re.findall(r'^\s*"([^"]+)"\s*=>', rust_code, re.MULTILINE))
    return command_keys | phf_keys


def ensure_lossless_regeneration(output_path, rust_code):
    """Reject a generated file that would discard checked-in map coverage."""
    if not output_path.exists():
        return

    existing = output_path.read_text(encoding="utf-8")
    missing_maps = extract_public_map_names(existing) - extract_public_map_names(rust_code)
    missing_keys = extract_mapping_keys(existing) - extract_mapping_keys(rust_code)
    if not missing_maps and not missing_keys:
        return

    details = []
    if missing_maps:
        details.append("public maps: " + ", ".join(sorted(missing_maps)))
    if missing_keys:
        sample = ", ".join(sorted(missing_keys)[:10])
        details.append(f"{len(missing_keys)} mapping keys (for example: {sample})")
    raise RuntimeError(
        "refusing a lossy maps.rs overwrite; synchronize tools/gen_maps.py "
        "with src/data/maps.rs first (missing " + "; ".join(details) + ")"
    )

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
    try:
        ensure_lossless_regeneration(output_path, rust_code)
    except RuntimeError as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1

    with open(output_path, 'w', encoding='utf-8') as f:
        f.write(rust_code)
    
    print(f"Generated: {output_path}")
    return 0

if __name__ == "__main__":
    sys.exit(main())
