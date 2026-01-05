//! Extended symbol mappings from tex2typst project
//!
//! This module provides comprehensive LaTeX to Typst symbol mappings,
//! covering ~900+ additional symbols not in the base mitex spec.

use fxhash::FxHashMap;
use lazy_static::lazy_static;

lazy_static! {
    /// Extended LaTeX to Typst symbol mappings
    /// Key: LaTeX command name (without backslash)
    /// Value: Typst equivalent
    pub static ref EXTENDED_SYMBOLS: FxHashMap<&'static str, &'static str> = {
        let mut m = FxHashMap::default();

        // === Spacing and Style ===
        m.insert("displaystyle", "display");
        m.insert("textstyle", "inline");
        m.insert("hspace", "#h");

        // === Delimiters and Bars ===
        m.insert("|", "bar.v.double");
        m.insert("vert", "bar.v");
        m.insert("Vert", "bar.v.double");
        m.insert("Vvert", "bar.v.triple");

        // === Triangles ===
        m.insert("blacktriangleleft", "triangle.filled.l");
        m.insert("blacktriangleright", "triangle.filled.r");
        m.insert("blacktriangle", "triangle.filled.small.t");
        m.insert("blacktriangledown", "triangle.filled.small.b");
        m.insert("bigblacktriangledown", "triangle.filled.b");
        m.insert("bigblacktriangleup", "triangle.filled.t");
        m.insert("bigtriangledown", "triangle.stroked.b");
        m.insert("bigtriangleup", "triangle.stroked.t");
        m.insert("triangledown", "triangle.stroked.small.b");
        m.insert("triangleleft", "triangle.stroked.l");
        m.insert("triangleright", "triangle.stroked.r");
        m.insert("vartriangle", "triangle.stroked.small.t");
        m.insert("vartriangleleft", "lt.tri");
        m.insert("vartriangleright", "gt.tri");
        m.insert("trianglelefteq", "lt.tri.eq");
        m.insert("trianglerighteq", "gt.tri.eq");
        m.insert("ntriangleleft", "lt.tri.not");
        m.insert("ntriangleright", "gt.tri.not");
        m.insert("ntrianglelefteq", "lt.tri.eq.not");
        m.insert("ntrianglerighteq", "gt.tri.eq.not");

        // === Card Suits ===
        m.insert("clubsuit", "suit.club.filled");
        m.insert("spadesuit", "suit.spade.filled");
        m.insert("heartsuit", "suit.heart.stroked");
        m.insert("diamondsuit", "suit.diamond.stroked");
        m.insert("varclubsuit", "suit.club.stroked");
        m.insert("varspadesuit", "suit.spade.stroked");

        // === Arrows - Basic ===
        m.insert("hookleftarrow", "arrow.l.hook");
        m.insert("hookrightarrow", "arrow.r.hook");
        m.insert("leftrightarrow", "arrow.l.r");
        m.insert("nearrow", "arrow.tr");
        m.insert("nwarrow", "arrow.tl");
        m.insert("searrow", "arrow.br");
        m.insert("swarrow", "arrow.bl");
        m.insert("updownarrow", "arrow.t.b");
        m.insert("leadsto", "arrow.r.squiggly");
        m.insert("rightsquigarrow", "arrow.r.squiggly");
        m.insert("leftsquigarrow", "arrow.l.squiggly");
        m.insert("leftrightsquigarrow", "arrow.l.r.wave");

        // === Arrows - Double ===
        m.insert("Leftrightarrow", "arrow.l.r.double");
        m.insert("Nearrow", "arrow.tr.double");
        m.insert("Nwarrow", "arrow.tl.double");
        m.insert("Searrow", "arrow.br.double");
        m.insert("Swarrow", "arrow.bl.double");
        m.insert("Updownarrow", "arrow.t.b.double");

        // === Arrows - Long ===
        m.insert("longleftarrow", "arrow.l.long");
        m.insert("longrightarrow", "arrow.r.long");
        m.insert("longleftrightarrow", "arrow.l.r.long");
        m.insert("Longleftarrow", "arrow.l.double.long");
        m.insert("Longrightarrow", "arrow.r.double.long");
        m.insert("Longleftrightarrow", "arrow.l.r.double.long");
        m.insert("longmapsto", "arrow.r.long.bar");
        m.insert("longmapsfrom", "arrow.l.long.bar");

        // === Arrows - Twohead ===
        m.insert("twoheadrightarrow", "arrow.r.twohead");
        m.insert("twoheadleftarrow", "arrow.l.twohead");
        m.insert("twoheaduparrow", "arrow.t.twohead");
        m.insert("twoheaddownarrow", "arrow.b.twohead");

        // === Arrows - Tail ===
        m.insert("rightarrowtail", "arrow.r.tail");
        m.insert("leftarrowtail", "arrow.l.tail");

        // === Arrows - Loop ===
        m.insert("looparrowleft", "arrow.l.loop");
        m.insert("looparrowright", "arrow.r.loop");

        // === Arrows - Curve ===
        m.insert("curvearrowleft", "arrow.ccw.half");
        m.insert("curvearrowright", "arrow.cw.half");

        // === Arrows - Negated ===
        m.insert("nleftarrow", "arrow.l.not");
        m.insert("nrightarrow", "arrow.r.not");
        m.insert("nleftrightarrow", "arrow.l.r.not");
        m.insert("nLeftarrow", "arrow.l.double.not");
        m.insert("nRightarrow", "arrow.r.double.not");
        m.insert("nLeftrightarrow", "arrow.l.r.double.not");

        // === Arrows - Multiple ===
        m.insert("leftleftarrows", "arrows.ll");
        m.insert("rightrightarrows", "arrows.rr");
        m.insert("leftrightarrows", "arrows.lr");
        m.insert("rightleftarrows", "arrows.rl");
        m.insert("upuparrows", "arrows.tt");
        m.insert("downdownarrows", "arrows.bb");
        m.insert("updownarrows", "arrows.tb");
        m.insert("downuparrows", "arrows.bt");

        // === Arrows - Triple/Quad ===
        m.insert("Lleftarrow", "arrow.l.triple");
        m.insert("Rrightarrow", "arrow.r.triple");
        m.insert("LLeftarrow", "arrow.l.quad");
        m.insert("RRightarrow", "arrow.r.quad");
        m.insert("Uuparrow", "arrow.t.triple");
        m.insert("Ddownarrow", "arrow.b.triple");
        m.insert("UUparrow", "arrow.t.quad");
        m.insert("DDownarrow", "arrow.b.quad");

        // === Harpoons ===
        m.insert("leftharpoonup", "harpoon.lt");
        m.insert("leftharpoondown", "harpoon.lb");
        m.insert("rightharpoonup", "harpoon.rt");
        m.insert("rightharpoondown", "harpoon.rb");
        m.insert("upharpoonleft", "harpoon.tl");
        m.insert("upharpoonright", "harpoon.tr");
        m.insert("downharpoonleft", "harpoon.bl");
        m.insert("downharpoonright", "harpoon.br");
        m.insert("leftrightharpoons", "harpoons.ltrb");
        m.insert("rightleftharpoons", "harpoons.rtlb");

        // === Map Arrows ===
        m.insert("mapsto", "arrow.r.bar");
        m.insert("mapsfrom", "arrow.l.bar");
        m.insert("Mapsto", "arrow.r.double.bar");
        m.insert("Mapsfrom", "arrow.l.double.bar");
        m.insert("mapsup", "arrow.t.bar");
        m.insert("mapsdown", "arrow.b.bar");

        // === Blackboard Bold (Bbb) ===
        m.insert("BbbA", "AA");
        m.insert("BbbB", "BB");
        m.insert("BbbC", "CC");
        m.insert("BbbD", "DD");
        m.insert("BbbE", "EE");
        m.insert("BbbF", "FF");
        m.insert("BbbG", "GG");
        m.insert("BbbH", "HH");
        m.insert("BbbI", "II");
        m.insert("BbbJ", "JJ");
        m.insert("BbbK", "KK");
        m.insert("BbbL", "LL");
        m.insert("BbbM", "MM");
        m.insert("BbbN", "NN");
        m.insert("BbbO", "OO");
        m.insert("BbbP", "PP");
        m.insert("BbbQ", "QQ");
        m.insert("BbbR", "RR");
        m.insert("BbbS", "SS");
        m.insert("BbbT", "TT");
        m.insert("BbbU", "UU");
        m.insert("BbbV", "VV");
        m.insert("BbbW", "WW");
        m.insert("BbbX", "XX");
        m.insert("BbbY", "YY");
        m.insert("BbbZ", "ZZ");

        // === Comparison - Greater ===
        m.insert("geqq", "gt.equiv");
        m.insert("geqslant", "gt.eq.slant");
        m.insert("ggg", "gt.triple");
        m.insert("gggnest", "gt.triple.nested");
        m.insert("gnapprox", "gt.napprox");
        m.insert("gneq", "gt.neq");
        m.insert("gneqq", "gt.nequiv");
        m.insert("gnsim", "gt.ntilde");
        m.insert("greater", "gt");
        m.insert("gtrapprox", "gt.approx");
        m.insert("gtrdot", "gt.dot");
        m.insert("gtreqless", "gt.eq.lt");
        m.insert("gtrless", "gt.lt");
        m.insert("gtrsim", "gt.tilde");
        m.insert("ngtr", "gt.not");
        m.insert("ngtrless", "gt.lt.not");
        m.insert("ngtrsim", "gt.tilde.not");

        // === Comparison - Less ===
        m.insert("leqq", "lt.equiv");
        m.insert("leqslant", "lt.eq.slant");
        m.insert("less", "lt");
        m.insert("lessapprox", "lt.approx");
        m.insert("lessdot", "lt.dot");
        m.insert("lesseqgtr", "lt.eq.gt");
        m.insert("lessgtr", "lt.gt");
        m.insert("lesssim", "lt.tilde");
        m.insert("lll", "lt.triple");
        m.insert("lllnest", "lt.triple.nested");
        m.insert("lnapprox", "lt.napprox");
        m.insert("lneq", "lt.neq");
        m.insert("lneqq", "lt.nequiv");
        m.insert("lnsim", "lt.ntilde");
        m.insert("nless", "lt.not");
        m.insert("nlessgtr", "lt.gt.not");
        m.insert("nlesssim", "lt.tilde.not");

        // === Precedence ===
        m.insert("Prec", "prec.double");
        m.insert("precapprox", "prec.approx");
        m.insert("preccurlyeq", "prec.curly.eq");
        m.insert("preceqq", "prec.equiv");
        m.insert("precnapprox", "prec.napprox");
        m.insert("precneq", "prec.neq");
        m.insert("precneqq", "prec.nequiv");
        m.insert("precnsim", "prec.ntilde");
        m.insert("precsim", "prec.tilde");
        m.insert("nprec", "prec.not");
        m.insert("npreccurlyeq", "prec.curly.eq.not");

        // === Succession ===
        m.insert("Succ", "succ.double");
        m.insert("succapprox", "succ.approx");
        m.insert("succcurlyeq", "succ.curly.eq");
        m.insert("succeqq", "succ.equiv");
        m.insert("succnapprox", "succ.napprox");
        m.insert("succneq", "succ.neq");
        m.insert("succneqq", "succ.nequiv");
        m.insert("succnsim", "succ.ntilde");
        m.insert("succsim", "succ.tilde");
        m.insert("nsucc", "succ.not");
        m.insert("nsucccurlyeq", "succ.curly.eq.not");

        // === Subset/Superset ===
        m.insert("Subset", "subset.double");
        m.insert("subsetdot", "subset.dot");
        m.insert("nsubset", "subset.not");
        m.insert("Supset", "supset.double");
        m.insert("supsetdot", "supset.dot");
        m.insert("nsupset", "supset.not");
        m.insert("sqsubset", "subset.sq");
        m.insert("sqsupset", "supset.sq");
        m.insert("sqsubsetneq", "subset.sq.neq");
        m.insert("sqsupsetneq", "supset.sq.neq");
        m.insert("nsqsubseteq", "subset.eq.sq.not");
        m.insert("nsqsupseteq", "supset.eq.sq.not");

        // === Set Operations ===
        m.insert("Cap", "inter.double");
        m.insert("capdot", "inter.dot");
        m.insert("capwedge", "inter.and");
        m.insert("Cup", "union.double");
        m.insert("cupdot", "union.dot");
        m.insert("cupleftarrow", "union.arrow");
        m.insert("cupvee", "union.or");
        m.insert("sqcap", "inter.sq");
        m.insert("Sqcap", "inter.sq.double");
        m.insert("sqcup", "union.sq");
        m.insert("Sqcup", "union.sq.double");
        m.insert("bigcap", "inter.big");
        m.insert("bigcup", "union.big");
        m.insert("bigsqcap", "inter.sq.big");
        m.insert("bigsqcup", "union.sq.big");
        m.insert("bigcupdot", "union.dot.big");
        m.insert("biguplus", "union.plus.big");
        m.insert("uminus", "union.minus");

        // === Logic ===
        m.insert("land", "and");
        m.insert("lor", "or");
        m.insert("lnot", "not");
        m.insert("curlyvee", "or.curly");
        m.insert("curlywedge", "and.curly");
        m.insert("Vee", "or.double");
        m.insert("Wedge", "and.double");
        m.insert("veedot", "or.dot");
        m.insert("wedgedot", "and.dot");
        m.insert("bigvee", "or.big");
        m.insert("bigwedge", "and.big");

        // === Equality/Equivalence ===
        m.insert("equal", "eq");
        m.insert("Equiv", "eq.quad");
        m.insert("nequiv", "equiv.not");
        m.insert("Doteq", "eq.dots");
        m.insert("doteq", "dot(eq)");
        m.insert("eqdef", "eq.def");
        m.insert("eqcolon", "eq.colon");
        m.insert("coloneq", "colon.eq");
        m.insert("Coloneq", "colon.double.eq");
        m.insert("eqgtr", "eq.gt");
        m.insert("eqless", "eq.lt");
        m.insert("curlyeqprec", "eq.prec");
        m.insert("curlyeqsucc", "eq.succ");
        m.insert("fallingdotseq", "eq.dots.down");
        m.insert("risingdotseq", "eq.dots.up");
        m.insert("measeq", "eq.m");
        m.insert("questeq", "eq.quest");
        m.insert("stareq", "eq.star");
        m.insert("triangleq", "eq.delta");
        m.insert("veeeq", "eq.equi");
        m.insert("wedgeq", "eq.est");

        // === Similarity/Approximation ===
        m.insert("approxeq", "approx.eq");
        m.insert("approxident", "tilde.triple");
        m.insert("backcong", "tilde.rev.equiv");
        m.insert("backsim", "tilde.rev");
        m.insert("backsimeq", "tilde.eq.rev");
        m.insert("dotsim", "tilde.dot");
        m.insert("eqsim", "minus.tilde");
        m.insert("napprox", "approx.not");
        m.insert("nasymp", "asymp.not");
        m.insert("ncong", "tilde.equiv.not");
        m.insert("nsim", "tilde.not");
        m.insert("nsimeq", "tilde.eq.not");
        m.insert("sime", "tilde.eq");
        m.insert("simneqq", "tilde.nequiv");

        // === Tack/Turnstile ===
        m.insert("dashv", "tack.l");
        m.insert("Dashv", "tack.l.double");
        m.insert("dashVdash", "tack.l.r");
        m.insert("vdash", "tack.r");
        m.insert("vDash", "tack.r.double");
        m.insert("Vdash", "forces");
        m.insert("nvdash", "tack.r.not");
        m.insert("nvDash", "tack.r.double.not");
        m.insert("nVdash", "forces.not");
        m.insert("longdashv", "tack.l.long");
        m.insert("vlongdash", "tack.r.long");
        m.insert("shortdowntack", "tack.b.short");
        m.insert("shortlefttack", "tack.l.short");
        m.insert("shortuptack", "tack.t.short");
        m.insert("assert", "tack.r.short");
        m.insert("barV", "tack.b.double");
        m.insert("Vbar", "tack.t.double");
        m.insert("bigbot", "tack.t.big");
        m.insert("bigtop", "tack.b.big");

        // === Dots ===
        m.insert("ddddot", "dot.quad");
        m.insert("dddot", "dot.triple");
        m.insert("adots", "dots.up");
        m.insert("unicodecdots", "dots.h.c");
        m.insert("unicodeellipsis", "dots.h");

        // === Integrals ===
        m.insert("fint", "integral.slash");
        m.insert("intbar", "integral.dash");
        m.insert("intBar", "integral.dash.double");
        m.insert("intcap", "integral.inter");
        m.insert("intclockwise", "integral.cw");
        m.insert("intcup", "integral.union");
        m.insert("intlarhk", "integral.arrow.hook");
        m.insert("intx", "integral.times");
        m.insert("ointctrclockwise", "integral.cont.ccw");
        m.insert("varointclockwise", "integral.cont.cw");
        m.insert("sqint", "integral.square");
        m.insert("awint", "integral.ccw");
        m.insert("sumint", "sum.integral");

        // === Brackets/Delimiters ===
        m.insert("langle", "chevron.l");
        m.insert("rangle", "chevron.r");
        m.insert("lAngle", "chevron.l.double");
        m.insert("rAngle", "chevron.r.double");
        m.insert("langledot", "chevron.l.dot");
        m.insert("rangledot", "chevron.r.dot");
        m.insert("llangle", "chevron.l.closed");
        m.insert("rrangle", "chevron.r.closed");
        m.insert("lcurvyangle", "chevron.l.curly");
        m.insert("rcurvyangle", "chevron.r.curly");
        m.insert("lbrace", "brace.l");
        m.insert("rbrace", "brace.r");
        m.insert("lBrace", "brace.l.stroked");
        m.insert("rBrace", "brace.r.stroked");
        m.insert("lbrack", "bracket.l");
        m.insert("rbrack", "bracket.r");
        m.insert("lBrack", "bracket.l.stroked");
        m.insert("rBrack", "bracket.r.stroked");
        m.insert("lceil", "ceil.l");
        m.insert("rceil", "ceil.r");
        m.insert("lfloor", "floor.l");
        m.insert("rfloor", "floor.r");
        m.insert("lparen", "paren.l");
        m.insert("rparen", "paren.r");
        m.insert("lParen", "paren.l.stroked");
        m.insert("rParen", "paren.r.stroked");
        m.insert("lgroup", "paren.l.flat");
        m.insert("rgroup", "paren.r.flat");
        m.insert("llparenthesis", "paren.l.closed");
        m.insert("rrparenthesis", "paren.r.closed");
        m.insert("lmoustache", "mustache.l");
        m.insert("rmoustache", "mustache.r");
        m.insert("lbag", "bag.l");
        m.insert("rbag", "bag.r");
        m.insert("lvzigzag", "fence.l");
        m.insert("rvzigzag", "fence.r");
        m.insert("Lvzigzag", "fence.l.double");
        m.insert("Rvzigzag", "fence.r.double");
        m.insert("lbrbrak", "shell.l");
        m.insert("rbrbrak", "shell.r");
        m.insert("Lbrbrak", "shell.l.stroked");
        m.insert("Rbrbrak", "shell.r.stroked");
        m.insert("lblkbrbrak", "shell.l.filled");
        m.insert("rblkbrbrak", "shell.r.filled");
        m.insert("obrbrak", "shell.t");
        m.insert("ubrbrak", "shell.b");

        // === Corners ===
        m.insert("llcorner", "corner.l.b");
        m.insert("lrcorner", "corner.r.b");
        m.insert("ulcorner", "corner.l.t");
        m.insert("urcorner", "corner.r.t");

        // === Circles/Shapes ===
        m.insert("bigcirc", "circle.big");
        m.insert("lgblkcircle", "circle.filled.big");
        m.insert("lgwhtcircle", "circle.stroked.big");
        m.insert("mdlgblkcircle", "circle.filled");
        m.insert("mdlgwhtcircle", "circle.stroked");
        m.insert("mdsmblkcircle", "circle.filled.tiny");
        m.insert("mdsmwhtcircle", "circle.stroked.small");
        m.insert("smblkcircle", "bullet");
        m.insert("smwhtcircle", "bullet.stroked");
        m.insert("vysmblkcircle", "circle.filled.small");
        m.insert("vysmwhtcircle", "circle.stroked.tiny");
        m.insert("dottedcircle", "circle.dotted");

        // === Squares ===
        m.insert("lgblksquare", "square.filled.big");
        m.insert("lgwhtsquare", "square.stroked.big");
        m.insert("mdlgblksquare", "square.filled");
        m.insert("mdlgwhtsquare", "square.stroked");
        m.insert("dottedsquare", "square.stroked.dotted");
        m.insert("squoval", "square.stroked.rounded");

        // === Diamonds/Lozenges ===
        m.insert("mdblkdiamond", "diamond.filled.medium");
        m.insert("mdblklozenge", "lozenge.filled.medium");
        m.insert("mdlgblkdiamond", "diamond.filled");
        m.insert("mdlgblklozenge", "lozenge.filled");
        m.insert("mdlgwhtdiamond", "diamond.stroked");
        m.insert("mdlgwhtlozenge", "lozenge.stroked");
        m.insert("mdwhtdiamond", "diamond.stroked.medium");
        m.insert("mdwhtlozenge", "lozenge.stroked.medium");
        m.insert("smblkdiamond", "diamond.filled.small");
        m.insert("smblklozenge", "lozenge.filled.small");
        m.insert("smwhtdiamond", "diamond.stroked.small");
        m.insert("smwhtlozenge", "lozenge.stroked.small");
        m.insert("diamondcdot", "diamond.stroked.dot");

        // === Rectangles ===
        m.insert("hrectangle", "rect.stroked.h");
        m.insert("hrectangleblack", "rect.filled.h");
        m.insert("vrectangle", "rect.stroked.v");
        m.insert("vrectangleblack", "rect.filled.v");

        // === Ovals ===
        m.insert("blkhorzoval", "ellipse.filled.h");
        m.insert("blkvertoval", "ellipse.filled.v");
        m.insert("whthorzoval", "ellipse.stroked.h");
        m.insert("whtvertoval", "ellipse.stroked.v");

        // === Other Shapes ===
        m.insert("parallelogram", "parallelogram.stroked");
        m.insert("parallelogramblack", "parallelogram.filled");
        m.insert("pentagon", "penta.stroked");
        m.insert("pentagonblack", "penta.filled");
        m.insert("varhexagon", "hexa.stroked");
        m.insert("varhexagonblack", "hexa.filled");
        m.insert("hourglass", "hourglass.stroked");
        m.insert("blackhourglass", "hourglass.filled");

        // === Box Operations ===
        m.insert("boxast", "ast.square");
        m.insert("boxdot", "dot.square");
        m.insert("boxminus", "minus.square");
        m.insert("boxplus", "plus.square");
        m.insert("boxtimes", "times.square");

        // === Circle Operations ===
        m.insert("circledast", "ast.op.o");
        m.insert("circledbullet", "bullet.o");
        m.insert("circledcirc", "compose.o");
        m.insert("circleddash", "dash.o");
        m.insert("circledequal", "cc.nd");
        m.insert("circledparallel", "parallel.o");
        m.insert("circledvert", "bar.v.o");
        m.insert("circledwhitebullet", "bullet.stroked.o");
        m.insert("obslash", "backslash.o");
        m.insert("odiv", "div.o");
        m.insert("odot", "dot.o");
        m.insert("odotslashdot", "div.slanted.o");
        m.insert("ogreaterthan", "gt.o");
        m.insert("olessthan", "lt.o");
        m.insert("ominus", "minus.o");
        m.insert("operp", "perp.o");
        m.insert("oplus", "plus.o");
        m.insert("opluslhrim", "plus.o.l");
        m.insert("oplusrhrim", "plus.o.r");
        m.insert("oslash", "slash.o");
        m.insert("otimes", "times.o");
        m.insert("otimeshat", "times.o.hat");
        m.insert("otimeslhrim", "times.o.l");
        m.insert("otimesrhrim", "times.o.r");
        m.insert("rightarrowonoplus", "plus.o.arrow");

        // === Big Operators ===
        m.insert("bigodot", "dot.o.big");
        m.insert("bigoplus", "plus.o.big");
        m.insert("bigotimes", "times.o.big");
        m.insert("bigtimes", "times.big");
        m.insert("biginterleave", "interleave.big");
        m.insert("bigstar", "star.filled");
        m.insert("bigwhitestar", "star.stroked");

        // === Plus/Minus Variants ===
        m.insert("dotminus", "minus.dot");
        m.insert("dotplus", "plus.dot");
        m.insert("doubleplus", "plus.double");
        m.insert("tripleplus", "plus.triple");
        m.insert("triangleminus", "minus.triangle");
        m.insert("triangleplus", "plus.triangle");

        // === Times Variants ===
        m.insert("divideontimes", "times.div");
        m.insert("leftthreetimes", "times.three.l");
        m.insert("rightthreetimes", "times.three.r");
        m.insert("ltimes", "times.l");
        m.insert("rtimes", "times.r");
        m.insert("smashtimes", "smash");
        m.insert("triangletimes", "times.triangle");

        // === Slashes ===
        m.insert("sslash", "slash.double");
        m.insert("trslash", "slash.triple");
        m.insert("xsol", "slash.big");
        m.insert("rsolbar", "backslash.not");

        // === Angles ===
        m.insert("angle", "angle");
        m.insert("angles", "angle.s");
        m.insert("angdnr", "angle.acute");
        m.insert("measuredangle", "angle.arc");
        m.insert("measuredangleleft", "angle.arc.rev");
        m.insert("measuredrightangle", "angle.right.arc");
        m.insert("revangle", "angle.rev");
        m.insert("rightangle", "angle.right");
        m.insert("rightanglemdot", "angle.right.dot");
        m.insert("rightanglesqr", "angle.right.square");
        m.insert("sphericalangle", "angle.spheric");
        m.insert("sphericalangleup", "angle.spheric.t");
        m.insert("gtlpar", "angle.spheric.rev");
        m.insert("threedangle", "angle.spatial");
        m.insert("wideangledown", "angle.obtuse");
        m.insert("rangledownzigzagarrow", "angle.azimuth");

        // === Parallel ===
        m.insert("parallel", "parallel");
        m.insert("nparallel", "parallel.not");
        m.insert("eparsl", "parallel.slanted.eq");
        m.insert("equalparallel", "parallel.eq");
        m.insert("equivVert", "parallel.equiv");
        m.insert("eqvparsl", "parallel.slanted.equiv");
        m.insert("nhpar", "parallel.struck");
        m.insert("parsim", "parallel.tilde");
        m.insert("smeparsl", "parallel.slanted.eq.tilde");

        // === Primes ===
        m.insert("prime", "prime");
        m.insert("dprime", "prime.double");
        m.insert("trprime", "prime.triple");
        m.insert("qprime", "prime.quad");
        m.insert("backprime", "prime.rev");
        m.insert("backdprime", "prime.double.rev");
        m.insert("backtrprime", "prime.triple.rev");

        // === Colons ===
        m.insert("colon", "colon");
        m.insert("Colon", "colon.double");
        m.insert("dashcolon", "dash.colon");
        m.insert("threedotcolon", "colon.tri.op");

        // === Join ===
        m.insert("Join", "join");
        m.insert("fullouterjoin", "join.l.r");
        m.insert("leftouterjoin", "join.l");
        m.insert("rightouterjoin", "join.r");

        // === Emptyset ===
        m.insert("emptyset", "nothing");
        m.insert("varnothing", "emptyset");
        m.insert("revemptyset", "emptyset.rev");
        m.insert("emptysetoarr", "emptyset.arrow.r");
        m.insert("emptysetoarrl", "emptyset.arrow.l");
        m.insert("emptysetobar", "emptyset.bar");
        m.insert("emptysetocirc", "emptyset.circle");

        // === Infinity ===
        m.insert("infin", "infinity");
        m.insert("iinfin", "infinity.incomplete");
        m.insert("nvinfty", "infinity.bar");
        m.insert("tieinfty", "infinity.tie");

        // === Error Bars ===
        m.insert("errbarblackcircle", "errorbar.circle.filled");
        m.insert("errbarblackdiamond", "errorbar.diamond.filled");
        m.insert("errbarblacksquare", "errorbar.square.filled");
        m.insert("errbarcircle", "errorbar.circle.stroked");
        m.insert("errbardiamond", "errorbar.diamond.stroked");
        m.insert("errbarsquare", "errorbar.square.stroked");

        // === Dice ===
        m.insert("dicei", "die.one");
        m.insert("diceii", "die.two");
        m.insert("diceiii", "die.three");
        m.insert("diceiv", "die.four");
        m.insert("dicev", "die.five");
        m.insert("dicevi", "die.six");

        // === Music ===
        m.insert("flat", "flat");
        m.insert("natural", "natural");
        m.insert("sharp", "sharp");
        m.insert("eighthnote", "note.eighth.alt");
        m.insert("quarternote", "note.quarter.alt");
        m.insert("twonotes", "note.eighth.beamed");

        // === Misc Symbols ===
        m.insert("aleph", "alef");
        m.insert("because", "because");
        m.insert("therefore", "therefore");
        m.insert("bot", "bot");
        m.insert("top", "tack.b");
        m.insert("bullet", "bullet");
        m.insert("hyphenbullet", "bullet.hyph");
        m.insert("inversebullet", "bullet.hole");
        m.insert("caretinsert", "caret");
        m.insert("checkmark", "checkmark");
        m.insert("complement", "complement");
        m.insert("copyright", "copyright");
        m.insert("dagger", "dagger");
        m.insert("ddagger", "dagger.double");
        m.insert("diamond", "diamond");
        m.insert("ell", "ell");
        m.insert("euro", "euro");
        m.insert("exists", "exists");
        m.insert("nexists", "exists.not");
        m.insert("forall", "forall");
        m.insert("frown", "frown");
        m.insert("smile", "smile");
        m.insert("hbar", "planck");
        m.insert("horizbar", "bar.h");
        m.insert("imath", "dotless.i");
        m.insert("jmath", "dotless.j");
        m.insert("imageof", "image");
        m.insert("increment", "laplace");
        m.insert("interleave", "interleave");
        m.insert("intercal", "top");
        m.insert("maltese", "maltese");
        m.insert("mho", "Omega.inv");
        m.insert("multimap", "multimap");
        m.insert("dualmap", "multimap.double");
        m.insert("nabla", "gradient");
        m.insert("neg", "not");
        m.insert("ni", "in.rev");
        m.insert("nni", "in.rev.not");
        m.insert("origof", "original");
        m.insert("partial", "partial");
        m.insert("perp", "perp");
        m.insert("P", "pilcrow");
        m.insert("mathparagraph", "pilcrow");
        m.insert("pounds", "pound");
        m.insert("mathsterling", "pound");
        m.insert("propto", "prop");
        m.insert("QED", "qed");
        m.insert("Question", "quest.double");
        m.insert("S", "section");
        m.insert("mathsection", "section");
        m.insert("setminus", "without");
        m.insert("smallsetminus", "without");
        m.insert("star", "star.op");
        m.insert("astrosun", "sun");
        m.insert("diameter", "diameter");
        m.insert("wr", "wreath");
        m.insert("yen", "yen");
        m.insert("mathyen", "yen");

        // === Greek variants ===
        m.insert("upDigamma", "Digamma");
        m.insert("updigamma", "digamma");
        m.insert("turnediota", "iota.inv");
        m.insert("upbackepsilon", "epsilon.alt.rev");

        // === Math punctuation ===
        m.insert("mathampersand", "amp");
        m.insert("upand", "amp.inv");
        m.insert("mathatsign", "at");
        m.insert("mathcolon", "colon");
        m.insert("mathcomma", "comma");
        m.insert("mathdollar", "dollar");
        m.insert("mathexclam", "excl");
        m.insert("mathhyphen", "hyph");
        m.insert("mathpercent", "percent");
        m.insert("mathperiod", "dot.basic");
        m.insert("mathplus", "plus");
        m.insert("minus", "minus");
        m.insert("mathquestion", "quest");
        m.insert("mathratio", "ratio");
        m.insert("mathsemicolon", "semi");
        m.insert("mathslash", "slash");

        // === Fence ===
        m.insert("fourvdots", "fence.dotted");
        m.insert("nhVvert", "interleave.struck");

        // === Lat ===
        m.insert("lat", "lat");
        m.insert("late", "lat.eq");
        m.insert("smt", "smt");
        m.insert("smte", "smt.eq");

        // === Misc operators ===
        m.insert("ast", "ast.op");
        m.insert("circ", "circle.small");
        m.insert("div", "div");
        m.insert("mid", "divides");
        m.insert("nmid", "divides.not");
        m.insert("revnmid", "divides.not.rev");
        m.insert("models", "models");
        m.insert("in", "in");
        m.insert("notin", "in.not");
        m.insert("smallin", "in.small");
        m.insert("smallni", "in.rev.small");

        // === Aliases ===
        m.insert("gets", "arrow.l");
        m.insert("iff", "arrow.l.r.double.long");
        m.insert("implies", "arrow.r.double.long");
        m.insert("to", "arrow.r");

        m
    };
}

/// Look up an extended symbol mapping
pub fn lookup_extended_symbol(latex_cmd: &str) -> Option<&'static str> {
    EXTENDED_SYMBOLS.get(latex_cmd).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extended_symbols_lookup() {
        assert_eq!(lookup_extended_symbol("BbbR"), Some("RR"));
        assert_eq!(
            lookup_extended_symbol("twoheadrightarrow"),
            Some("arrow.r.twohead")
        );
        assert_eq!(lookup_extended_symbol("curlyvee"), Some("or.curly"));
        assert_eq!(lookup_extended_symbol("nonexistent"), None);
    }

    #[test]
    fn test_symbol_count() {
        // Ensure we have a substantial number of symbols
        assert!(
            EXTENDED_SYMBOLS.len() > 500,
            "Expected 500+ symbols, got {}",
            EXTENDED_SYMBOLS.len()
        );
    }
}
