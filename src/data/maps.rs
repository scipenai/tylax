// Static symbol mappings derived from tex2typst and project-specific additions.
// Regenerate with `python tools/gen_maps.py`, which refuses an overwrite that
// would drop or alter any mapping. Keep edits here and in the generator in step.

use fxhash::FxHashMap;
use lazy_static::lazy_static;
use mitex_spec::{ArgPattern, ArgShape, CmdShape, CommandSpec, CommandSpecItem, GlobStr};
use phf::phf_map;

// =============================================================================
// TEX_COMMAND_SPEC: Runtime-constructed CommandSpec for mitex parser
// Uses lazy_static because CommandSpec requires runtime construction
// =============================================================================

lazy_static! {
    /// LaTeX command specification for Mitex
    pub static ref TEX_COMMAND_SPEC: CommandSpec = {
        let mut m = FxHashMap::default();
        // Helper closures for conciseness: these shapes repeat
        // hundreds of times, and the guards in tests/integration_tests.rs
        // expand their names when comparing this file with the generator.
        let cmd1 = || CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::FixedLenTerm { len: 1 } },
            alias: None,
        });
        let cmd2 = || CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::FixedLenTerm { len: 2 } },
            alias: None,
        });
        let cmd3 = || CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::FixedLenTerm { len: 3 } },
            alias: None,
        });
        let cmd1_opt = || CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right {
                pattern: ArgPattern::Glob {
                    pattern: GlobStr::from("{,b}t"),
                },
            },
            alias: None,
        });
        let cmd2_opt = || CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right {
                pattern: ArgPattern::Glob {
                    pattern: GlobStr::from("{,b}tt"),
                },
            },
            alias: None,
        });
        let cmd3_opt = || CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right {
                pattern: ArgPattern::Glob {
                    pattern: GlobStr::from("{,b}ttt"),
                },
            },
            alias: None,
        });

        m.insert(" ".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("med".to_string()),
        }));
        m.insert(",".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("thin".to_string()),
        }));
        m.insert(":".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("med".to_string()),
        }));
        m.insert(";".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("thick".to_string()),
        }));
        m.insert(">".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("med".to_string()),
        }));
        m.insert("Delta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Delta".to_string()),
        }));
        m.insert("Downarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.b.double".to_string()),
        }));
        m.insert("Gamma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Gamma".to_string()),
        }));
        m.insert("Im".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Im".to_string()),
        }));
        m.insert("Lambda".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Lambda".to_string()),
        }));
        m.insert("Leftarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.double".to_string()),
        }));
        m.insert("Leftrightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.r.double".to_string()),
        }));
        m.insert("Longleftarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.double.long".to_string()),
        }));
        m.insert("Longleftrightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.r.double.long".to_string()),
        }));
        m.insert("Longrightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r.double.long".to_string()),
        }));
        m.insert("Omega".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Omega".to_string()),
        }));
        m.insert("Phi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Phi".to_string()),
        }));
        m.insert("Pi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Pi".to_string()),
        }));
        m.insert("Pr".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Pr".to_string()),
        }));
        m.insert("Psi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Psi".to_string()),
        }));
        m.insert("Re".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Re".to_string()),
        }));
        m.insert("Rightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r.double".to_string()),
        }));
        m.insert("Sigma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Sigma".to_string()),
        }));
        m.insert("Theta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Theta".to_string()),
        }));
        m.insert("Uparrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.t.double".to_string()),
        }));
        m.insert("Updownarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.t.b.double".to_string()),
        }));
        m.insert("Upsilon".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Upsilon".to_string()),
        }));
        m.insert("Xi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Xi".to_string()),
        }));
        m.insert("aleph".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("aleph".to_string()),
        }));
        m.insert("alpha".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("alpha".to_string()),
        }));
        m.insert("amalg".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("product.co".to_string()),
        }));
        m.insert("angle".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("angle".to_string()),
        }));
        m.insert("approx".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("approx".to_string()),
        }));
        m.insert("arccos".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arccos".to_string()),
        }));
        m.insert("arcsin".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arcsin".to_string()),
        }));
        m.insert("arctan".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arctan".to_string()),
        }));
        m.insert("arg".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arg".to_string()),
        }));
        m.insert("ast".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("ast".to_string()),
        }));
        m.insert("asymp".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("asymp".to_string()),
        }));
        m.insert("backslash".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("backslash".to_string()),
        }));
        m.insert("beta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("beta".to_string()),
        }));
        m.insert("beth".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("beth".to_string()),
        }));
        m.insert("bigcap".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("inter.big".to_string()),
        }));
        m.insert("bigcup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("union.big".to_string()),
        }));
        m.insert("bigodot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dot.o.big".to_string()),
        }));
        m.insert("bigoplus".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("plus.o.big".to_string()),
        }));
        m.insert("bigotimes".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("times.o.big".to_string()),
        }));
        m.insert("bigsqcup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("union.sq.big".to_string()),
        }));
        m.insert("bigtriangledown".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.b".to_string()),
        }));
        m.insert("bigtriangleup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.t".to_string()),
        }));
        m.insert("bigvee".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("or.big".to_string()),
        }));
        m.insert("bigwedge".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("and.big".to_string()),
        }));
        m.insert("blacklozenge".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lozenge.filled".to_string()),
        }));
        m.insert("blacksquare".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("square.filled".to_string()),
        }));
        m.insert("blacktriangleleft".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.filled.l".to_string()),
        }));
        m.insert("blacktriangleright".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.filled.r".to_string()),
        }));
        m.insert("bot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("bot".to_string()),
        }));
        m.insert("bullet".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("bullet".to_string()),
        }));
        m.insert("cal".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("cal".to_string()),
        }));
        m.insert("cap".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("inter".to_string()),
        }));
        m.insert("cdot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dot.op".to_string()),
        }));
        m.insert("cdots".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots.c".to_string()),
        }));
        m.insert("chi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("chi".to_string()),
        }));
        m.insert("circ".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("circle.small".to_string()),
        }));
        m.insert("clubsuit".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("suit.club".to_string()),
        }));
        m.insert("cong".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tilde.equiv".to_string()),
        }));
        m.insert("coprod".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("product.co".to_string()),
        }));
        m.insert("cos".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("cos".to_string()),
        }));
        m.insert("cosh".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("cosh".to_string()),
        }));
        m.insert("cot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("cot".to_string()),
        }));
        m.insert("coth".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("coth".to_string()),
        }));
        m.insert("csc".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("csc".to_string()),
        }));
        m.insert("cup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("union".to_string()),
        }));
        m.insert("dagger".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dagger".to_string()),
        }));
        m.insert("dashv".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tack.l".to_string()),
        }));
        m.insert("ddagger".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dagger.double".to_string()),
        }));
        m.insert("dddot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dot.triple".to_string()),
        }));
        m.insert("ddots".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots.down".to_string()),
        }));
        m.insert("deg".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("deg".to_string()),
        }));
        m.insert("delta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("delta".to_string()),
        }));
        m.insert("det".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("det".to_string()),
        }));
        m.insert("diamond".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("diamond".to_string()),
        }));
        m.insert("diamondsuit".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("suit.diamond".to_string()),
        }));
        m.insert("dim".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dim".to_string()),
        }));
        m.insert("displaystyle".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("display".to_string()),
        }));
        m.insert("div".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("div".to_string()),
        }));
        m.insert("doteq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("eq.dot".to_string()),
        }));
        m.insert("dots".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots".to_string()),
        }));
        m.insert("dotsb".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots.c".to_string()),
        }));
        m.insert("dotsc".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots.c".to_string()),
        }));
        m.insert("dotsm".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots.c".to_string()),
        }));
        m.insert("downarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.b".to_string()),
        }));
        m.insert("ell".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("ell".to_string()),
        }));
        m.insert("emptyset".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("emptyset".to_string()),
        }));
        m.insert("epsilon".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("epsilon.alt".to_string()),
        }));
        m.insert("equiv".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("equiv".to_string()),
        }));
        m.insert("eta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("eta".to_string()),
        }));
        m.insert("exists".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("exists".to_string()),
        }));
        m.insert("exp".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("exp".to_string()),
        }));
        m.insert("flat".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("flat".to_string()),
        }));
        m.insert("forall".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("forall".to_string()),
        }));
        m.insert("frown".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("frown".to_string()),
        }));
        m.insert("gamma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("gamma".to_string()),
        }));
        m.insert("gcd".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("gcd".to_string()),
        }));
        m.insert("ge".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("gt.eq".to_string()),
        }));
        m.insert("geq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("gt.eq".to_string()),
        }));
        m.insert("gg".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("gt.double".to_string()),
        }));
        m.insert("gimel".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("gimel".to_string()),
        }));
        m.insert("hbar".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("planck.reduce".to_string()),
        }));
        m.insert("heartsuit".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("suit.heart".to_string()),
        }));
        m.insert("hom".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("hom".to_string()),
        }));
        m.insert("hookleftarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.hook".to_string()),
        }));
        m.insert("hookrightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r.hook".to_string()),
        }));
        m.insert("iff".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.r.double.long".to_string()),
        }));
        m.insert("iiiint".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("integral.quad".to_string()),
        }));
        m.insert("iiint".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("integral.triple".to_string()),
        }));
        m.insert("iint".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("integral.double".to_string()),
        }));
        m.insert("imath".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dotless.i".to_string()),
        }));
        m.insert("in".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("in".to_string()),
        }));
        m.insert("inf".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("inf".to_string()),
        }));
        m.insert("infty".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("infinity".to_string()),
        }));
        m.insert("int".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("integral".to_string()),
        }));
        m.insert("iota".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("iota".to_string()),
        }));
        m.insert("jmath".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dotless.j".to_string()),
        }));
        m.insert("kappa".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("kappa".to_string()),
        }));
        m.insert("ker".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("ker".to_string()),
        }));
        m.insert("lVert".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("bar.v.double".to_string()),
        }));
        m.insert("lambda".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lambda".to_string()),
        }));
        m.insert("langle".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("chevron.l".to_string()),
        }));
        m.insert("lbrace".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("brace.l".to_string()),
        }));
        m.insert("lceil".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("ceil.l".to_string()),
        }));
        m.insert("ldots".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots.h".to_string()),
        }));
        m.insert("le".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lt.eq".to_string()),
        }));
        m.insert("leadsto".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r.squiggly".to_string()),
        }));
        m.insert("leftarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l".to_string()),
        }));
        m.insert("leftharpoondown".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("harpoon.lb".to_string()),
        }));
        m.insert("leftharpoonup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("harpoon.lt".to_string()),
        }));
        m.insert("leftrightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.r".to_string()),
        }));
        m.insert("leq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lt.eq".to_string()),
        }));
        m.insert("lfloor".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("floor.l".to_string()),
        }));
        m.insert("lg".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lg".to_string()),
        }));
        m.insert("lhd".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.l".to_string()),
        }));
        m.insert("lim".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lim".to_string()),
        }));
        m.insert("liminf".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("liminf".to_string()),
        }));
        m.insert("limsup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("limsup".to_string()),
        }));
        m.insert("ll".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lt.double".to_string()),
        }));
        m.insert("ln".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("ln".to_string()),
        }));
        m.insert("lnot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("not".to_string()),
        }));
        m.insert("log".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("log".to_string()),
        }));
        m.insert("longleftarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.long".to_string()),
        }));
        m.insert("longleftrightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.l.r.long".to_string()),
        }));
        m.insert("longmapsto".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r.long.bar".to_string()),
        }));
        m.insert("longrightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r.long".to_string()),
        }));
        m.insert("lozenge".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("lozenge".to_string()),
        }));
        m.insert("lvert".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("bar.v".to_string()),
        }));
        m.insert("mapsto".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r.bar".to_string()),
        }));
        m.insert("max".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("max".to_string()),
        }));
        m.insert("mid".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("divides".to_string()),
        }));
        m.insert("min".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("min".to_string()),
        }));
        m.insert("models".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("models".to_string()),
        }));
        m.insert("mp".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("minus.plus".to_string()),
        }));
        m.insert("mu".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("mu".to_string()),
        }));
        m.insert("nabla".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("nabla".to_string()),
        }));
        m.insert("natural".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("natural".to_string()),
        }));
        m.insert("ne".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("eq.not".to_string()),
        }));
        m.insert("nearrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.tr".to_string()),
        }));
        m.insert("neg".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("not".to_string()),
        }));
        m.insert("neq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("eq.not".to_string()),
        }));
        m.insert("nexists".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("exists.not".to_string()),
        }));
        m.insert("ni".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("in.rev".to_string()),
        }));
        m.insert("notin".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("in.not".to_string()),
        }));
        m.insert("nu".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("nu".to_string()),
        }));
        m.insert("nwarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.tl".to_string()),
        }));
        m.insert("odot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dot.o".to_string()),
        }));
        m.insert("oiiint".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("integral.vol".to_string()),
        }));
        m.insert("oiint".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("integral.surf".to_string()),
        }));
        m.insert("oint".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("integral.cont".to_string()),
        }));
        m.insert("omega".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("omega".to_string()),
        }));
        m.insert("ominus".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("minus.o".to_string()),
        }));
        m.insert("oplus".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("plus.o".to_string()),
        }));
        m.insert("oslash".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("slash.o".to_string()),
        }));
        m.insert("otimes".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("times.o".to_string()),
        }));
        m.insert("parallel".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("parallel".to_string()),
        }));
        m.insert("partial".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("diff".to_string()),
        }));
        m.insert("perp".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("perp".to_string()),
        }));
        m.insert("phi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("phi.alt".to_string()),
        }));
        m.insert("pi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("pi".to_string()),
        }));
        m.insert("pm".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("plus.minus".to_string()),
        }));
        m.insert("prec".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("prec".to_string()),
        }));
        m.insert("preceq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("prec.eq".to_string()),
        }));
        m.insert("prime".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("prime".to_string()),
        }));
        m.insert("prod".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("product".to_string()),
        }));
        m.insert("propto".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("prop".to_string()),
        }));
        m.insert("psi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("psi".to_string()),
        }));
        m.insert("rVert".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("bar.v.double".to_string()),
        }));
        m.insert("rangle".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("chevron.r".to_string()),
        }));
        m.insert("rbrace".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("brace.r".to_string()),
        }));
        m.insert("rceil".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("ceil.r".to_string()),
        }));
        m.insert("rfloor".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("floor.r".to_string()),
        }));
        m.insert("rhd".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.r".to_string()),
        }));
        m.insert("rho".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("rho".to_string()),
        }));
        m.insert("rightarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r".to_string()),
        }));
        m.insert("rightharpoondown".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("harpoon.rb".to_string()),
        }));
        m.insert("rightharpoonup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("harpoon.rt".to_string()),
        }));
        m.insert("rvert".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("bar.v".to_string()),
        }));
        m.insert("searrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.br".to_string()),
        }));
        m.insert("sec".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sec".to_string()),
        }));
        m.insert("setminus".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("without".to_string()),
        }));
        m.insert("sharp".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sharp".to_string()),
        }));
        m.insert("sigma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sigma".to_string()),
        }));
        m.insert("sim".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tilde.op".to_string()),
        }));
        m.insert("simeq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tilde.eq".to_string()),
        }));
        m.insert("sin".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sin".to_string()),
        }));
        m.insert("sinh".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sinh".to_string()),
        }));
        m.insert("smile".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("smile".to_string()),
        }));
        m.insert("spadesuit".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("suit.spade".to_string()),
        }));
        m.insert("sqcap".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sect.sq".to_string()),
        }));
        m.insert("sqcup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("union.sq".to_string()),
        }));
        m.insert("sqsubset".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("subset.sq".to_string()),
        }));
        m.insert("sqsubseteq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("subset.sq.eq".to_string()),
        }));
        m.insert("sqsupset".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("supset.sq".to_string()),
        }));
        m.insert("sqsupseteq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("supset.sq.eq".to_string()),
        }));
        m.insert("square".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("square".to_string()),
        }));
        m.insert("star".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("star".to_string()),
        }));
        m.insert("subset".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("subset".to_string()),
        }));
        m.insert("subseteq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("subset.eq".to_string()),
        }));
        m.insert("succ".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("succ".to_string()),
        }));
        m.insert("succeq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("succ.eq".to_string()),
        }));
        m.insert("sum".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sum".to_string()),
        }));
        m.insert("sup".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sup".to_string()),
        }));
        m.insert("supset".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("supset".to_string()),
        }));
        m.insert("supseteq".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("supset.eq".to_string()),
        }));
        m.insert("surd".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sqrt".to_string()),
        }));
        m.insert("swarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.bl".to_string()),
        }));
        m.insert("tan".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tan".to_string()),
        }));
        m.insert("tanh".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tanh".to_string()),
        }));
        m.insert("tau".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tau".to_string()),
        }));
        m.insert("textstyle".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("inline".to_string()),
        }));
        m.insert("theta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("theta".to_string()),
        }));
        m.insert("times".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("times".to_string()),
        }));
        m.insert("to".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.r".to_string()),
        }));
        m.insert("top".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("top".to_string()),
        }));
        m.insert("triangle".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.t".to_string()),
        }));
        m.insert("triangleleft".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.l".to_string()),
        }));
        m.insert("triangleright".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.r".to_string()),
        }));
        m.insert("unlhd".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.l.eq".to_string()),
        }));
        m.insert("unrhd".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("triangle.r.eq".to_string()),
        }));
        m.insert("uparrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.t".to_string()),
        }));
        m.insert("updownarrow".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("arrow.t.b".to_string()),
        }));
        m.insert("upsilon".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("upsilon".to_string()),
        }));
        m.insert("varDelta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Delta".to_string()),
        }));
        m.insert("varGamma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Gamma".to_string()),
        }));
        m.insert("varLambda".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Lambda".to_string()),
        }));
        m.insert("varOmega".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Omega".to_string()),
        }));
        m.insert("varPhi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Phi".to_string()),
        }));
        m.insert("varPi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Pi".to_string()),
        }));
        m.insert("varPsi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Psi".to_string()),
        }));
        m.insert("varSigma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Sigma".to_string()),
        }));
        m.insert("varTheta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Theta".to_string()),
        }));
        m.insert("varUpsilon".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Upsilon".to_string()),
        }));
        m.insert("varXi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("Xi".to_string()),
        }));
        m.insert("varepsilon".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("epsilon".to_string()),
        }));
        m.insert("varnothing".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("nothing".to_string()),
        }));
        m.insert("varphi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("phi".to_string()),
        }));
        m.insert("varpi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("pi.alt".to_string()),
        }));
        m.insert("varrho".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("rho.alt".to_string()),
        }));
        m.insert("varsigma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("sigma.alt".to_string()),
        }));
        m.insert("vartheta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("theta.alt".to_string()),
        }));
        m.insert("vdash".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("tack.r".to_string()),
        }));
        m.insert("vdots".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("dots.v".to_string()),
        }));
        m.insert("vee".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("or".to_string()),
        }));
        m.insert("wedge".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("and".to_string()),
        }));
        m.insert("wp".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("weierstrass".to_string()),
        }));
        m.insert("wr".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("wreath".to_string()),
        }));
        m.insert("xi".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("xi".to_string()),
        }));
        m.insert("zeta".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("zeta".to_string()),
        }));
        m.insert("|".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("bar.v.double".to_string()),
        }));
        m.insert("~".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: Some("space.nobreak".to_string()),
        }));
        m.insert("typstcite".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::FixedLenTerm { len: 1 } },
            alias: Some("__typstcite__".to_string()),
        }));
        m.insert("item".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::Glob { pattern: GlobStr::from("{,b}") } },
            alias: None,
        }));
        m.insert("part".to_string(), cmd1_opt());
        m.insert("chapter".to_string(), cmd1_opt());
        m.insert("section".to_string(), cmd1_opt());
        m.insert("subsection".to_string(), cmd1_opt());
        m.insert("subsubsection".to_string(), cmd1_opt());
        m.insert("paragraph".to_string(), cmd1_opt());
        m.insert("subparagraph".to_string(), cmd1_opt());
        m.insert("caption".to_string(), cmd1_opt());
        m.insert("sqrt".to_string(), cmd1_opt());
        m.insert("dd".to_string(), cmd1_opt());
        m.insert("differential".to_string(), cmd1_opt());
        m.insert("hyperref".to_string(), cmd1_opt());
        m.insert("cp".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("cross".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("crossproduct".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("divisionsymbol".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("dotproduct".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("injlim".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("projlim".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qall".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qand".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qas".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qassume".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qc".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qcc".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qcomma".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qelse".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qeven".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qfor".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qgiven".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qif".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qin".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qinteger".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qlet".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qodd".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qor".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qotherwise".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qsince".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qthen".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qunless".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("qusing".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("varinjlim".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("varprojlim".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("vdot".to_string(), CommandSpecItem::Cmd(CmdShape {
            args: ArgShape::Right { pattern: ArgPattern::None },
            alias: None,
        }));
        m.insert("aligned".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::None,
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("figure".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("figure*".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("table".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("table*".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("wrapfigure".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("enumerate".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("itemize".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("description".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("list".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("adjustbox".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("tcolorbox".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("algorithm".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("algorithmic".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("lstlisting".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("minipage".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}{,b}{,b}t") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("multicols".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("t{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("multicols*".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("t{,b}") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("tabular".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}t") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("longtable".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}t") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("longtabu".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}t") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("array".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("{,b}t") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("tabular*".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("t{,b}t") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));
        m.insert("tabularx".to_string(), CommandSpecItem::Env(mitex_spec::EnvShape {
            args: ArgPattern::Glob { pattern: GlobStr::from("t{,b}t") },
            ctx_feature: mitex_spec::ContextFeature::None,
            alias: None,
        }));

        // Commands with required arguments
        m.insert("Acf".to_string(), cmd1());
        m.insert("Acl".to_string(), cmd1());
        m.insert("Acs".to_string(), cmd1());
        m.insert("Bqty".to_string(), cmd1());
        m.insert("Cref".to_string(), cmd1());
        m.insert("DeclareMathOperator".to_string(), cmd2());
        m.insert("Gls".to_string(), cmd1());
        m.insert("PV".to_string(), cmd1());
        m.insert("Pmqty".to_string(), cmd1());
        m.insert("Res".to_string(), cmd1());
        m.insert("Residue".to_string(), cmd1());
        m.insert("Set".to_string(), cmd1());
        m.insert("abs".to_string(), cmd1());
        m.insert("abs*".to_string(), cmd1());
        m.insert("absolutevalue".to_string(), cmd1());
        m.insert("ac".to_string(), cmd1());
        m.insert("acomm".to_string(), cmd2());
        m.insert("acomm*".to_string(), cmd2());
        m.insert("acommutator".to_string(), cmd2());
        m.insert("acrfull".to_string(), cmd1());
        m.insert("acrlong".to_string(), cmd1());
        m.insert("acrshort".to_string(), cmd1());
        m.insert("acute".to_string(), cmd1());
        m.insert("admat".to_string(), cmd1());
        m.insert("anticommutator".to_string(), cmd2());
        m.insert("antidiagonalmatrix".to_string(), cmd1());
        m.insert("author".to_string(), cmd1());
        m.insert("autocite".to_string(), cmd1());
        m.insert("autoref".to_string(), cmd1());
        m.insert("bar".to_string(), cmd1());
        m.insert("bcancel".to_string(), cmd1());
        m.insert("bibitem".to_string(), cmd1());
        m.insert("binom".to_string(), cmd2());
        m.insert("bm".to_string(), cmd1());
        m.insert("bmqty".to_string(), cmd1());
        m.insert("boldsymbol".to_string(), cmd1());
        m.insert("boxed".to_string(), cmd1());
        m.insert("bqty".to_string(), cmd1());
        m.insert("bra".to_string(), cmd1());
        m.insert("bra*".to_string(), cmd1());
        m.insert("braket".to_string(), cmd2());
        m.insert("braket*".to_string(), cmd2());
        m.insert("breve".to_string(), cmd1());
        m.insert("cancel".to_string(), cmd1());
        m.insert("cfrac".to_string(), cmd2());
        m.insert("check".to_string(), cmd1());
        m.insert("cite".to_string(), cmd1());
        m.insert("citep".to_string(), cmd1());
        m.insert("citet".to_string(), cmd1());
        m.insert("color".to_string(), cmd1());
        m.insert("colorbox".to_string(), cmd2());
        m.insert("comm".to_string(), cmd2());
        m.insert("comm*".to_string(), cmd2());
        m.insert("commutator".to_string(), cmd2());
        m.insert("cref".to_string(), cmd1());
        m.insert("curl".to_string(), cmd1());
        m.insert("date".to_string(), cmd1());
        m.insert("ddot".to_string(), cmd1());
        m.insert("dfrac".to_string(), cmd2());
        m.insert("diagonalmatrix".to_string(), cmd1());
        m.insert("displaylines".to_string(), cmd1());
        m.insert("divergence".to_string(), cmd1());
        m.insert("dmat".to_string(), cmd1());
        m.insert("dot".to_string(), cmd1());
        m.insert("dyad".to_string(), cmd2());
        m.insert("dyad*".to_string(), cmd2());
        m.insert("emph".to_string(), cmd1());
        m.insert("eqref".to_string(), cmd1());
        m.insert("ev".to_string(), cmd2());
        m.insert("ev*".to_string(), cmd2());
        m.insert("eval".to_string(), cmd1());
        m.insert("eval*".to_string(), cmd1());
        m.insert("evaluated".to_string(), cmd1());
        m.insert("expectationvalue".to_string(), cmd2());
        m.insert("expval".to_string(), cmd2());
        m.insert("expval*".to_string(), cmd2());
        m.insert("fbox".to_string(), cmd1());
        m.insert("fcolorbox".to_string(), cmd3());
        m.insert("flatfrac".to_string(), cmd2());
        m.insert("footcite".to_string(), cmd1());
        m.insert("footnote".to_string(), cmd1());
        m.insert("frac".to_string(), cmd2());
        m.insert("gls".to_string(), cmd1());
        m.insert("grad".to_string(), cmd1());
        m.insert("gradient".to_string(), cmd1());
        m.insert("grave".to_string(), cmd1());
        m.insert("hat".to_string(), cmd1());
        m.insert("highlight".to_string(), cmd1());
        m.insert("hl".to_string(), cmd1());
        m.insert("href".to_string(), cmd2());
        m.insert("hspace".to_string(), cmd1());
        m.insert("hspace*".to_string(), cmd1());
        m.insert("identitymatrix".to_string(), cmd1());
        m.insert("imat".to_string(), cmd1());
        m.insert("innerproduct".to_string(), cmd2());
        m.insert("ip".to_string(), cmd2());
        m.insert("ket".to_string(), cmd1());
        m.insert("ket*".to_string(), cmd1());
        m.insert("ketbra".to_string(), cmd2());
        m.insert("label".to_string(), cmd1());
        m.insert("laplacian".to_string(), cmd1());
        m.insert("mathbb".to_string(), cmd1());
        m.insert("mathbf".to_string(), cmd1());
        m.insert("mathbin".to_string(), cmd1());
        m.insert("mathcal".to_string(), cmd1());
        m.insert("mathclose".to_string(), cmd1());
        m.insert("mathfrak".to_string(), cmd1());
        m.insert("mathinner".to_string(), cmd1());
        m.insert("mathit".to_string(), cmd1());
        m.insert("mathop".to_string(), cmd1());
        m.insert("mathopen".to_string(), cmd1());
        m.insert("mathord".to_string(), cmd1());
        m.insert("mathpunct".to_string(), cmd1());
        m.insert("mathrel".to_string(), cmd1());
        m.insert("mathrm".to_string(), cmd1());
        m.insert("mathsf".to_string(), cmd1());
        m.insert("mathtt".to_string(), cmd1());
        m.insert("matrixdeterminant".to_string(), cmd1());
        m.insert("matrixel".to_string(), cmd3());
        m.insert("matrixelement".to_string(), cmd3());
        m.insert("matrixquantity".to_string(), cmd1());
        m.insert("mdet".to_string(), cmd1());
        m.insert("mel".to_string(), cmd3());
        m.insert("mel*".to_string(), cmd3());
        m.insert("mqty".to_string(), cmd1());
        m.insert("multicolumn".to_string(), cmd3());
        m.insert("multirow".to_string(), cmd3());
        m.insert("newacronym".to_string(), cmd3());
        m.insert("newcommand".to_string(), cmd2());
        m.insert("newglossaryentry".to_string(), cmd2());
        m.insert("norm".to_string(), cmd1());
        m.insert("norm*".to_string(), cmd1());
        m.insert("not".to_string(), cmd1());
        m.insert("op".to_string(), cmd2());
        m.insert("order".to_string(), cmd1());
        m.insert("order*".to_string(), cmd1());
        m.insert("outerproduct".to_string(), cmd2());
        m.insert("overbrace".to_string(), cmd1());
        m.insert("overleftarrow".to_string(), cmd1());
        m.insert("overleftrightarrow".to_string(), cmd1());
        m.insert("overline".to_string(), cmd1());
        m.insert("overrightarrow".to_string(), cmd1());
        m.insert("overset".to_string(), cmd2());
        m.insert("pageref".to_string(), cmd1());
        m.insert("parencite".to_string(), cmd1());
        m.insert("paulimatrix".to_string(), cmd1());
        m.insert("pb".to_string(), cmd2());
        m.insert("pb*".to_string(), cmd2());
        m.insert("phantom".to_string(), cmd1());
        m.insert("pmat".to_string(), cmd1());
        m.insert("pmod".to_string(), cmd1());
        m.insert("pmqty".to_string(), cmd1());
        m.insert("pod".to_string(), cmd1());
        m.insert("poissonbracket".to_string(), cmd2());
        m.insert("pqty".to_string(), cmd1());
        m.insert("principalvalue".to_string(), cmd1());
        m.insert("providecommand".to_string(), cmd2());
        m.insert("pv".to_string(), cmd1());
        m.insert("qq".to_string(), cmd1());
        m.insert("qqtext".to_string(), cmd1());
        m.insert("ref".to_string(), cmd1());
        m.insert("renewcommand".to_string(), cmd2());
        m.insert("sPmqty".to_string(), cmd1());
        m.insert("sbmqty".to_string(), cmd1());
        m.insert("set".to_string(), cmd1());
        m.insert("smallmatrixdeterminant".to_string(), cmd1());
        m.insert("smallmatrixquantity".to_string(), cmd1());
        m.insert("smdet".to_string(), cmd1());
        m.insert("smqty".to_string(), cmd1());
        m.insert("spmqty".to_string(), cmd1());
        m.insert("stackrel".to_string(), cmd2());
        m.insert("svmqty".to_string(), cmd1());
        m.insert("text".to_string(), cmd1());
        m.insert("textbf".to_string(), cmd1());
        m.insert("textcircled".to_string(), cmd1());
        m.insert("textcite".to_string(), cmd1());
        m.insert("textcolor".to_string(), cmd2());
        m.insert("textit".to_string(), cmd1());
        m.insert("textrm".to_string(), cmd1());
        m.insert("textsc".to_string(), cmd1());
        m.insert("texttt".to_string(), cmd1());
        m.insert("tfrac".to_string(), cmd2());
        m.insert("tilde".to_string(), cmd1());
        m.insert("title".to_string(), cmd1());
        m.insert("underbrace".to_string(), cmd1());
        m.insert("underline".to_string(), cmd1());
        m.insert("underset".to_string(), cmd2());
        m.insert("url".to_string(), cmd1());
        m.insert("va".to_string(), cmd1());
        m.insert("var".to_string(), cmd1());
        m.insert("variation".to_string(), cmd1());
        m.insert("vb".to_string(), cmd1());
        m.insert("vec".to_string(), cmd1());
        m.insert("vectorarrow".to_string(), cmd1());
        m.insert("vectorbold".to_string(), cmd1());
        m.insert("vectorunit".to_string(), cmd1());
        m.insert("vev".to_string(), cmd1());
        m.insert("vmqty".to_string(), cmd1());
        m.insert("vqty".to_string(), cmd1());
        m.insert("vspace".to_string(), cmd1());
        m.insert("vspace*".to_string(), cmd1());
        m.insert("vu".to_string(), cmd1());
        m.insert("widehat".to_string(), cmd1());
        m.insert("widetilde".to_string(), cmd1());
        m.insert("xLeftarrow".to_string(), cmd1());
        m.insert("xLeftrightarrow".to_string(), cmd1());
        m.insert("xRightarrow".to_string(), cmd1());
        m.insert("xhookleftarrow".to_string(), cmd1());
        m.insert("xhookrightarrow".to_string(), cmd1());
        m.insert("xleftarrow".to_string(), cmd1());
        m.insert("xleftharpoondown".to_string(), cmd1());
        m.insert("xleftharpoonup".to_string(), cmd1());
        m.insert("xleftrightarrow".to_string(), cmd1());
        m.insert("xleftrightharpoons".to_string(), cmd1());
        m.insert("xlongequal".to_string(), cmd1());
        m.insert("xmapsto".to_string(), cmd1());
        m.insert("xmat".to_string(), cmd3());
        m.insert("xmatrix".to_string(), cmd3());
        m.insert("xrightarrow".to_string(), cmd1());
        m.insert("xrightharpoondown".to_string(), cmd1());
        m.insert("xrightharpoonup".to_string(), cmd1());
        m.insert("xrightleftharpoons".to_string(), cmd1());
        m.insert("xtofrom".to_string(), cmd1());
        m.insert("xtwoheadleftarrow".to_string(), cmd1());
        m.insert("xtwoheadrightarrow".to_string(), cmd1());
        m.insert("zeromatrix".to_string(), cmd2());
        m.insert("zmat".to_string(), cmd2());
        m.insert("derivative".to_string(), cmd2_opt());
        m.insert("dv".to_string(), cmd2_opt());
        m.insert("dv*".to_string(), cmd2_opt());
        m.insert("fderivative".to_string(), cmd2_opt());
        m.insert("fdv".to_string(), cmd2_opt());
        m.insert("fdv*".to_string(), cmd2_opt());
        m.insert("functionalderivative".to_string(), cmd2_opt());
        m.insert("partialderivative".to_string(), cmd3_opt());
        m.insert("pderivative".to_string(), cmd3_opt());
        m.insert("pdv".to_string(), cmd3_opt());
        m.insert("pdv*".to_string(), cmd3_opt());

        CommandSpec::new(m)
    };
}

// =============================================================================
// TYPST_TO_TEX: Compile-time perfect hash map for Typst -> LaTeX conversion
// Uses phf for O(1) lookup with zero runtime initialization cost
// =============================================================================

/// Typst to LaTeX symbol mapping (compile-time perfect hash)
pub static TYPST_TO_TEX: phf::Map<&'static str, &'static str> = phf_map! {
    "!=" => "neq",
    "-->" => "longrightarrow",
    "->" => "rightarrow",
    "->>" => "twoheadrightarrow",
    "..." => "ldots",
    "::=" => "Coloneqq",
    ":=" => "coloneqq",
    "<-" => "leftarrow",
    "<--" => "longleftarrow",
    "<-->" => "longleftrightarrow",
    "<-<" => "leftarrowtail",
    "<->" => "leftrightarrow",
    "<<" => "ll",
    "<<-" => "twoheadleftarrow",
    "<<<" => "lll",
    "<=" => "leq",
    "<==" => "Longleftarrow",
    "<==>" => "Longleftrightarrow",
    "<=>" => "Leftrightarrow",
    "<~" => "leftsquigarrow",
    "=:" => "eqqcolon",
    "==>" => "Longrightarrow",
    "=>" => "Rightarrow",
    ">->" => "rightarrowtail",
    ">=" => "geq",
    ">>" => "gg",
    ">>>" => "ggg",
    "AA" => "forall",
    "Alpha" => "A",
    "BB" => "\\mathbb{B}",
    "Beta" => "B",
    "CC" => "mathbb{C}",
    "Chi" => "X",
    "DD" => "\\mathbb{D}",
    "Delta" => "Delta",
    "Digamma" => "\\Digamma",
    "EE" => "exists",
    "Epsilon" => "E",
    "Eta" => "H",
    "FF" => "\\mathbb{F}",
    "GG" => "\\mathbb{G}",
    "Gamma" => "Gamma",
    "HH" => "\\mathbb{H}",
    "II" => "\\mathbb{I}",
    "Im" => "Im",
    "Iota" => "I",
    "JJ" => "\\mathbb{J}",
    "KK" => "\\mathbb{K}",
    "Kappa" => "K",
    "LL" => "\\mathbb{L}",
    "Lambda" => "Lambda",
    "MM" => "\\mathbb{M}",
    "Mu" => "M",
    "NN" => "mathbb{N}",
    "Nu" => "N",
    "OO" => "\\mathbb{O}",
    "Omega" => "Omega",
    "Omicron" => "O",
    "PP" => "\\mathbb{P}",
    "Phi" => "Phi",
    "Pi" => "Pi",
    "Pr" => "Pr",
    "Psi" => "Psi",
    "QQ" => "mathbb{Q}",
    "RR" => "mathbb{R}",
    "Re" => "Re",
    "Rho" => "P",
    "SS" => "\\mathbb{S}",
    "Sigma" => "Sigma",
    "TT" => "\\mathbb{T}",
    "Tau" => "T",
    "Theta" => "Theta",
    "UU" => "\\mathbb{U}",
    "Upsilon" => "Upsilon",
    "VV" => "\\mathbb{V}",
    "WW" => "\\mathbb{W}",
    "XX" => "\\mathbb{X}",
    "Xi" => "Xi",
    "YY" => "\\mathbb{Y}",
    "ZZ" => "mathbb{Z}",
    "Zeta" => "Z",
    "acute" => "acute",
    "aleph" => "aleph",
    "alpha" => "alpha",
    "amalg" => "amalg",
    "and" => "wedge",
    "and.big" => "bigwedge",
    "angle" => "angle",
    "angle.arc" => "measuredangle",
    "angle.l" => "langle",
    "angle.l.double" => "\\lAngle",
    "angle.r" => "rangle",
    "angle.r.double" => "\\rAngle",
    "angle.spheric" => "sphericalangle",
    "approx" => "approx",
    "approx.eq" => "\\approxeq",
    "approx.not" => "napprox",
    "arccos" => "arccos",
    "arcsin" => "arcsin",
    "arctan" => "arctan",
    "arg" => "arg",
    "argmax" => "argmax",
    "argmin" => "argmin",
    "arrow.b" => "downarrow",
    "arrow.b.double" => "Downarrow",
    "arrow.ccw" => "\\curvearrowleft",
    "arrow.cw" => "\\curvearrowright",
    "arrow.l" => "leftarrow",
    "arrow.l.bar" => "mapsfrom",
    "arrow.l.double" => "Leftarrow",
    "arrow.l.double.long" => "Longleftarrow",
    "arrow.l.double.not" => "\\nLeftarrow",
    "arrow.l.hook" => "hookleftarrow",
    "arrow.l.long" => "longleftarrow",
    "arrow.l.not" => "\\nleftarrow",
    "arrow.l.r" => "leftrightarrow",
    "arrow.l.r.double" => "Leftrightarrow",
    "arrow.l.r.double.long" => "Longleftrightarrow",
    "arrow.l.r.double.not" => "\\nLeftrightarrow",
    "arrow.l.r.long" => "longleftrightarrow",
    "arrow.l.r.not" => "\\nleftrightarrow",
    "arrow.l.r.wave" => "\\leftrightsquigarrow",
    "arrow.l.squiggly" => "leftsquigarrow",
    "arrow.l.tail" => "leftarrowtail",
    "arrow.l.twohead" => "twoheadleftarrow",
    "arrow.ne" => "nearrow",
    "arrow.nw" => "nwarrow",
    "arrow.r" => "rightarrow",
    "arrow.r.bar" => "mapsto",
    "arrow.r.double" => "Rightarrow",
    "arrow.r.double.long" => "Longrightarrow",
    "arrow.r.double.not" => "\\nRightarrow",
    "arrow.r.hook" => "hookrightarrow",
    "arrow.r.long" => "longrightarrow",
    "arrow.r.long.bar" => "longmapsto",
    "arrow.r.not" => "\\nrightarrow",
    "arrow.r.squiggly" => "rightsquigarrow",
    "arrow.r.tail" => "rightarrowtail",
    "arrow.r.twohead" => "twoheadrightarrow",
    "arrow.se" => "searrow",
    "arrow.sw" => "swarrow",
    "arrow.t" => "uparrow",
    "arrow.t.b" => "updownarrow",
    "arrow.t.b.double" => "Updownarrow",
    "arrow.t.double" => "Uparrow",
    "arrows.ll" => "leftleftarrows",
    "arrows.lr" => "leftrightarrows",
    "arrows.rl" => "rightleftarrows",
    "arrows.rr" => "rightrightarrows",
    "ast" => "ast",
    "ast.op" => "\\ast",
    "asymp" => "asymp",
    "ballot" => "times",
    "bar.v" => "|",
    "bar.v.double" => "\\|",
    "because" => "because",
    "beta" => "beta",
    "beth" => "beth",
    "bowtie" => "bowtie",
    "brace.l" => "\\{",
    "brace.r" => "\\}",
    "bracket.l" => "[",
    "bracket.r" => "]",
    "breve" => "breve",
    "bullet" => "bullet",
    "caron" => "check",
    "ceil.l" => "lceil",
    "ceil.r" => "rceil",
    "checkmark" => "checkmark",
    "chevron.l" => "langle",
    "chevron.r" => "rangle",
    "chi" => "chi",
    "circle" => "circ",
    "circle.div" => "oslash",
    "circle.dot" => "odot",
    "circle.minus" => "ominus",
    "circle.plus" => "oplus",
    "circle.small" => "circ",
    "circle.stroked" => "\\circ",
    "circle.stroked.small" => "\\circ",
    "circle.stroked.tiny" => "mathring",
    "circle.times" => "otimes",
    "colon" => "colon",
    "coloneq" => "coloneqq",
    "comma" => ",",
    "complement" => "complement",
    "coproduct" => "coprod",
    "copyright" => "copyright",
    "cos" => "cos",
    "cosh" => "cosh",
    "cot" => "cot",
    "coth" => "coth",
    "csc" => "csc",
    "dagger" => "dagger",
    "dagger.double" => "ddagger",
    "daleth" => "daleth",
    "deg" => "deg",
    "degree" => "degree",
    "delta" => "delta",
    "det" => "det",
    "diaer" => "ddot",
    "diameter" => "diameter",
    "diamond.op" => "diamond",
    "diamond.stroked" => "\\diamond",
    "diamond.stroked.small" => "\\diamond",
    "dif" => "mathrm{d}",
    "diff" => "partial",
    "digamma" => "\\digamma",
    "dim" => "dim",
    "div" => "div",
    "divides" => "mid",
    "divides.not" => "nmid",
    "dollar" => "\\$",
    "dot" => "cdot",
    "dot.c" => "cdot",
    "dot.circle" => "\\odot",
    "dot.circle.big" => "bigodot",
    "dot.o" => "\\odot",
    "dot.o.big" => "\\bigodot",
    "dot.op" => "cdot",
    "doteq" => "doteq",
    "dotless.i" => "\\imath",
    "dotless.j" => "\\jmath",
    "dots" => "ldots",
    "dots.c" => "cdots",
    "dots.down" => "ddots",
    "dots.h" => "ldots",
    "dots.h.c" => "\\cdots",
    "dots.up" => "iddots",
    "dots.v" => "vdots",
    "ell" => "ell",
    "emptyset" => "emptyset",
    "epsilon" => "varepsilon",
    "epsilon.alt" => "epsilon",
    "eq" => "eq",
    "eq.def" => "overset{\\text{def}}{=}",
    "eq.delta" => "triangleq",
    "eq.not" => "neq",
    "eq.quest" => "stackrel{?}{=}",
    "eq.star" => "stackrel{*}{=}",
    "eqcolon" => "eqqcolon",
    "equiv" => "equiv",
    "equiv.not" => "nequiv",
    "eta" => "eta",
    "euro" => "euro",
    "exists" => "exists",
    "exists.not" => "nexists",
    "exp" => "exp",
    "flat" => "flat",
    "floor.l" => "lfloor",
    "floor.r" => "rfloor",
    "forall" => "forall",
    "forces" => "Vdash",
    "forces.not" => "\\nVdash",
    "frown" => "frown",
    "gamma" => "gamma",
    "gcd" => "gcd",
    "gimel" => "gimel",
    "gradient" => "nabla",
    "grave" => "grave",
    "gt" => "gt",
    "gt.approx" => "\\gtrapprox",
    "gt.double" => "gg",
    "gt.eq" => "geq",
    "gt.eq.not" => "ngeq",
    "gt.eq.slant" => "geqslant",
    "gt.equiv" => "\\geqq",
    "gt.lt" => "\\gtrless",
    "gt.not" => "ngtr",
    "gt.tilde" => "\\gtrsim",
    "gt.tri" => "\\vartriangleright",
    "gt.tri.eq" => "\\trianglerighteq",
    "gt.tri.eq.not" => "\\ntrianglerighteq",
    "gt.tri.not" => "\\ntriangleright",
    "gt.triple" => "ggg",
    "harpoon.lb" => "leftharpoondown",
    "harpoon.lt" => "leftharpoonup",
    "harpoon.rb" => "rightharpoondown",
    "harpoon.rt" => "rightharpoonup",
    "harpoons.ltrb" => "leftrightharpoons",
    "harpoons.rtlb" => "rightleftharpoons",
    "harpoons.rtlt" => "\\upharpoonright\\!\\upharpoonleft",
    "hat" => "hat",
    "hbar" => "hbar",
    "hom" => "hom",
    "hyph" => "-",
    "hyph.minus" => "-",
    "imath" => "imath",
    "in" => "in",
    "in.not" => "notin",
    "in.rev" => "ni",
    "in.rev.not" => "notni",
    "inf" => "inf",
    "infinity" => "infty",
    "integral" => "int",
    "integral.cont" => "oint",
    "integral.double" => "iint",
    "integral.quad" => "iiiint",
    "integral.surf" => "oiint",
    "integral.triple" => "iiint",
    "integral.vol" => "oiiint",
    "inter" => "\\cap",
    "inter.big" => "\\bigcap",
    "inter.double" => "\\Cap",
    "inter.sq" => "\\sqcap",
    "iota" => "iota",
    "jmath" => "jmath",
    "join" => "\\bowtie",
    "kappa" => "kappa",
    "kappa.alt" => "varkappa",
    "ker" => "ker",
    "lambda" => "lambda",
    "laplace" => "Delta",
    "lcm" => "operatorname{lcm}",
    "lg" => "lg",
    "lim" => "lim",
    "liminf" => "liminf",
    "limsup" => "limsup",
    "ln" => "ln",
    "log" => "log",
    "lt" => "lt",
    "lt.approx" => "\\lessapprox",
    "lt.double" => "ll",
    "lt.eq" => "leq",
    "lt.eq.not" => "nleq",
    "lt.eq.slant" => "leqslant",
    "lt.equiv" => "\\leqq",
    "lt.gt" => "\\lessgtr",
    "lt.not" => "nless",
    "lt.tilde" => "\\lesssim",
    "lt.tri" => "\\vartriangleleft",
    "lt.tri.eq" => "\\trianglelefteq",
    "lt.tri.eq.not" => "\\ntrianglelefteq",
    "lt.tri.not" => "\\ntriangleleft",
    "lt.triple" => "lll",
    "macron" => "bar",
    "maltese" => "\\maltese",
    "max" => "max",
    "med" => ":",
    "min" => "min",
    "minus" => "-",
    "minus.circle" => "\\ominus",
    "minus.o" => "\\ominus",
    "minus.plus" => "mp",
    "minus.square" => "\\boxminus",
    "models" => "models",
    "mu" => "mu",
    "nabla" => "nabla",
    "natural" => "natural",
    "not" => "neg",
    "nothing" => "varnothing",
    "nu" => "nu",
    "omega" => "omega",
    "oo" => "infty",
    "or" => "vee",
    "or.big" => "bigvee",
    "paragraph" => "P",
    "parallel" => "parallel",
    "parallel.not" => "nparallel",
    "paren.l" => "(",
    "paren.r" => ")",
    "partial" => "partial",
    "percent" => "\\%",
    "perp" => "perp",
    "phi" => "varphi",
    "phi.alt" => "phi",
    "pi" => "pi",
    "pi.alt" => "varpi",
    "pilcrow" => "\\P",
    "planck" => "hbar",
    "planck.reduce" => "hbar",
    "plus" => "+",
    "plus.circle" => "oplus",
    "plus.circle.big" => "bigoplus",
    "plus.minus" => "pm",
    "plus.o" => "\\oplus",
    "plus.o.big" => "\\bigoplus",
    "plus.square" => "\\boxplus",
    "pound" => "pounds",
    "prec" => "prec",
    "prec.approx" => "\\precapprox",
    "prec.curly.eq" => "\\preccurlyeq",
    "prec.curly.eq.not" => "\\npreccurlyeq",
    "prec.eq" => "preceq",
    "prec.not" => "nprec",
    "prec.tilde" => "\\precsim",
    "prime" => "prime",
    "prime.double" => "\\prime\\prime",
    "product" => "prod",
    "product.co" => "\\coprod",
    "prop" => "propto",
    "psi" => "psi",
    "qed" => "square",
    "registered" => "textregistered",
    "rho" => "rho",
    "rho.alt" => "varrho",
    "sec" => "sec",
    "sect" => "cap",
    "sect.big" => "bigcap",
    "sect.sq" => "sqcap",
    "sect.sq.big" => "bigsqcap",
    "section" => "S",
    "sharp" => "sharp",
    "shell.l" => "\\lgroup",
    "shell.r" => "\\rgroup",
    "sigma" => "sigma",
    "sigma.alt" => "varsigma",
    "sin" => "sin",
    "sinh" => "sinh",
    "slash.o" => "\\oslash",
    "smile" => "smile",
    "space" => "\\ ",
    "space.en" => "\\;",
    "space.med" => "\\:",
    "space.neg" => "\\!",
    "space.nobreak" => "~",
    "space.quad" => "\\quad",
    "space.thick" => "\\;",
    "space.thin" => "\\,",
    "space.wide" => "\\qquad",
    "square" => "square",
    "square.dot" => "boxdot",
    "square.filled" => "blacksquare",
    "square.minus" => "boxminus",
    "square.plus" => "boxplus",
    "square.stroked" => "square",
    "square.times" => "boxtimes",
    "star" => "star",
    "star.filled" => "bigstar",
    "star.op" => "*",
    "subset" => "subset",
    "subset.double" => "\\Subset",
    "subset.eq" => "subseteq",
    "subset.eq.not" => "nsubseteq",
    "subset.eq.sq" => "sqsubseteq",
    "subset.neq" => "\\subsetneq",
    "subset.not" => "nsubset",
    "subset.sq" => "sqsubset",
    "succ" => "succ",
    "succ.approx" => "\\succapprox",
    "succ.curly.eq" => "\\succcurlyeq",
    "succ.curly.eq.not" => "\\nsucccurlyeq",
    "succ.eq" => "succeq",
    "succ.not" => "nsucc",
    "succ.tilde" => "\\succsim",
    "suit.club" => "clubsuit",
    "suit.club.filled" => "\\clubsuit",
    "suit.diamond" => "diamondsuit",
    "suit.diamond.stroked" => "\\diamondsuit",
    "suit.heart" => "heartsuit",
    "suit.heart.stroked" => "\\heartsuit",
    "suit.spade" => "spadesuit",
    "suit.spade.filled" => "\\spadesuit",
    "sum" => "sum",
    "sup" => "sup",
    "supset" => "supset",
    "supset.double" => "\\Supset",
    "supset.eq" => "supseteq",
    "supset.eq.not" => "nsupseteq",
    "supset.eq.sq" => "sqsupseteq",
    "supset.neq" => "\\supsetneq",
    "supset.not" => "nsupset",
    "supset.sq" => "sqsupset",
    "tack.b" => "bot",
    "tack.l" => "dashv",
    "tack.l.double" => "Dashv",
    "tack.r" => "vdash",
    "tack.r.double" => "vDash",
    "tack.r.double.not" => "nvDash",
    "tack.r.not" => "nvdash",
    "tack.t" => "top",
    "tan" => "tan",
    "tanh" => "tanh",
    "tau" => "tau",
    "therefore" => "therefore",
    "theta" => "theta",
    "theta.alt" => "vartheta",
    "thick" => ";",
    "thin" => ",",
    "tilde" => "tilde",
    "tilde.eq" => "simeq",
    "tilde.eq.not" => "nsimeq",
    "tilde.equiv" => "cong",
    "tilde.equiv.not" => "ncong",
    "tilde.not" => "\\nsim",
    "tilde.op" => "sim",
    "tilde.rev" => "\\backsim",
    "tilde.rev.equiv" => "\\backcong",
    "times" => "times",
    "times.circle" => "otimes",
    "times.circle.big" => "bigotimes",
    "times.o" => "\\otimes",
    "times.o.big" => "\\bigotimes",
    "times.square" => "\\boxtimes",
    "trademark" => "texttrademark",
    "triangle.b" => "bigtriangledown",
    "triangle.filled.b" => "\\blacktriangledown",
    "triangle.filled.l" => "\\blacktriangleleft",
    "triangle.filled.r" => "\\blacktriangleright",
    "triangle.filled.t" => "\\blacktriangle",
    "triangle.l" => "triangleleft",
    "triangle.r" => "triangleright",
    "triangle.stroked.b" => "\\triangledown",
    "triangle.stroked.l" => "\\triangleleft",
    "triangle.stroked.r" => "\\triangleright",
    "triangle.stroked.small.t" => "\\vartriangle",
    "triangle.stroked.t" => "\\triangle",
    "triangle.t" => "bigtriangleup",
    "union" => "cup",
    "union.big" => "bigcup",
    "union.dot" => "\\cupdot",
    "union.double" => "\\Cup",
    "union.plus" => "\\uplus",
    "union.sq" => "sqcup",
    "union.sq.big" => "bigsqcup",
    "upsilon" => "upsilon",
    "vec" => "vec",
    "vert" => "vert",
    "vert.double" => "Vert",
    "without" => "\\setminus",
    "wp" => "wp",
    "wreath" => "wr",
    "xi" => "xi",
    "xor" => "\\oplus",
    "xor.big" => "\\bigoplus",
    "yen" => "textyen",
    "zeta" => "zeta",
    "|->" => "mapsto",
    "|=>" => "Mapsto",
    "||" => "|",
    "~>" => "rightsquigarrow",
    "~~>" => "leadsto",
};

// =============================================================================
// DELIMITER_MAP: Single source of truth for lr() delimiter conversion
// Maps Typst delimiter representations to LaTeX output
// =============================================================================

/// Map from Typst delimiter names/chars to LaTeX
/// Key: Typst representation (symbol name or Unicode char)
/// Value: LaTeX output string
pub static DELIMITER_MAP: phf::Map<&'static str, &'static str> = phf_map! {
    "(" => "(",
    ")" => ")",
    "[" => "[",
    "]" => "]",
    "angle.l" => "\\langle",
    "angle.r" => "\\rangle",
    "bar.v" => "|",
    "bar.v.double" => "\\|",
    "brace.l" => "\\{",
    "brace.r" => "\\}",
    "bracket.l" => "[",
    "bracket.r" => "]",
    "ceil.l" => "\\lceil",
    "ceil.r" => "\\rceil",
    "chevron.l" => "\\langle",
    "chevron.r" => "\\rangle",
    "floor.l" => "\\lfloor",
    "floor.r" => "\\rfloor",
    "paren.l" => "(",
    "paren.r" => ")",
    "vert" => "|",
    "vert.double" => "\\|",
    "{" => "\\{",
    "|" => "|",
    "||" => "\\|",
    "}" => "\\}",
    "⌈" => "\\lceil",
    "⌉" => "\\rceil",
    "⌊" => "\\lfloor",
    "⌋" => "\\rfloor",
    "⟨" => "\\langle",
    "⟩" => "\\rangle",
    "〈" => "\\langle",
    "〉" => "\\rangle",
};
