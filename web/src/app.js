/**
 * Tylax Web App
 * LaTeX ↔ Typst bidirectional converter
 */

// State
const state = {
    direction: 't2l', // 'l2t' = LaTeX to Typst, 't2l' = Typst to LaTeX (默认 Typst → LaTeX)
    mode: 'math', // 'math' = Math formula, 'document' = Full document
    lang: 'zh', // 'zh' or 'en'
    wasmReady: false,
    wasm: null,
};

const translations = {
    zh: {
        title: 'Tylax - LaTeX ↔ Typst 转换器',
        description: 'Tylax - 高性能 LaTeX ↔ Typst 双向转换器，Rust + WebAssembly 驱动',
        modeMath: '公式',
        modeDocument: '文档',
        swap: '交换方向',
        theme: '切换主题',
        lang: '切换语言',
        clear: '清空',
        paste: '粘贴',
        copy: '复制',
        inputLatex: 'LaTeX 输入',
        outputTypst: 'Typst 输出',
        inputTypst: 'Typst 输入',
        outputLatex: 'LaTeX 输出',
        placeholderMathLatex: '在此输入 LaTeX 公式...\n\n例如：\\frac{1}{2} + \\alpha^2',
        placeholderDocLatex: '在此输入 LaTeX 文档...\n\n例如：\n\\documentclass{article}\n\\begin{document}\n\\section{Introduction}\nHello, world!\n\\end{document}',
        placeholderMathTypst: '在此输入 Typst 公式...\n\n例如：frac(1, 2) + alpha^2',
        placeholderDocTypst: '在此输入 Typst 文档...\n\n例如：\n= Introduction\n\nHello, *world*!\n\n$ E = m c^2 $',
        placeholderOutput: '转换结果将显示在这里...',
        hint: '支持 <strong>TikZ ↔ CeTZ</strong> 图形互转！直接输入 TikZ 或 CeTZ 代码即可自动识别转换',
        chars: '字符',
        ready: '就绪',
        converting: '转换中...',
        time: '转换耗时',
        failed: '转换失败',
        wasmLoaded: 'WASM 加载成功 ✓',
        wasmFallback: 'WASM 不可用，使用 JavaScript 模式',
        useJsMode: '使用 JavaScript 模式',
        copied: '已复制到剪贴板 ✓',
        copyFailed: '复制失败',
        pasted: '已粘贴',
        pasteFailed: '粘贴失败',
        noContent: '没有内容可复制',
    },
    en: {
        title: 'Tylax - LaTeX ↔ Typst Converter',
        description: 'Tylax - High-performance bidirectional LaTeX ↔ Typst converter powered by Rust and WebAssembly',
        modeMath: 'Math',
        modeDocument: 'Doc',
        swap: 'Swap Direction',
        theme: 'Toggle Theme',
        lang: 'Switch Language',
        clear: 'Clear',
        paste: 'Paste',
        copy: 'Copy',
        inputLatex: 'LaTeX Input',
        outputTypst: 'Typst Output',
        inputTypst: 'Typst Input',
        outputLatex: 'LaTeX Output',
        placeholderMathLatex: 'Enter LaTeX formula here...\n\ne.g.: \\frac{1}{2} + \\alpha^2',
        placeholderDocLatex: 'Enter LaTeX document here...\n\ne.g.:\n\\documentclass{article}\n\\begin{document}\n\\section{Introduction}\nHello, world!\n\\end{document}',
        placeholderMathTypst: 'Enter Typst formula here...\n\ne.g.: frac(1, 2) + alpha^2',
        placeholderDocTypst: 'Enter Typst document here...\n\ne.g.:\n= Introduction\n\nHello, *world*!\n\n$ E = m c^2 $',
        placeholderOutput: 'Conversion result will appear here...',
        hint: 'Supports <strong>TikZ ↔ CeTZ</strong> graphics conversion! Just enter TikZ or CeTZ code directly.',
        chars: 'chars',
        ready: 'Ready',
        converting: 'Converting...',
        time: 'Time',
        failed: 'Failed',
        wasmLoaded: 'WASM loaded successfully ✓',
        wasmFallback: 'WASM not available, using JS fallback',
        useJsMode: 'Using JavaScript Mode',
        copied: 'Copied to clipboard ✓',
        copyFailed: 'Copy failed',
        pasted: 'Pasted',
        pasteFailed: 'Paste failed',
        noContent: 'No content to copy',
    }
};

// DOM Elements
const elements = {
    // ... existing elements ...
    langToggle: document.getElementById('langToggle'), // Add langToggle
    // ... existing elements ...
    inputEditor: document.getElementById('inputEditor'),
    outputEditor: document.getElementById('outputEditor'),
    inputTitle: document.getElementById('inputTitle'),
    outputTitle: document.getElementById('outputTitle'),
    leftLabel: document.getElementById('leftLabel'),
    rightLabel: document.getElementById('rightLabel'),
    directionToggle: document.getElementById('directionToggle'),
    directionArrow: document.getElementById('directionArrow'),
    swapBtn: document.getElementById('swapBtn'),
    clearInputBtn: document.getElementById('clearInputBtn'),
    pasteBtn: document.getElementById('pasteBtn'),
    copyBtn: document.getElementById('copyBtn'),
    themeToggle: document.getElementById('themeToggle'),
    langToggle: document.getElementById('langToggle'), // Added
    charCount: document.getElementById('charCount'),
    convertTime: document.getElementById('convertTime'),
    toast: document.getElementById('toast'),
    toastMessage: document.getElementById('toastMessage'),
    modeToggle: document.getElementById('modeToggle'),
    modeMath: document.getElementById('modeMath'),
    modeDocument: document.getElementById('modeDocument'),
};

// ===== WASM Loading =====
async function initWasm() {
    try {
        // Dynamic import the WASM module
        const wasm = await import(/* @vite-ignore */ './pkg/tylax.js');
        // Initialize the WASM module
        await wasm.default();
        state.wasm = wasm;
        state.wasmReady = true;
        console.log('WASM module loaded successfully');
        console.log('Available functions:', Object.keys(wasm));
        showToast(translations[state.lang].wasmLoaded);
    } catch (e) {
        console.error('WASM loading failed:', e);
        console.log('WASM not available, using JavaScript fallback');
        showToast(translations[state.lang].useJsMode, 3000);
    }
}

// ===== Hierarchical Content Detection =====

/**
 * Layer 1: Check if input is a full LaTeX document
 * Documents contain structural elements like \documentclass, \begin{document}, sections
 */
function isFullLatexDocument(input) {
    return input.includes('\\documentclass') ||
           input.includes('\\begin{document}') ||
           (input.includes('\\section') && input.includes('\\end')) ||
           (input.includes('\\chapter') && input.includes('\\end'));
}

/**
 * Layer 1: Check if input is a full Typst document
 * Documents contain structural elements like #set, headings, or multiple paragraphs
 * EXCLUDES CeTZ code (which also uses #import but should be handled separately)
 */
function isFullTypstDocument(input) {
    // If it's CeTZ code, don't treat as document
    if (isPureCeTZ(input)) {
        return false;
    }
    return input.includes('#set') ||
           input.includes('#show') ||
           (input.includes('#import') && !input.includes('cetz')) ||
           /^=\s+\S/.test(input) ||  // Starts with heading
           input.includes('\n= ');    // Contains heading
}

/**
 * Layer 2: Check if input is a PURE TikZ code fragment
 * Must start with \begin{tikzpicture} and end with \end{tikzpicture}
 * This is stricter than general TikZ detection
 */
function isPureTikZ(input) {
    const trimmed = input.trim();
    return trimmed.startsWith('\\begin{tikzpicture}') && 
           trimmed.includes('\\end{tikzpicture}');
}

/**
 * Layer 2: Check if input is a PURE CeTZ code fragment
 * Must contain #canvas or cetz import pattern
 */
function isPureCeTZ(input) {
    const trimmed = input.trim();
    return (trimmed.includes('#canvas') && trimmed.includes('draw')) ||
           (trimmed.includes('@preview/cetz') && trimmed.includes('canvas'));
}

/**
 * Layer 3: Check if input contains TikZ elements (for documents with embedded TikZ)
 * Less strict - used for hint detection
 */
function containsTikZ(input) {
    return input.includes('\\begin{tikzpicture}') ||
           input.includes('tikzpicture');
}

/**
 * Layer 3: Check if input contains CeTZ elements (for documents with embedded CeTZ)
 */
function containsCeTZ(input) {
    return input.includes('#canvas') ||
           input.includes('cetz');
}

// ===== Conversion Functions =====

/**
 * Convert TikZ to CeTZ (direct)
 */
function tikzToCetz(input) {
    const t = translations[state.lang];
    if (state.wasmReady && state.wasm && state.wasm.tikzToCetz) {
        return state.wasm.tikzToCetz(input);
    }
    return (state.lang === 'zh' ? '// TikZ 转 CeTZ 需要 WASM 支持\n' : '// TikZ to CeTZ conversion requires WASM\n') + input;
}

/**
 * Convert CeTZ to TikZ (direct)
 */
function cetzToTikz(input) {
    const t = translations[state.lang];
    if (state.wasmReady && state.wasm && state.wasm.cetzToTikz) {
        return state.wasm.cetzToTikz(input);
    }
    return (state.lang === 'zh' ? '% CeTZ 转 TikZ 需要 WASM 支持\n' : '% CeTZ to TikZ conversion requires WASM\n') + input;
}

/**
 * Hierarchical LaTeX to Typst conversion
 * Priority: TikZ → Full document → Math formula
 * TikZ must be checked FIRST for standalone TikZ fragments
 */
function latexToTypst(input) {
    // Priority 1: Pure TikZ fragment
    if (isPureTikZ(input)) {
        return tikzToCetz(input);
    }
    
    // Priority 2: Full document (will handle embedded TikZ internally)
    if (isFullLatexDocument(input)) {
        if (state.wasmReady && state.wasm && state.wasm.latexDocumentToTypst) {
            return state.wasm.latexDocumentToTypst(input);
        }
        return fallbackLatexDocumentToTypst(input);
    }
    
    // Priority 3: Math formula (default)
    if (state.wasmReady && state.wasm) {
        return state.wasm.latexToTypst(input);
    }
    return fallbackLatexToTypst(input);
}

/**
 * Hierarchical Typst to LaTeX conversion
 * Priority: CeTZ → Full document → Math formula
 * CeTZ must be checked FIRST because it also uses #import
 */
function typstToLatex(input) {
    // Priority 1: Pure CeTZ fragment (must check before document because CeTZ uses #import)
    if (isPureCeTZ(input)) {
        return cetzToTikz(input);
    }
    
    // Priority 2: Full document
    if (isFullTypstDocument(input)) {
        if (state.wasmReady && state.wasm && state.wasm.typstDocumentToLatex) {
            return state.wasm.typstDocumentToLatex(input);
        }
        return fallbackTypstDocumentToLatex(input);
    }
    
    // Priority 3: Math formula (default)
    if (state.wasmReady && state.wasm) {
        return state.wasm.typstToLatex(input);
    }
    return fallbackTypstToLatex(input);
}

/**
 * Convert LaTeX document to Typst document
 * Uses hierarchical detection internally
 */
function latexDocumentToTypst(input) {
    // Pure TikZ in document mode → direct TikZ conversion
    if (isPureTikZ(input) && !isFullLatexDocument(input)) {
        return tikzToCetz(input);
    }
    
    // Full document (may contain TikZ, handled internally by converter)
    if (state.wasmReady && state.wasm && state.wasm.latexDocumentToTypst) {
        return state.wasm.latexDocumentToTypst(input);
    }
    return fallbackLatexDocumentToTypst(input);
}

/**
 * Convert Typst document to LaTeX document
 * Uses hierarchical detection internally
 */
function typstDocumentToLatex(input) {
    // Pure CeTZ in document mode → direct CeTZ conversion
    if (isPureCeTZ(input) && !isFullTypstDocument(input)) {
        return cetzToTikz(input);
    }
    
    // Full document (may contain CeTZ, handled internally by converter)
    if (state.wasmReady && state.wasm && state.wasm.typstDocumentToLatex) {
        return state.wasm.typstDocumentToLatex(input);
    }
    return fallbackTypstDocumentToLatex(input);
}

/**
 * Fallback LaTeX to Typst conversion (basic)
 */
function fallbackLatexToTypst(input) {
    let result = input;

    // Greek letters
    const greekMap = {
        '\\alpha': 'alpha', '\\beta': 'beta', '\\gamma': 'gamma', '\\delta': 'delta',
        '\\epsilon': 'epsilon', '\\zeta': 'zeta', '\\eta': 'eta', '\\theta': 'theta',
        '\\iota': 'iota', '\\kappa': 'kappa', '\\lambda': 'lambda', '\\mu': 'mu',
        '\\nu': 'nu', '\\xi': 'xi', '\\pi': 'pi', '\\rho': 'rho',
        '\\sigma': 'sigma', '\\tau': 'tau', '\\upsilon': 'upsilon', '\\phi': 'phi',
        '\\chi': 'chi', '\\psi': 'psi', '\\omega': 'omega',
        '\\Gamma': 'Gamma', '\\Delta': 'Delta', '\\Theta': 'Theta', '\\Lambda': 'Lambda',
        '\\Xi': 'Xi', '\\Pi': 'Pi', '\\Sigma': 'Sigma', '\\Phi': 'Phi',
        '\\Psi': 'Psi', '\\Omega': 'Omega',
    };

    // Operators and symbols
    const symbolMap = {
        '\\infty': 'infinity', '\\pm': 'plus.minus', '\\mp': 'minus.plus',
        '\\times': 'times', '\\div': 'div', '\\cdot': 'dot',
        '\\leq': '<=', '\\geq': '>=', '\\neq': '!=',
        '\\approx': 'approx', '\\equiv': 'equiv', '\\sim': 'tilde.op',
        '\\subset': 'subset', '\\supset': 'supset', '\\subseteq': 'subset.eq',
        '\\supseteq': 'supset.eq', '\\in': 'in', '\\notin': 'in.not',
        '\\cup': 'union', '\\cap': 'sect', '\\emptyset': 'emptyset',
        '\\forall': 'forall', '\\exists': 'exists', '\\neg': 'not',
        '\\land': 'and', '\\lor': 'or', '\\Rightarrow': '=>',
        '\\Leftarrow': '<=', '\\Leftrightarrow': '<=>',
        '\\rightarrow': '->', '\\leftarrow': '<-', '\\leftrightarrow': '<->',
        '\\partial': 'diff', '\\nabla': 'nabla',
        '\\sum': 'sum', '\\prod': 'product', '\\int': 'integral',
        '\\sin': 'sin', '\\cos': 'cos', '\\tan': 'tan',
        '\\log': 'log', '\\ln': 'ln', '\\exp': 'exp',
        '\\lim': 'lim', '\\max': 'max', '\\min': 'min',
        '\\ldots': '...', '\\cdots': 'dots.c', '\\vdots': 'dots.v',
    };

    // Apply Greek letters
    for (const [tex, typst] of Object.entries(greekMap)) {
        result = result.replaceAll(tex, typst);
    }

    // Apply symbols
    for (const [tex, typst] of Object.entries(symbolMap)) {
        result = result.replaceAll(tex, typst);
    }

    // Fractions: \frac{a}{b} -> frac(a, b)
    result = result.replace(/\\frac\{([^{}]*)\}\{([^{}]*)\}/g, 'frac($1, $2)');

    // Square root: \sqrt{x} -> sqrt(x)
    result = result.replace(/\\sqrt\{([^{}]*)\}/g, 'sqrt($1)');

    // Subscript/superscript with braces: _{...} -> _(...), ^{...} -> ^(...)
    result = result.replace(/_\{([^{}]*)\}/g, '_($1)');
    result = result.replace(/\^\{([^{}]*)\}/g, '^($1)');

    // Text mode: \text{...} -> "..."
    result = result.replace(/\\text\{([^{}]*)\}/g, '"$1"');

    // Remove remaining backslashes from simple commands
    result = result.replace(/\\([a-zA-Z]+)/g, '$1');

    return result.trim();
}

/**
 * Fallback Typst to LaTeX conversion (basic)
 */
function fallbackTypstToLatex(input) {
    let result = input;

    // Greek letters (reverse)
    const greekMap = {
        'alpha': '\\alpha', 'beta': '\\beta', 'gamma': '\\gamma', 'delta': '\\delta',
        'epsilon': '\\epsilon', 'zeta': '\\zeta', 'eta': '\\eta', 'theta': '\\theta',
        'iota': '\\iota', 'kappa': '\\kappa', 'lambda': '\\lambda', 'mu': '\\mu',
        'nu': '\\nu', 'xi': '\\xi', 'pi': '\\pi', 'rho': '\\rho',
        'sigma': '\\sigma', 'tau': '\\tau', 'upsilon': '\\upsilon', 'phi': '\\phi',
        'chi': '\\chi', 'psi': '\\psi', 'omega': '\\omega',
        'Gamma': '\\Gamma', 'Delta': '\\Delta', 'Theta': '\\Theta', 'Lambda': '\\Lambda',
        'Xi': '\\Xi', 'Pi': '\\Pi', 'Sigma': '\\Sigma', 'Phi': '\\Phi',
        'Psi': '\\Psi', 'Omega': '\\Omega',
    };

    const symbolMap = {
        'infinity': '\\infty', 'plus.minus': '\\pm', 'minus.plus': '\\mp',
        'times': '\\times', 'div': '\\div', 'dot': '\\cdot',
        '<=': '\\leq', '>=': '\\geq', '!=': '\\neq',
        'approx': '\\approx', 'equiv': '\\equiv',
        'subset': '\\subset', 'supset': '\\supset', 'subset.eq': '\\subseteq',
        'supset.eq': '\\supseteq', 'in': '\\in', 'in.not': '\\notin',
        'union': '\\cup', 'sect': '\\cap', 'emptyset': '\\emptyset',
        'forall': '\\forall', 'exists': '\\exists', 'not': '\\neg',
        '=>': '\\Rightarrow', '<=>': '\\Leftrightarrow',
        '->': '\\rightarrow', '<-': '\\leftarrow', '<->': '\\leftrightarrow',
        'diff': '\\partial', 'nabla': '\\nabla',
        'sum': '\\sum', 'product': '\\prod', 'integral': '\\int',
        'sin': '\\sin', 'cos': '\\cos', 'tan': '\\tan',
        'log': '\\log', 'ln': '\\ln', 'exp': '\\exp',
        'lim': '\\lim', 'max': '\\max', 'min': '\\min',
        '...': '\\ldots', 'dots.c': '\\cdots', 'dots.v': '\\vdots',
    };

    // Fractions: frac(a, b) -> \frac{a}{b}
    result = result.replace(/frac\(([^,]+),\s*([^)]+)\)/g, '\\frac{$1}{$2}');

    // Square root: sqrt(x) -> \sqrt{x}
    result = result.replace(/sqrt\(([^)]+)\)/g, '\\sqrt{$1}');

    // Apply symbols (longer first to avoid partial matches)
    const sortedSymbols = Object.entries(symbolMap).sort((a, b) => b[0].length - a[0].length);
    for (const [typst, tex] of sortedSymbols) {
        const escaped = typst.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        result = result.replace(new RegExp(`\\b${escaped}\\b`, 'g'), tex);
    }

    // Apply Greek letters
    for (const [typst, tex] of Object.entries(greekMap)) {
        result = result.replace(new RegExp(`\\b${typst}\\b`, 'g'), tex);
    }

    // Subscript/superscript with parentheses: _(...)  -> _{...}, ^(...) -> ^{...}
    result = result.replace(/_\(([^)]+)\)/g, '_{$1}');
    result = result.replace(/\^\(([^)]+)\)/g, '^{$1}');

    // Text: "..." -> \text{...}
    result = result.replace(/"([^"]+)"/g, '\\text{$1}');

    return result.trim();
}

/**
 * Fallback LaTeX document to Typst document conversion
 */
function fallbackLatexDocumentToTypst(input) {
    let result = input;

    // Remove document class and packages
    result = result.replace(/\\documentclass(\[.*?\])?\{.*?\}/g, '');
    result = result.replace(/\\usepackage(\[.*?\])?\{.*?\}/g, '');

    // Extract title, author, date
    const titleMatch = result.match(/\\title\{([^}]*)\}/);
    const authorMatch = result.match(/\\author\{([^}]*)\}/);

    // Remove preamble commands
    result = result.replace(/\\title\{[^}]*\}/g, '');
    result = result.replace(/\\author\{[^}]*\}/g, '');
    result = result.replace(/\\date\{[^}]*\}/g, '');
    result = result.replace(/\\maketitle/g, '');

    // Remove document environment
    result = result.replace(/\\begin\{document\}/g, '');
    result = result.replace(/\\end\{document\}/g, '');

    // Convert sections
    result = result.replace(/\\section\{([^}]*)\}/g, '= $1');
    result = result.replace(/\\subsection\{([^}]*)\}/g, '== $1');
    result = result.replace(/\\subsubsection\{([^}]*)\}/g, '=== $1');

    // Convert text formatting
    result = result.replace(/\\textbf\{([^}]*)\}/g, '*$1*');
    result = result.replace(/\\textit\{([^}]*)\}/g, '_$1_');
    result = result.replace(/\\emph\{([^}]*)\}/g, '_$1_');
    result = result.replace(/\\texttt\{([^}]*)\}/g, '`$1`');

    // Convert lists
    result = result.replace(/\\begin\{itemize\}/g, '');
    result = result.replace(/\\end\{itemize\}/g, '');
    result = result.replace(/\\begin\{enumerate\}/g, '');
    result = result.replace(/\\end\{enumerate\}/g, '');
    result = result.replace(/\\item\s*/g, '- ');

    // Convert citations and references
    result = result.replace(/\\cite\{([^}]*)\}/g, '@$1');
    result = result.replace(/\\ref\{([^}]*)\}/g, '@$1');
    result = result.replace(/\\label\{([^}]*)\}/g, '<$1>');

    // Convert math environments
    result = result.replace(/\$\$([^$]+)\$\$/g, '$ $1 $');
    result = result.replace(/\\\[([^\]]+)\\\]/g, '$ $1 $');

    // Apply math conversion to inline math
    result = result.replace(/\$([^$]+)\$/g, (match, math) => {
        return '$' + fallbackLatexToTypst(math) + '$';
    });

    // Build header
    let header = '';
    if (titleMatch || authorMatch) {
        header += '#set document(\n';
        if (titleMatch) header += `  title: "${titleMatch[1]}",\n`;
        if (authorMatch) header += `  author: "${authorMatch[1]}",\n`;
        header += ')\n\n';

        if (titleMatch) {
            header += '#align(center)[\n';
            header += `  #text(size: 2em, weight: "bold")[${titleMatch[1]}]\n`;
            if (authorMatch) {
                header += `  \n  ${authorMatch[1]}\n`;
            }
            header += ']\n\n';
        }
    }

    return (header + result).trim();
}

/**
 * Fallback Typst document to LaTeX document conversion
 */
function fallbackTypstDocumentToLatex(input) {
    let result = input;

    // Convert headings
    result = result.replace(/^=== (.+)$/gm, '\\subsubsection{$1}');
    result = result.replace(/^== (.+)$/gm, '\\subsection{$1}');
    result = result.replace(/^= (.+)$/gm, '\\section{$1}');

    // Convert text formatting
    result = result.replace(/\*([^*]+)\*/g, '\\textbf{$1}');
    result = result.replace(/_([^_]+)_/g, '\\textit{$1}');
    result = result.replace(/`([^`]+)`/g, '\\texttt{$1}');

    // Convert lists
    result = result.replace(/^- (.+)$/gm, '\\item $1');
    result = result.replace(/^\+ (.+)$/gm, '\\item $1');

    // Convert citations and references
    result = result.replace(/@([a-zA-Z0-9_-]+)/g, '\\cite{$1}');
    result = result.replace(/<([a-zA-Z0-9_-]+)>/g, '\\label{$1}');

    // Convert display math
    result = result.replace(/\$ ([^$]+) \$/g, (match, math) => {
        return '\\[' + fallbackTypstToLatex(math) + '\\]';
    });

    // Convert inline math
    result = result.replace(/\$([^$]+)\$/g, (match, math) => {
        return '$' + fallbackTypstToLatex(math) + '$';
    });

    // Wrap in document
    const doc = `\\documentclass{article}
\\usepackage{amsmath}
\\usepackage{amssymb}

\\begin{document}

${result.trim()}

\\end{document}`;

    return doc;
}

// ===== UI Functions =====

function updateDirection() {
    if (state.direction === 'l2t') {
        elements.leftLabel.textContent = 'LaTeX';
        elements.rightLabel.textContent = 'Typst';
    } else {
        elements.leftLabel.textContent = 'Typst';
        elements.rightLabel.textContent = 'LaTeX';
    }
    // 箭头始终指向右边（→），表示 输入 → 输出
    elements.directionToggle.classList.remove('reversed');
    updateTitles();
}

function updateMode() {
    if (elements.modeMath && elements.modeDocument) {
        if (state.mode === 'math') {
            elements.modeMath.classList.add('active');
            elements.modeDocument.classList.remove('active');
        } else {
            elements.modeMath.classList.remove('active');
            elements.modeDocument.classList.add('active');
        }
    }
    updateTitles();
}

function updateTitles() {
    const t = translations[state.lang];
    const modeLabel = state.mode === 'math' ? t.modeMath : t.modeDocument;

    if (state.direction === 'l2t') {
        elements.inputTitle.textContent = state.lang === 'zh' ? `LaTeX ${modeLabel}输入` : `LaTeX ${modeLabel} Input`;
        elements.outputTitle.textContent = state.lang === 'zh' ? `Typst ${modeLabel}输出` : `Typst ${modeLabel} Output`;

        if (state.mode === 'math') {
            elements.inputEditor.placeholder = t.placeholderMathLatex;
        } else {
            elements.inputEditor.placeholder = t.placeholderDocLatex;
        }
    } else {
        elements.inputTitle.textContent = state.lang === 'zh' ? `Typst ${modeLabel}输入` : `Typst ${modeLabel} Input`;
        elements.outputTitle.textContent = state.lang === 'zh' ? `LaTeX ${modeLabel}输出` : `LaTeX ${modeLabel} Output`;

        if (state.mode === 'math') {
            elements.inputEditor.placeholder = t.placeholderMathTypst;
        } else {
            elements.inputEditor.placeholder = t.placeholderDocTypst;
        }
    }
}

function swapDirection() {
    // Swap the content
    const inputValue = elements.inputEditor.value;
    const outputValue = elements.outputEditor.value;

    // Change direction
    state.direction = state.direction === 'l2t' ? 't2l' : 'l2t';
    updateDirection();

    // Swap content
    elements.inputEditor.value = outputValue;
    elements.outputEditor.value = inputValue;

    // Trigger conversion
    convert();
}

function setMode(mode) {
    state.mode = mode;
    updateMode();
    convert();
}

function convert() {
    const input = elements.inputEditor.value.trim();
    const t = translations[state.lang];

    if (!input) {
        elements.outputEditor.value = '';
        elements.charCount.textContent = `0 ${t.chars}`;
        elements.convertTime.textContent = t.ready;
        return;
    }

    const startTime = performance.now();

    try {
        let output;
        if (state.mode === 'math') {
            // Math mode
            if (state.direction === 'l2t') {
                output = latexToTypst(input);
            } else {
                output = typstToLatex(input);
            }
        } else {
            // Document mode
            if (state.direction === 'l2t') {
                output = latexDocumentToTypst(input);
            } else {
                output = typstDocumentToLatex(input);
            }
        }

        elements.outputEditor.value = output;

        const endTime = performance.now();
        const duration = (endTime - startTime).toFixed(2);

        elements.charCount.textContent = `${input.length} ${t.chars}`;
        elements.convertTime.textContent = `${t.time}: ${duration}ms`;
    } catch (e) {
        elements.outputEditor.value = `Error: ${e.message}`;
        elements.convertTime.textContent = t.failed;
    }
}

function showToast(message, duration = 2000) {
    elements.toastMessage.textContent = message;
    elements.toast.classList.add('show');

    setTimeout(() => {
        elements.toast.classList.remove('show');
    }, duration);
}

async function copyOutput() {
    const output = elements.outputEditor.value;
    const t = translations[state.lang];
    if (!output) {
        showToast(t.noContent);
        return;
    }

    try {
        await navigator.clipboard.writeText(output);
        showToast(t.copied);
    } catch (e) {
        showToast(t.copyFailed);
    }
}

async function pasteInput() {
    const t = translations[state.lang];
    try {
        const text = await navigator.clipboard.readText();
        elements.inputEditor.value = text;
        convert();
        showToast(t.pasted);
    } catch (e) {
        showToast(t.pasteFailed);
    }
}

function clearInput() {
    const t = translations[state.lang];
    elements.inputEditor.value = '';
    elements.outputEditor.value = '';
    elements.charCount.textContent = `0 ${t.chars}`;
    elements.convertTime.textContent = t.ready;
}

function toggleTheme() {
    const html = document.documentElement;
    const currentTheme = html.getAttribute('data-theme');
    const newTheme = currentTheme === 'light' ? 'dark' : 'light';

    html.setAttribute('data-theme', newTheme);
    localStorage.setItem('theme', newTheme);
}

function loadTheme() {
    const savedTheme = localStorage.getItem('theme');
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const theme = savedTheme || (prefersDark ? 'dark' : 'light');

    document.documentElement.setAttribute('data-theme', theme);
}

function toggleLanguage() {
    state.lang = state.lang === 'zh' ? 'en' : 'zh';
    localStorage.setItem('lang', state.lang);
    updateLanguageUI();
}

function loadLanguage() {
    const savedLang = localStorage.getItem('lang');
    // Default to browser language or 'zh'
    const browserLang = navigator.language.startsWith('zh') ? 'zh' : 'en';
    state.lang = savedLang || browserLang;
    updateLanguageUI();
}

function updateLanguageUI() {
    const t = translations[state.lang];
    const isEn = state.lang === 'en';

    // Update HTML elements
    document.documentElement.lang = isEn ? 'en' : 'zh-CN';
    document.title = t.title;
    const metaDesc = document.querySelector('meta[name="description"]');
    if (metaDesc) metaDesc.content = t.description;
    
    // Toggle button text
    const langText = document.querySelector('.lang-text');
    if (langText) langText.textContent = isEn ? '中' : 'EN';
    
    // Mode buttons
    const modeMathSpan = elements.modeMath.querySelector('span');
    const modeDocSpan = elements.modeDocument.querySelector('span');
    if (modeMathSpan) modeMathSpan.textContent = t.modeMath;
    if (modeDocSpan) modeDocSpan.textContent = t.modeDocument;
    
    // Output placeholder
    elements.outputEditor.placeholder = t.placeholderOutput;

    // Tooltips
    elements.themeToggle.title = t.theme;
    elements.langToggle.title = t.lang;
    elements.swapBtn.title = t.swap;
    elements.clearInputBtn.title = t.clear;
    elements.pasteBtn.title = t.paste;
    elements.copyBtn.title = t.copy;
    
    // Hint
    const hintText = document.querySelector('.hint-text');
    if (hintText) hintText.innerHTML = t.hint;

    // Trigger title update
    updateTitles();
}

function loadExample(latex) {
    // Switch to math mode for examples
    state.mode = 'math';
    updateMode();

    // If we're in t2l mode, convert to typst first
    if (state.direction === 't2l') {
        elements.inputEditor.value = latexToTypst(latex);
    } else {
        elements.inputEditor.value = latex;
    }
    convert();

    // Scroll to editor
    elements.inputEditor.scrollIntoView({ behavior: 'smooth', block: 'center' });
    elements.inputEditor.focus();
}

function loadDocumentExample(example) {
    // Switch to document mode
    state.mode = 'document';
    updateMode();

    elements.inputEditor.value = example;
    convert();

    // Scroll to editor
    elements.inputEditor.scrollIntoView({ behavior: 'smooth', block: 'center' });
    elements.inputEditor.focus();
}

// ===== Event Listeners =====

function setupEventListeners() {
    // Input handling with debounce
    let debounceTimeout;
    elements.inputEditor.addEventListener('input', () => {
        clearTimeout(debounceTimeout);
        debounceTimeout = setTimeout(convert, 100);
    });

    // Direction controls
    elements.swapBtn.addEventListener('click', swapDirection);
    elements.directionToggle.addEventListener('click', swapDirection);

    // Mode toggle
    if (elements.modeMath) {
        elements.modeMath.addEventListener('click', () => setMode('math'));
    }
    if (elements.modeDocument) {
        elements.modeDocument.addEventListener('click', () => setMode('document'));
    }

    // Action buttons
    elements.clearInputBtn.addEventListener('click', clearInput);
    elements.pasteBtn.addEventListener('click', pasteInput);
    elements.copyBtn.addEventListener('click', copyOutput);
    elements.themeToggle.addEventListener('click', toggleTheme);
    elements.langToggle.addEventListener('click', toggleLanguage); // Add this

    // Example cards (math)
    document.querySelectorAll('.example-card[data-latex]').forEach(card => {
        card.addEventListener('click', () => {
            const latex = card.getAttribute('data-latex');
            loadExample(latex);
        });
    });

    // Example cards (document)
    document.querySelectorAll('.example-card[data-document]').forEach(card => {
        card.addEventListener('click', () => {
            const doc = card.getAttribute('data-document');
            loadDocumentExample(doc);
        });
    });

    // Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
        // Ctrl/Cmd + Enter to convert
        if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
            convert();
        }
        // Ctrl/Cmd + Shift + C to copy output
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'C') {
            e.preventDefault();
            copyOutput();
        }
        // Ctrl/Cmd + Shift + V to paste
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'V') {
            e.preventDefault();
            pasteInput();
        }
        // Ctrl/Cmd + M to toggle mode
        if ((e.ctrlKey || e.metaKey) && e.key === 'm') {
            e.preventDefault();
            setMode(state.mode === 'math' ? 'document' : 'math');
        }
    });
}

// ===== Initialization =====

async function init() {
    loadTheme();
    loadLanguage(); // Add this
    updateDirection();
    updateMode();
    setupEventListeners();

    // Load WASM
    await initWasm();

    // Check for URL params
    const params = new URLSearchParams(window.location.search);
    const input = params.get('input');
    const dir = params.get('dir');
    const mode = params.get('mode');

    if (dir === 't2l') {
        state.direction = 't2l';
        updateDirection();
    }

    if (mode === 'document') {
        state.mode = 'document';
        updateMode();
    }

    if (input) {
        elements.inputEditor.value = decodeURIComponent(input);
        convert();
    }
}

// Start the app
init();
