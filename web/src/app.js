/**
 * Tylax Web App
 * LaTeX ↔ Typst bidirectional converter
 */

// Import KaTeX for math rendering
import katex from 'katex';
import 'katex/dist/katex.min.css';

// Import CeTZ renderer for graphics preview
import { renderGraphicsToSVG, hasRenderableGraphics } from './cetz-renderer.js';

// State
const state = {
    direction: 't2l', // 'l2t' = LaTeX to Typst, 't2l' = Typst to LaTeX (默认 Typst → LaTeX)
    mode: 'math', // 'math' = Math formula, 'document' = Full document
    lang: 'zh', // 'zh' or 'en'
    wasmReady: false,
    wasm: null,
    previewEnabled: true, // Preview panel state
    previewType: 'math', // 'math', 'table', 'graphics', 'document', 'none'
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
        preview: '预览',
        previewPlaceholder: '输入内容后，预览将显示在这里',
        previewError: '预览渲染失败',
        previewToggle: '切换预览',
        previewInputLabel: '输入预览 (INPUT)',
        previewOutputLabel: '输出预览 (OUTPUT)',
        betaBadge: '(实验功能)',
        typstNotPreviewable: '无法直接预览 Typst 输出',
        typstNotPreviewableHint: '请查看输出框中的 Typst 代码',
        previewNotAvailable: '无法预览',
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
        preview: 'Preview',
        previewPlaceholder: 'Preview will appear here after entering content',
        previewError: 'Preview rendering failed',
        previewToggle: 'Toggle Preview',
        previewInputLabel: 'Input Preview (INPUT)',
        previewOutputLabel: 'Output Preview (OUTPUT)',
        betaBadge: '(Experimental)',
        typstNotPreviewable: 'Cannot preview Typst output directly',
        typstNotPreviewableHint: 'Please check the Typst code in the output box',
        previewNotAvailable: 'Preview not available',
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
    // Preview elements
    previewPanel: document.getElementById('previewPanel'),
    previewTitle: document.getElementById('previewTitle'),
    previewToggleBtn: document.getElementById('previewToggleBtn'),
    previewContent: document.getElementById('previewContent'),
    previewPlaceholder: document.getElementById('previewPlaceholder'),
    previewSplitView: document.getElementById('previewSplitView'),
    // Split view elements
    previewMathInput: document.getElementById('previewMathInput'),
    previewTableInput: document.getElementById('previewTableInput'),
    previewMathOutput: document.getElementById('previewMathOutput'),
    previewTableOutput: document.getElementById('previewTableOutput'),
    previewInputLabel: document.getElementById('previewInputLabel'),
    previewOutputLabel: document.getElementById('previewOutputLabel'),
    betaBadge: document.querySelector('.badge-beta'),
    
    previewError: document.getElementById('previewError'),
};

// ===== Debug Mode =====
// Set to true to enable debug logging (or use import.meta.env.DEV in Vite)
const DEBUG = import.meta.env?.DEV || false;

// ===== WASM Loading =====
async function initWasm() {
    try {
        const wasm = await import(/* @vite-ignore */ './pkg/tylax.js');
        await wasm.default();
        state.wasm = wasm;
        state.wasmReady = true;
        if (DEBUG) console.log('WASM loaded:', Object.keys(wasm));
        showToast(translations[state.lang].wasmLoaded);
    } catch (e) {
        console.error('WASM loading failed:', e);
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

// ===== Preview Functions =====

/**
 * Detect if input is a LaTeX table environment
 */
function isLatexTable(input) {
    return input.includes('\\begin{tabular}') ||
           input.includes('\\begin{table}') ||
           input.includes('\\begin{longtable}') ||
           input.includes('\\begin{tabularx}') ||
           input.includes('\\begin{array}');
}

/**
 * Detect if input is a Typst table
 */
function isTypstTable(input) {
    return input.includes('#table(') || 
           input.includes('table(') ||
           /^\s*table\s*\(/.test(input);
}

/**
 * Detect if input is TikZ graphics (LaTeX)
 */
function isLatexGraphics(input) {
    return input.includes('\\begin{tikzpicture}') ||
           input.includes('\\begin{pgfpicture}') ||
           input.includes('\\tikz') ||
           input.includes('\\draw') ||
           input.includes('\\fill') ||
           input.includes('\\node') ||
           input.includes('\\path');
}

/**
 * Detect if input is CeTZ graphics (Typst)
 */
function isTypstGraphics(input) {
    return input.includes('#canvas(') ||
           input.includes('canvas(') ||
           input.includes('#import "@preview/cetz') ||
           input.includes('draw.line(') ||
           input.includes('draw.circle(') ||
           input.includes('draw.rect(') ||
           input.includes('line(') && input.includes('stroke:');
}

/**
 * Detect if input is a full LaTeX document
 */
function isLatexDocument(input) {
    return input.includes('\\documentclass') ||
           input.includes('\\begin{document}') ||
           input.includes('\\usepackage') ||
           input.includes('\\section') ||
           input.includes('\\chapter') ||
           input.includes('\\title{') ||
           input.includes('\\maketitle');
}

/**
 * Detect if input is a full Typst document
 * EXCLUDES CeTZ graphics imports to avoid misclassification
 */
function isTypstDocument(input) {
    // First check: if it's pure CeTZ graphics, it's NOT a document
    if (isPureCeTZ(input) || isTypstGraphics(input)) {
        return false;
    }
    
    return input.includes('#set document') ||
           input.includes('#set page') ||
           input.includes('#set text') ||
           /^=\s+\w/.test(input) || // Heading like "= Title"
           (input.includes('#import') && !input.includes('cetz')) || // Import but not CeTZ
           input.includes('#show:') ||
           input.includes('#let ');
}

/**
 * Detect the type of content for preview
 * Priority: Graphics > Table > Document > Math
 * 
 * Note: Graphics (TikZ/CeTZ) is checked FIRST because:
 * - CeTZ uses #import which could be confused with document
 * - TikZ is often standalone and should render as graphics
 * 
 * @returns 'graphics' | 'table' | 'document' | 'math' | 'none'
 */
function detectPreviewType(input) {
    if (!input || input.trim() === '') {
        return 'none';
    }
    
    const isLatexInput = state.direction === 'l2t';

    // 1. Check for graphics FIRST (TikZ/CeTZ)
    // Must be before document check because CeTZ uses #import
    if (isLatexInput) {
        if (isLatexGraphics(input)) return 'graphics';
    } else {
        if (isTypstGraphics(input)) return 'graphics';
    }
    
    // 2. Check for tables
    if (isLatexInput) {
        if (isLatexTable(input)) return 'table';
    } else {
        if (isTypstTable(input)) return 'table';
    }
    
    // 3. Check for full documents
    if (isLatexInput) {
        if (isLatexDocument(input)) return 'document';
    } else {
        if (isTypstDocument(input)) return 'document';
    }
    
    // 4. Default to math preview for formula mode
    if (state.mode === 'math') {
        return 'math';
    }
    
    // 5. Fallback for document mode without clear structure
    if (state.mode === 'document') {
        return 'document';
    }
    
    return 'none';
}

// ===== Math Preprocessing Functions =====

/**
 * Smart wrap LaTeX to fix KaTeX alignment errors
 * Detects bare alignment characters (&, \\) and wraps in aligned environment
 * @param {string} latex - Raw LaTeX string
 * @returns {string} - Properly wrapped LaTeX
 */
function smartWrapLatex(latex) {
    if (!latex || typeof latex !== 'string') return latex;
    
    const trimmed = latex.trim();
    
    // Check if already wrapped in an environment
    const hasEnvironment = /\\begin\{(aligned|align|array|matrix|pmatrix|bmatrix|cases|gather|equation|eqnarray|split)\}/i.test(trimmed);
    if (hasEnvironment) {
        return trimmed;
    }
    
    // Check if contains alignment characters that need wrapping
    const hasAlignment = trimmed.includes('&');
    const hasLineBreak = /\\\\/.test(trimmed);
    
    // If has alignment or line breaks, wrap in aligned environment
    if (hasAlignment || hasLineBreak) {
        return `\\begin{aligned} ${trimmed} \\end{aligned}`;
    }
    
    return trimmed;
}

/**
 * Render math formula preview using KaTeX (Split View)
 * @param {string} inputLatex - LaTeX for input pane
 * @param {string} outputLatex - LaTeX for output pane
 * @param {boolean} inputConvertFailed - If true, show "cannot preview" message for input
 * @param {boolean} outputConvertFailed - If true, show "cannot preview" message for output
 */
function renderMathPreview(inputLatex, outputLatex, inputConvertFailed = false, outputConvertFailed = false) {
    const t = translations[state.lang];
    const previewSplitView = elements.previewSplitView;
    const previewMathInput = elements.previewMathInput;
    const previewMathOutput = elements.previewMathOutput;
    const previewTableInput = elements.previewTableInput;
    const previewTableOutput = elements.previewTableOutput;
    const previewError = elements.previewError;
    const previewPlaceholder = elements.previewPlaceholder;
    
    // If nothing to preview and no conversion happened
    const hasInput = inputLatex && inputLatex.trim() !== '';
    const hasOutput = outputLatex && outputLatex.trim() !== '';
    if (!hasInput && !hasOutput && !inputConvertFailed && !outputConvertFailed) {
        showPreviewPlaceholder();
        return;
    }
    
    // Setup UI for Math Split View
    previewPlaceholder.style.display = 'none';
    previewError.classList.remove('active');
    previewSplitView.classList.add('active');
    
    // Show Math containers, hide Table containers
    previewMathInput.style.display = 'flex';
    previewMathOutput.style.display = 'flex';
    previewTableInput.style.display = 'none';
    previewTableOutput.style.display = 'none';
    
    // Message for when preview is not available
    const cannotPreviewMsg = (isTypst) => `
        <div style="color: var(--text-muted); font-size: 0.85rem; text-align: center; padding: 1rem;">
            <div>${isTypst ? t.typstNotPreviewable : t.previewNotAvailable}</div>
            <div style="font-size: 0.75rem; opacity: 0.7; margin-top: 0.3rem;">${isTypst ? t.typstNotPreviewableHint : ''}</div>
        </div>
    `;
    
    // Helper to render to a specific container
    const renderToContainer = (latex, container, convertFailed = false) => {
        if (convertFailed) {
            container.innerHTML = cannotPreviewMsg(state.direction === 'l2t');
            return;
        }
        if (!latex) {
            container.innerHTML = '';
            return;
        }
        try {
            // Clean up the LaTeX for KaTeX
            let cleanLatex = latex.trim();
            // Remove display math delimiters if present
            cleanLatex = cleanLatex.replace(/^\$\$/, '').replace(/\$\$$/, '');
            cleanLatex = cleanLatex.replace(/^\\\[/, '').replace(/\\\]$/, '');
            cleanLatex = cleanLatex.replace(/^\$/, '').replace(/\$$/, '');
            
            // Smart wrap: auto-add aligned environment for bare alignments
            cleanLatex = smartWrapLatex(cleanLatex);
            
            katex.render(cleanLatex, container, {
                displayMode: true,
                throwOnError: false,
                errorColor: '#f85149',
                trust: true,
                strict: false,
                macros: {
                    "\\RR": "\\mathbb{R}",
                    "\\NN": "\\mathbb{N}",
                    "\\ZZ": "\\mathbb{Z}",
                    "\\QQ": "\\mathbb{Q}",
                    "\\CC": "\\mathbb{C}",
                }
            });
        } catch (e) {
            console.error('KaTeX render error:', e);
            container.innerHTML = `<span style="color: var(--error); font-size: 0.8rem;">Render Error</span>`;
        }
    };

    // Render Input (Left)
    renderToContainer(inputLatex, previewMathInput, inputConvertFailed);
    
    // Render Output (Right)
    renderToContainer(outputLatex, previewMathOutput, outputConvertFailed);
    
    state.previewType = 'math';
}

/**
 * Render table preview from structured data (Split View)
 */
function renderTablePreview(tableData) {
    const previewSplitView = elements.previewSplitView;
    const previewTableInput = elements.previewTableInput;
    const previewTableOutput = elements.previewTableOutput;
    const previewMathInput = elements.previewMathInput;
    const previewMathOutput = elements.previewMathOutput;
    const previewPlaceholder = elements.previewPlaceholder;
    const previewError = elements.previewError;
    
    // Handle error response from WASM
    if (tableData && tableData.error) {
        showPreviewError(tableData.error);
        return;
    }
    
    if (!tableData || !tableData.rows) {
        showPreviewPlaceholder();
        return;
    }
    
    previewPlaceholder.style.display = 'none';
    previewError.classList.remove('active');
    previewSplitView.classList.add('active');
    
    // Show Table containers, hide Math
    previewTableInput.style.display = 'block';
    previewTableOutput.style.display = 'block';
    previewMathInput.style.display = 'none';
    previewMathOutput.style.display = 'none';
    
    try {
        const tableHtml = generateTableHtml(tableData);
        previewTableInput.innerHTML = tableHtml;
        
        // Show source format indicator in right pane
        const direction = state.direction === 'l2t' ? 'Typst' : 'LaTeX';
        previewTableOutput.innerHTML = `
            <div style="text-align: center; color: var(--text-muted); padding: 20px; font-size: 0.8rem;">
                <p style="margin-bottom: 8px;">表格结构已解析</p>
                <p>${tableData.column_count} 列 × ${tableData.rows.length} 行</p>
            </div>
        `;
        
        state.previewType = 'table';
        
    } catch (e) {
        console.error('Table render error:', e);
        showPreviewError(e.message || 'Table render failed');
    }
}

/**
 * Generate HTML table from WASM TablePreviewData
 * Structure: { rows: [{ cells: [{ content, colspan, rowspan, align, is_header }], has_bottom_border }], has_header, column_count, default_alignments }
 */
function generateTableHtml(tableData) {
    if (!tableData || !tableData.rows || tableData.rows.length === 0) {
        return '<p style="color: var(--text-muted);">Empty table</p>';
    }
    
    let html = '<table class="preview-table-content">';
    
    for (let rowIdx = 0; rowIdx < tableData.rows.length; rowIdx++) {
        const row = tableData.rows[rowIdx];
        const rowHasBottomBorder = row.has_bottom_border;
        
        html += `<tr${rowHasBottomBorder ? ' class="border-bottom"' : ''}>`;
        
        for (let cellIdx = 0; cellIdx < row.cells.length; cellIdx++) {
            const cell = row.cells[cellIdx];
            const tag = cell.is_header ? 'th' : 'td';
            
            // Map alignment - handle enum values from Rust
            let alignClass = '';
            if (cell.align) {
                const alignStr = typeof cell.align === 'string' ? cell.align : cell.align.toString();
                if (alignStr === 'Left' || alignStr === 'left') alignClass = 'align-left';
                else if (alignStr === 'Center' || alignStr === 'center') alignClass = 'align-center';
                else if (alignStr === 'Right' || alignStr === 'right') alignClass = 'align-right';
            } else if (tableData.default_alignments && tableData.default_alignments[cellIdx]) {
                // Use default column alignment
                const defaultAlign = tableData.default_alignments[cellIdx];
                const alignStr = typeof defaultAlign === 'string' ? defaultAlign : defaultAlign.toString();
                if (alignStr === 'Left' || alignStr === 'left') alignClass = 'align-left';
                else if (alignStr === 'Center' || alignStr === 'center') alignClass = 'align-center';
                else if (alignStr === 'Right' || alignStr === 'right') alignClass = 'align-right';
            }
            
            const colspanAttr = cell.colspan > 1 ? ` colspan="${cell.colspan}"` : '';
            const rowspanAttr = cell.rowspan > 1 ? ` rowspan="${cell.rowspan}"` : '';
            
            // Render cell content with rich text support
            const content = renderCellContent(cell.content || '');
            
            html += `<${tag} class="${alignClass}"${colspanAttr}${rowspanAttr}>${content}</${tag}>`;
        }
        html += '</tr>';
    }
    
    html += '</table>';
    return html;
}

/**
 * Render cell content with rich text (math, bold, italic, symbols)
 * Supports both LaTeX and Typst syntax
 * @param {string} content - Raw cell content
 * @returns {string} - HTML rendered content
 */
function renderCellContent(content) {
    if (!content) return '';
    
    let result = content;
    
    // 1. Convert Typst symbols to Unicode/HTML (before other processing)
    const typstSymbols = {
        'arrow.b': '↓',
        'arrow.t': '↑',
        'arrow.l': '←',
        'arrow.r': '→',
        'arrow.lr': '↔',
        'arrow.tb': '↕',
        'checkmark': '✓',
        'times': '×',
        'plus.minus': '±',
        'minus.plus': '∓',
        'dots': '…',
        'dots.h': '⋯',
        'dots.v': '⋮',
        'infinity': '∞',
        'approx': '≈',
        'neq': '≠',
        'leq': '≤',
        'geq': '≥',
        'sum': '∑',
        'product': '∏',
        'integral': '∫',
    };
    
    // Replace Typst symbols (inside $ or standalone)
    for (const [symbol, replacement] of Object.entries(typstSymbols)) {
        // Replace $symbol$ pattern
        result = result.replace(new RegExp(`\\$${symbol.replace('.', '\\.')}\\$`, 'g'), replacement);
        // Replace standalone symbol (word boundary)
        result = result.replace(new RegExp(`\\b${symbol.replace('.', '\\.')}\\b`, 'g'), replacement);
    }
    
    // 2. Render LaTeX math: $...$
    if (result.includes('$') && typeof katex !== 'undefined') {
        result = result.replace(/\$([^$]+)\$/g, (match, math) => {
            try {
                return katex.renderToString(math, { 
                    throwOnError: false,
                    displayMode: false 
                });
            } catch {
                return match;
            }
        });
    }
    
    // 3. Handle LaTeX text formatting
    // \textbf{...} -> <b>...</b>
    result = result.replace(/\\textbf\{([^}]+)\}/g, '<b>$1</b>');
    // \textit{...} -> <i>...</i>
    result = result.replace(/\\textit\{([^}]+)\}/g, '<i>$1</i>');
    // \emph{...} -> <em>...</em>
    result = result.replace(/\\emph\{([^}]+)\}/g, '<em>$1</em>');
    // \underline{...} -> <u>...</u>
    result = result.replace(/\\underline\{([^}]+)\}/g, '<u>$1</u>');
    
    // 4. Handle Typst text formatting
    // *text* -> <b>text</b> (bold)
    // Must not match ** which could be empty or escaped
    result = result.replace(/(?<!\*)\*([^*]+)\*(?!\*)/g, '<b>$1</b>');
    // _text_ -> <i>text</i> (italic) - be careful not to match underscores in words
    result = result.replace(/(?<![a-zA-Z0-9])_([^_]+)_(?![a-zA-Z0-9])/g, '<i>$1</i>');
    
    // 5. Escape any remaining HTML-unsafe characters (except our tags)
    // This is a simplified version - we preserve our generated tags
    result = result.replace(/</g, '&lt;').replace(/>/g, '&gt;');
    // Restore our HTML tags
    result = result.replace(/&lt;(\/?(b|i|em|u|span)[^&]*)&gt;/g, '<$1>');
    // Restore KaTeX output (which contains lots of HTML)
    result = result.replace(/&lt;(span class="katex[^&]*)&gt;/g, '<$1>');
    
    return result;
}

// ===== Document Outline Functions =====

/**
 * Extract document outline (headings) from LaTeX or Typst content
 * @param {string} input - Document content
 * @param {string} format - 'latex' or 'typst'
 * @returns {Array} Array of { level, title, type } objects
 */
function extractDocumentOutline(input, format) {
    const outline = [];
    
    if (format === 'latex') {
        // LaTeX heading patterns
        const patterns = [
            { regex: /\\chapter\{([^}]+)\}/g, level: 1, type: 'chapter' },
            { regex: /\\section\{([^}]+)\}/g, level: 2, type: 'section' },
            { regex: /\\subsection\{([^}]+)\}/g, level: 3, type: 'subsection' },
            { regex: /\\subsubsection\{([^}]+)\}/g, level: 4, type: 'subsubsection' },
            { regex: /\\paragraph\{([^}]+)\}/g, level: 5, type: 'paragraph' },
        ];
        
        // Also extract document info
        const titleMatch = input.match(/\\title\{([^}]+)\}/);
        if (titleMatch) {
            outline.push({ level: 0, title: titleMatch[1], type: 'title' });
        }
        
        const authorMatch = input.match(/\\author\{([^}]+)\}/);
        if (authorMatch) {
            outline.push({ level: 0, title: `Author: ${authorMatch[1]}`, type: 'author' });
        }
        
        // Extract all headings in order
        const allMatches = [];
        for (const pattern of patterns) {
            let match;
            while ((match = pattern.regex.exec(input)) !== null) {
                allMatches.push({
                    index: match.index,
                    level: pattern.level,
                    title: match[1],
                    type: pattern.type
                });
            }
        }
        
        // Sort by position in document
        allMatches.sort((a, b) => a.index - b.index);
        outline.push(...allMatches);
        
    } else if (format === 'typst') {
        // Typst heading patterns: = Heading, == Subheading, etc.
        const lines = input.split('\n');
        
        for (const line of lines) {
            // Match headings: = Title, == Section, === Subsection, etc.
            const headingMatch = line.match(/^(=+)\s+(.+)$/);
            if (headingMatch) {
                const level = headingMatch[1].length;
                outline.push({
                    level,
                    title: headingMatch[2].trim(),
                    type: level === 1 ? 'title' : `heading-${level}`
                });
            }
            
            // Match #set document(title: "...")
            const docTitleMatch = line.match(/#set\s+document\s*\(\s*title\s*:\s*"([^"]+)"/);
            if (docTitleMatch) {
                outline.unshift({ level: 0, title: docTitleMatch[1], type: 'title' });
            }
        }
    }
    
    return outline;
}

/**
 * Render document outline as HTML
 * @param {Array} outline - Array of { level, title, type } objects
 * @returns {string} HTML string
 */
function renderDocumentOutlineHtml(outline) {
    if (!outline || outline.length === 0) {
        return '<div class="outline-empty">No structure detected</div>';
    }
    
    let html = '<div class="document-outline">';
    html += '<h4 class="outline-title">📄 Document Structure</h4>';
    html += '<ul class="outline-list">';
    
    for (const item of outline) {
        const indent = item.level * 16;
        const typeClass = `outline-${item.type}`;
        const icon = getOutlineIcon(item.type);
        
        html += `<li class="outline-item ${typeClass}" style="padding-left: ${indent}px">`;
        html += `<span class="outline-icon">${icon}</span>`;
        html += `<span class="outline-text">${escapeHtml(item.title)}</span>`;
        html += '</li>';
    }
    
    html += '</ul></div>';
    return html;
}

/**
 * Get icon for outline item type
 */
function getOutlineIcon(type) {
    switch (type) {
        case 'title': return '📖';
        case 'author': return '👤';
        case 'chapter': return '📑';
        case 'section': return '§';
        case 'subsection': return '•';
        case 'subsubsection': return '◦';
        case 'paragraph': return '¶';
        default: return '•';
    }
}

/**
 * Escape HTML special characters
 */
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

/**
 * Render document preview (outline mode)
 */
function renderDocumentPreview(input, output) {
    const previewSplitView = elements.previewSplitView;
    const previewMathInput = elements.previewMathInput;
    const previewMathOutput = elements.previewMathOutput;
    const previewTableInput = elements.previewTableInput;
    const previewTableOutput = elements.previewTableOutput;
    const previewPlaceholder = elements.previewPlaceholder;
    const previewError = elements.previewError;
    
    if (!input || input.trim() === '') {
        showPreviewPlaceholder();
        return;
    }
    
    // Setup UI for Document Preview (reuse table containers for outline)
    previewPlaceholder.style.display = 'none';
    previewError.classList.remove('active');
    previewSplitView.classList.add('active');
    
    // Use table containers for document outline
    previewTableInput.style.display = 'block';
    previewTableOutput.style.display = 'block';
    previewMathInput.style.display = 'none';
    previewMathOutput.style.display = 'none';
    
    // Extract and render input outline
    const inputFormat = state.direction === 'l2t' ? 'latex' : 'typst';
    const inputOutline = extractDocumentOutline(input, inputFormat);
    previewTableInput.innerHTML = renderDocumentOutlineHtml(inputOutline);
    
    // Extract and render output outline
    if (output && output.trim()) {
        const outputFormat = state.direction === 'l2t' ? 'typst' : 'latex';
        const outputOutline = extractDocumentOutline(output, outputFormat);
        previewTableOutput.innerHTML = renderDocumentOutlineHtml(outputOutline);
    } else {
        previewTableOutput.innerHTML = '<div class="outline-empty">Waiting for conversion...</div>';
    }
    
    state.previewType = 'document';
}

// ===== Graphics Preview Functions =====

/**
 * Render graphics preview using SVG
 * Handles TikZ (LaTeX) and CeTZ (Typst) graphics
 */
function renderGraphicsPreview(input, output) {
    const previewSplitView = elements.previewSplitView;
    const previewMathInput = elements.previewMathInput;
    const previewMathOutput = elements.previewMathOutput;
    const previewTableInput = elements.previewTableInput;
    const previewTableOutput = elements.previewTableOutput;
    const previewPlaceholder = elements.previewPlaceholder;
    const previewError = elements.previewError;
    
    if (!input || input.trim() === '') {
        showPreviewPlaceholder();
        return;
    }
    
    // Setup UI for Graphics Preview (reuse table containers for SVG)
    previewPlaceholder.style.display = 'none';
    previewError.classList.remove('active');
    previewSplitView.classList.add('active');
    
    // Use table containers for graphics
    previewTableInput.style.display = 'block';
    previewTableOutput.style.display = 'block';
    previewMathInput.style.display = 'none';
    previewMathOutput.style.display = 'none';
    
    try {
        // Render input graphics
        const inputSvg = renderGraphicsToSVG(input);
        previewTableInput.innerHTML = `
            <div class="graphics-preview">
                <div class="graphics-label">🎨 Input Graphics</div>
                ${inputSvg}
            </div>
        `;
        
        // Render output graphics if available
        if (output && output.trim()) {
            const outputSvg = renderGraphicsToSVG(output);
            previewTableOutput.innerHTML = `
                <div class="graphics-preview">
                    <div class="graphics-label">🎨 Output Graphics</div>
                    ${outputSvg}
                </div>
            `;
        } else {
            previewTableOutput.innerHTML = `
                <div class="graphics-preview">
                    <div class="graphics-label">🎨 Output Graphics</div>
                    <div class="graphics-placeholder">Waiting for conversion...</div>
                </div>
            `;
        }
        
        state.previewType = 'graphics';
        
    } catch (e) {
        console.error('Graphics render error:', e);
        showPreviewError('Graphics render failed: ' + e.message);
    }
}

/**
 * Show placeholder when there's nothing to preview
 */
function showPreviewPlaceholder() {
    if (!elements.previewPlaceholder) return;
    
    elements.previewPlaceholder.style.display = 'block';
    elements.previewSplitView.classList.remove('active');
    if (elements.previewError) {
        elements.previewError.classList.remove('active');
    }
    
    // Clear all preview content
    if (elements.previewMathInput) elements.previewMathInput.innerHTML = '';
    if (elements.previewMathOutput) elements.previewMathOutput.innerHTML = '';
    if (elements.previewTableInput) elements.previewTableInput.innerHTML = '';
    if (elements.previewTableOutput) elements.previewTableOutput.innerHTML = '';
    
    state.previewType = 'none';
}

/**
 * Show preview error message
 */
function showPreviewError(message) {
    if (!elements.previewError) return;
    
    elements.previewPlaceholder.style.display = 'none';
    elements.previewSplitView.classList.remove('active');
    elements.previewError.textContent = message;
    elements.previewError.classList.add('active');
}

/**
 * Update preview based on current input/output
 */
function updatePreview() {
    if (!state.previewEnabled) return;
    
    const input = elements.inputEditor.value.trim();
    const output = elements.outputEditor.value.trim();
    
    if (!input) {
        showPreviewPlaceholder();
        return;
    }
    
    const previewType = detectPreviewType(input);
    
    switch (previewType) {
        case 'graphics':
            // Graphics preview using SVG renderer
            renderGraphicsPreview(input, output);
            break;
            
        case 'table':
            // Table preview using WASM parsed data
            if (state.wasmReady && state.wasm && state.wasm.previewTable) {
                try {
                    const format = state.direction === 'l2t' ? 'latex' : 'typst';
                    const tableData = state.wasm.previewTable(input, format);
                    renderTablePreview(tableData);
                } catch (e) {
                    if (DEBUG) console.log('Table preview not available:', e);
                    showPreviewPlaceholder();
                }
            } else {
                // Fallback: show placeholder for tables until backend is ready
                showPreviewPlaceholder();
            }
            break;
            
        case 'document':
            // Document preview using outline extraction
            renderDocumentPreview(input, output);
            break;
            
        case 'math':
            let inputLatex, outputLatex;
            let inputConvertFailed = false;
            let outputConvertFailed = false;
            
            if (state.direction === 'l2t') {
                // LaTeX -> Typst
                inputLatex = input; // Left: Input LaTeX (direct)
                
                // Right: Output Typst -> show as-is (can't render Typst with KaTeX)
                // Mark as "typst" to show special message
                outputLatex = null;
                outputConvertFailed = true; // Typst output can't be previewed with KaTeX
            } else {
                // Typst -> LaTeX
                // Left: Input Typst -> converted to LaTeX for preview
                if (input && state.wasmReady && state.wasm && state.wasm.typstToLatex) {
                     try {
                         inputLatex = state.wasm.typstToLatex(input);
                     } catch(e) {
                         inputLatex = null;
                         inputConvertFailed = true;
                     }
                } else {
                    inputLatex = null;
                    inputConvertFailed = true;
                }
                outputLatex = output; // Right: Output LaTeX (direct)
            }
            
            renderMathPreview(inputLatex, outputLatex, inputConvertFailed, outputConvertFailed);
            break;
            
        default:
            showPreviewPlaceholder();
    }
}

/**
 * Update preview UI language
 */
function updatePreviewLanguage() {
    const t = translations[state.lang];
    
    if (elements.previewTitle) {
        elements.previewTitle.textContent = t.preview;
    }
    if (elements.previewPlaceholder) {
        elements.previewPlaceholder.textContent = t.previewPlaceholder;
    }
    if (elements.previewToggleBtn) {
        elements.previewToggleBtn.title = t.previewToggle;
    }
    if (elements.previewInputLabel) {
        elements.previewInputLabel.textContent = t.previewInputLabel;
    }
    if (elements.previewOutputLabel) {
        elements.previewOutputLabel.textContent = t.previewOutputLabel;
    }
    if (elements.betaBadge) {
        elements.betaBadge.textContent = t.betaBadge;
    }
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
        showPreviewPlaceholder();
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
        
        // Update preview
        updatePreview();
    } catch (e) {
        elements.outputEditor.value = `Error: ${e.message}`;
        elements.convertTime.textContent = t.failed;
        showPreviewError(e.message);
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
    
    // Clear preview
    showPreviewPlaceholder();
}

function toggleTheme() {
    const html = document.documentElement;
    const currentTheme = html.getAttribute('data-theme');
    const newTheme = currentTheme === 'light' ? 'dark' : 'light';

    html.setAttribute('data-theme', newTheme);
    localStorage.setItem('theme', newTheme);
}

function togglePreview() {
    if (elements.previewPanel) {
        elements.previewPanel.classList.toggle('collapsed');
        state.previewEnabled = !elements.previewPanel.classList.contains('collapsed');
        localStorage.setItem('previewCollapsed', !state.previewEnabled);
    }
}

function loadPreviewState() {
    const collapsed = localStorage.getItem('previewCollapsed') === 'true';
    if (collapsed && elements.previewPanel) {
        elements.previewPanel.classList.add('collapsed');
        state.previewEnabled = false;
    }
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
    
    // Update preview language
    updatePreviewLanguage();
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
    elements.langToggle.addEventListener('click', toggleLanguage);
    
    // Preview toggle
    if (elements.previewToggleBtn) {
        elements.previewToggleBtn.addEventListener('click', togglePreview);
    }

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
    
    // Scroll synchronization between editor and preview
    setupScrollSync();
}

/**
 * Setup scroll synchronization between input editor and preview panes
 * Uses percentage-based scrolling for approximate sync
 */
function setupScrollSync() {
    const inputEditor = elements.inputEditor;
    const previewPanes = document.querySelectorAll('.preview-pane');
    
    let isScrolling = false;
    let scrollTimeout;
    
    // Sync from input editor to preview
    inputEditor.addEventListener('scroll', () => {
        if (isScrolling) return;
        
        clearTimeout(scrollTimeout);
        scrollTimeout = setTimeout(() => {
            const scrollPercentage = inputEditor.scrollTop / (inputEditor.scrollHeight - inputEditor.clientHeight);
            
            previewPanes.forEach(pane => {
                if (pane.scrollHeight > pane.clientHeight) {
                    isScrolling = true;
                    pane.scrollTop = scrollPercentage * (pane.scrollHeight - pane.clientHeight);
                    setTimeout(() => { isScrolling = false; }, 50);
                }
            });
        }, 16); // ~60fps throttle
    });
    
    // Sync from preview to input (bidirectional)
    previewPanes.forEach(pane => {
        pane.addEventListener('scroll', () => {
            if (isScrolling) return;
            
            clearTimeout(scrollTimeout);
            scrollTimeout = setTimeout(() => {
                if (pane.scrollHeight <= pane.clientHeight) return;
                
                const scrollPercentage = pane.scrollTop / (pane.scrollHeight - pane.clientHeight);
                
                isScrolling = true;
                inputEditor.scrollTop = scrollPercentage * (inputEditor.scrollHeight - inputEditor.clientHeight);
                
                // Also sync other preview panes
                previewPanes.forEach(otherPane => {
                    if (otherPane !== pane && otherPane.scrollHeight > otherPane.clientHeight) {
                        otherPane.scrollTop = scrollPercentage * (otherPane.scrollHeight - otherPane.clientHeight);
                    }
                });
                
                setTimeout(() => { isScrolling = false; }, 50);
            }, 16);
        });
    });
}

// ===== Initialization =====

async function init() {
    loadTheme();
    loadLanguage();
    loadPreviewState();
    updateDirection();
    updateMode();
    setupEventListeners();
    updatePreviewLanguage();

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
