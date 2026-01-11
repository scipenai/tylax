/**
 * CeTZ-to-SVG Renderer
 * A lightweight JavaScript parser and renderer for basic CeTZ graphics
 * 
 * Supports:
 * - line((x1, y1), (x2, y2), ...)
 * - circle((x, y), radius: r, ...)
 * - rect((x1, y1), (x2, y2), ...)
 * - content((x, y), [...], ...)
 */

// ===== Configuration =====
const SVG_CONFIG = {
    viewBoxWidth: 300,
    viewBoxHeight: 200,
    padding: 20,
    defaultStroke: '#58a6ff',
    defaultFill: 'none',
    defaultStrokeWidth: 1.5,
    fontSize: 12,
    textColor: '#e6edf3',
    gridColor: 'rgba(255, 255, 255, 0.1)',
};

// Base color palette
const BASE_COLORS = {
    'black': '#000000', 'white': '#ffffff',
    'red': '#f85149', 'green': '#3fb950', 'blue': '#58a6ff',
    'yellow': '#d29922', 'orange': '#db6d28', 'purple': '#a371f7',
    'gray': '#8b949e', 'grey': '#8b949e', 'cyan': '#39c5cf',
    'magenta': '#db61a2', 'pink': '#db61a2', 'brown': '#a5733e',
    'lime': '#7ee787', 'olive': '#6e7681', 'teal': '#2ea043',
    'navy': '#1f6feb', 'maroon': '#9d4040', 'aqua': '#79c0ff',
};

// ===== Color Processing =====

/**
 * Parse color string and return { color: string, opacity: number }
 * Supports: named colors, color!percent (TikZ), color.lighten/darken/transparentize (Typst)
 * @param {string} colorName - Color expression
 * @returns {{ color: string, opacity: number } | null}
 */
function parseColor(colorName) {
    if (!colorName) return null;
    
    const cleanName = colorName.trim().replace(/[,)\s]+$/, '');
    
    // Helper to get hex from name
    const getHex = (name) => BASE_COLORS[name.toLowerCase()] || (name.startsWith('#') ? name : null);
    
    // Handle TikZ color!percent (e.g., "red!50")
    const tikzMatch = cleanName.match(/^(\w+)!(\d+)$/);
    if (tikzMatch) {
        const hex = getHex(tikzMatch[1]);
        if (hex) {
            return { 
                color: blendHex(hex, '#ffffff', 1 - parseInt(tikzMatch[2]) / 100), 
                opacity: 1 
            };
        }
    }
    
    // Handle Typst color.lighten(percent)
    const lightenMatch = cleanName.match(/^(\w+)\.lighten\((\d+)%?\)?$/);
    if (lightenMatch) {
        const hex = getHex(lightenMatch[1]);
        if (hex) {
            return { 
                color: blendHex(hex, '#ffffff', parseInt(lightenMatch[2]) / 100), 
                opacity: 1 
            };
        }
    }
    
    // Handle Typst color.darken(percent)
    const darkenMatch = cleanName.match(/^(\w+)\.darken\((\d+)%?\)?$/);
    if (darkenMatch) {
        const hex = getHex(darkenMatch[1]);
        if (hex) {
            return { 
                color: blendHex(hex, '#000000', parseInt(darkenMatch[2]) / 100), 
                opacity: 1 
            };
        }
    }
    
    // Handle Typst color.transparentize(percent)
    const transMatch = cleanName.match(/^(\w+)\.(?:transparentize|opacity)\((\d+)%?\)?$/);
    if (transMatch) {
        const hex = getHex(transMatch[1]);
        if (hex) {
            return { 
                color: hex, 
                opacity: 1 - parseInt(transMatch[2]) / 100 
            };
        }
    }
    
    // Plain color name
    const hex = getHex(cleanName);
    if (hex) {
        return { color: hex, opacity: 1 };
    }
    
    // Fallback: return as-is (might be a CSS color like "rgb(...)")
    return { color: cleanName, opacity: 1 };
}

/**
 * Blend two hex colors
 * @param {string} c1 - First hex color
 * @param {string} c2 - Second hex color  
 * @param {number} amount - 0 = c1, 1 = c2
 * @returns {string} Blended hex color
 */
function blendHex(c1, c2, amount) {
    const r1 = parseInt(c1.slice(1, 3), 16);
    const g1 = parseInt(c1.slice(3, 5), 16);
    const b1 = parseInt(c1.slice(5, 7), 16);
    
    const r2 = parseInt(c2.slice(1, 3), 16);
    const g2 = parseInt(c2.slice(3, 5), 16);
    const b2 = parseInt(c2.slice(5, 7), 16);
    
    const r = Math.round(r1 + (r2 - r1) * amount);
    const g = Math.round(g1 + (g2 - g1) * amount);
    const b = Math.round(b1 + (b2 - b1) * amount);
    
    return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
}

// ===== Coordinate Transform =====

/**
 * Transform CeTZ coordinates to SVG coordinates
 * CeTZ uses mathematical coordinates (y up), SVG uses screen coordinates (y down)
 */
function transformCoord(x, y, bounds) {
    const { minX, maxX, minY, maxY } = bounds;
    const width = SVG_CONFIG.viewBoxWidth - 2 * SVG_CONFIG.padding;
    const height = SVG_CONFIG.viewBoxHeight - 2 * SVG_CONFIG.padding;
    
    const rangeX = maxX - minX || 1;
    const rangeY = maxY - minY || 1;
    
    const svgX = SVG_CONFIG.padding + ((x - minX) / rangeX) * width;
    const svgY = SVG_CONFIG.padding + ((maxY - y) / rangeY) * height; // Flip Y
    
    return { x: svgX, y: svgY };
}

// ===== Parser =====

/**
 * Parse coordinate string "(x, y)" -> [x, y]
 */
function parseCoord(coordStr) {
    const parts = coordStr.split(',').map(s => parseFloat(s.trim()));
    return [parts[0] || 0, parts[1] || 0];
}

/**
 * Parse TikZ coordinate string "x,y" -> [x, y]
 */
function parseTikZCoord(coordStr) {
    const parts = coordStr.split(',').map(s => parseFloat(s.trim()));
    return [parts[0] || 0, parts[1] || 0];
}

/**
 * Parse CeTZ/TikZ options string
 * @returns {Object} Options with strokeColor, strokeOpacity, fillColor, fillOpacity, etc.
 */
function parseOptions(optStr) {
    const options = {};
    
    // Parse stroke color (CeTZ style: stroke: color, TikZ style: draw=color)
    const strokeMatch = optStr.match(/stroke\s*:\s*(?:\(paint:\s*)?([a-zA-Z][a-zA-Z0-9!.()%]*)/) ||
                        optStr.match(/draw\s*=\s*([a-zA-Z][a-zA-Z0-9!.()%]*)/);
    if (strokeMatch) {
        const parsed = parseColor(strokeMatch[1]);
        if (parsed) {
            options.strokeColor = parsed.color;
            options.strokeOpacity = parsed.opacity;
        }
    }
    
    // Parse fill color (CeTZ style: fill: color, TikZ style: fill=color)
    const fillMatch = optStr.match(/fill\s*:\s*([a-zA-Z][a-zA-Z0-9!.()%]*)/) ||
                      optStr.match(/fill\s*=\s*([a-zA-Z][a-zA-Z0-9!.()%]*)/);
    if (fillMatch) {
        const parsed = parseColor(fillMatch[1]);
        if (parsed) {
            options.fillColor = parsed.color;
            options.fillOpacity = parsed.opacity;
        }
    }
    
    // Parse stroke thickness
    const thicknessMatch = optStr.match(/(?:thickness|line\s*width)\s*[=:]\s*([\d.]+)/i);
    if (thicknessMatch) {
        options.strokeWidth = parseFloat(thicknessMatch[1]) * 2;
    }
    
    // Parse TikZ thick/very thick
    if (optStr.includes('very thick')) options.strokeWidth = 3;
    else if (optStr.includes('thick')) options.strokeWidth = 2;
    else if (optStr.includes('thin')) options.strokeWidth = 0.5;
    
    // Parse dash
    if (optStr.includes('dash:') || optStr.includes('dashed')) options.dashArray = '5,5';
    if (optStr.includes('dotted')) options.dashArray = '2,2';
    
    // Parse arrows
    if (optStr.includes('->') || optStr.includes('mark: (end:')) options.markerEnd = 'arrow';
    if (optStr.includes('<-') || optStr.includes('mark: (start:')) options.markerStart = 'arrow';
    if (optStr.includes('<->')) { options.markerStart = 'arrow'; options.markerEnd = 'arrow'; }
    
    return options;
}

/**
 * Parse CeTZ code and extract drawing commands
 * @param {string} code - CeTZ code
 * @returns {Array} Array of parsed commands
 */
function parseCeTZ(code) {
    const commands = [];
    let match;
    
    // === CeTZ Commands ===
    
    // line((x1, y1), (x2, y2), ...)
    const lineRegex = /line\s*\(\s*\(([^)]+)\)\s*,\s*\(([^)]+)\)(?:\s*,\s*([^)]*))?\)/g;
    while ((match = lineRegex.exec(code)) !== null) {
        const [x1, y1] = parseCoord(match[1]);
        const [x2, y2] = parseCoord(match[2]);
        const options = parseOptions(match[3] || '');
        commands.push({ type: 'line', x1, y1, x2, y2, ...options });
    }
    
    // circle((x, y), radius: r, ...)
    const circleRegex = /circle\s*\(\s*\(([^)]+)\)\s*,\s*radius\s*:\s*([\d.]+)(?:\s*,\s*([^)]*))?\)/g;
    while ((match = circleRegex.exec(code)) !== null) {
        const [cx, cy] = parseCoord(match[1]);
        const radius = parseFloat(match[2]);
        const options = parseOptions(match[3] || '');
        commands.push({ type: 'circle', cx, cy, radius, ...options });
    }
    
    // rect((x1, y1), (x2, y2), ...)
    const rectRegex = /rect\s*\(\s*\(([^)]+)\)\s*,\s*\(([^)]+)\)(?:\s*,\s*([^)]*))?\)/g;
    while ((match = rectRegex.exec(code)) !== null) {
        const [x1, y1] = parseCoord(match[1]);
        const [x2, y2] = parseCoord(match[2]);
        const options = parseOptions(match[3] || '');
        commands.push({ type: 'rect', x1, y1, x2, y2, ...options });
    }
    
    // content((x, y), [...], ...)
    const contentRegex = /content\s*\(\s*\(([^)]+)\)\s*,\s*\[([^\]]*)\](?:\s*,\s*([^)]*))?\)/g;
    while ((match = contentRegex.exec(code)) !== null) {
        const [x, y] = parseCoord(match[1]);
        const text = match[2].replace(/\\\$/g, '$').replace(/\$/g, '');
        const options = parseOptions(match[3] || '');
        commands.push({ type: 'text', x, y, text, ...options });
    }
    
    // arc((x,y), start: deg, stop: deg, radius: r) - simplified as circle
    const arcRegex = /arc\s*\(\s*\(([^)]+)\)[^)]*radius\s*:\s*([\d.]+)/g;
    while ((match = arcRegex.exec(code)) !== null) {
        const [cx, cy] = parseCoord(match[1]);
        const radius = parseFloat(match[2]);
        const options = parseOptions(match[0]);
        commands.push({ type: 'circle', cx, cy, radius, ...options });
    }
    
    // bezier((x1,y1), (x2,y2), (c1x,c1y), (c2x,c2y))
    const bezierRegex = /bezier\s*\(\s*\(([^)]+)\)\s*,\s*\(([^)]+)\)\s*,\s*\(([^)]+)\)\s*(?:,\s*\(([^)]+)\))?/g;
    while ((match = bezierRegex.exec(code)) !== null) {
        const [x1, y1] = parseCoord(match[1]);
        const [x2, y2] = parseCoord(match[2]);
        const [cx1, cy1] = parseCoord(match[3]);
        const [cx2, cy2] = match[4] ? parseCoord(match[4]) : [cx1, cy1];
        commands.push({ type: 'bezier', x1, y1, x2, y2, cx1, cy1, cx2, cy2 });
    }
    
    // === TikZ Commands ===
    
    // \draw[options] (x1,y1) rectangle (x2,y2)
    const tikzRectRegex = /\\(draw|fill|path)\s*(?:\[([^\]]*)\])?\s*\(([^)]+)\)\s*rectangle\s*\(([^)]+)\)/g;
    while ((match = tikzRectRegex.exec(code)) !== null) {
        const cmd = match[1];
        const options = parseOptions(match[2] || '');
        const [x1, y1] = parseTikZCoord(match[3]);
        const [x2, y2] = parseTikZCoord(match[4]);
        
        if (cmd === 'fill' && !options.fillColor) {
            options.fillColor = options.strokeColor || SVG_CONFIG.defaultStroke;
            options.fillOpacity = options.strokeOpacity || 1;
            options.strokeColor = 'none';
        }
        
        commands.push({ type: 'rect', x1, y1, x2, y2, ...options });
    }
    
    // \draw[options] (x,y) circle (r)
    const tikzCircleRegex = /\\(draw|fill|path)\s*(?:\[([^\]]*)\])?\s*\(([^)]+)\)\s*circle\s*\(([^)]+)\)/g;
    while ((match = tikzCircleRegex.exec(code)) !== null) {
        const cmd = match[1];
        const options = parseOptions(match[2] || '');
        const [cx, cy] = parseTikZCoord(match[3]);
        const radius = parseFloat(match[4]);
        
        if (cmd === 'fill' && !options.fillColor) {
            options.fillColor = options.strokeColor || SVG_CONFIG.defaultStroke;
            options.fillOpacity = options.strokeOpacity || 1;
            options.strokeColor = 'none';
        }
        
        commands.push({ type: 'circle', cx, cy, radius, ...options });
    }
    
    // \draw[options] (x1,y1) -- (x2,y2) -- ...
    const tikzLineRegex = /\\draw\s*(?:\[([^\]]*)\])?\s*\(([^)]+)\)(?:\s*--\s*\(([^)]+)\))+/g;
    while ((match = tikzLineRegex.exec(code)) !== null) {
        const options = parseOptions(match[1] || '');
        const coordMatches = match[0].matchAll(/\(([^)]+)\)/g);
        const coords = Array.from(coordMatches).map(m => parseTikZCoord(m[1]));
        
        for (let i = 0; i < coords.length - 1; i++) {
            commands.push({ 
                type: 'line', 
                x1: coords[i][0], y1: coords[i][1], 
                x2: coords[i+1][0], y2: coords[i+1][1], 
                ...options 
            });
        }
    }
    
    // \node[options] at (x,y) {text}
    const tikzNodeRegex = /\\node\s*(?:\[([^\]]*)\])?\s*at\s*\(([^)]+)\)\s*\{([^}]+)\}/g;
    while ((match = tikzNodeRegex.exec(code)) !== null) {
        const options = parseOptions(match[1] || '');
        const [x, y] = parseTikZCoord(match[2]);
        const text = match[3].replace(/\\\$/g, '$').replace(/\$/g, '');
        commands.push({ type: 'text', x, y, text, ...options });
    }
    
    return commands;
}

// ===== SVG Generator =====

/**
 * Calculate bounding box for all commands
 */
function calculateBounds(commands) {
    let minX = Infinity, maxX = -Infinity;
    let minY = Infinity, maxY = -Infinity;
    
    for (const cmd of commands) {
        switch (cmd.type) {
            case 'line':
                minX = Math.min(minX, cmd.x1, cmd.x2);
                maxX = Math.max(maxX, cmd.x1, cmd.x2);
                minY = Math.min(minY, cmd.y1, cmd.y2);
                maxY = Math.max(maxY, cmd.y1, cmd.y2);
                break;
            case 'circle':
                minX = Math.min(minX, cmd.cx - cmd.radius);
                maxX = Math.max(maxX, cmd.cx + cmd.radius);
                minY = Math.min(minY, cmd.cy - cmd.radius);
                maxY = Math.max(maxY, cmd.cy + cmd.radius);
                break;
            case 'ellipse':
                minX = Math.min(minX, cmd.cx - cmd.rx);
                maxX = Math.max(maxX, cmd.cx + cmd.rx);
                minY = Math.min(minY, cmd.cy - cmd.ry);
                maxY = Math.max(maxY, cmd.cy + cmd.ry);
                break;
            case 'rect':
                minX = Math.min(minX, cmd.x1, cmd.x2);
                maxX = Math.max(maxX, cmd.x1, cmd.x2);
                minY = Math.min(minY, cmd.y1, cmd.y2);
                maxY = Math.max(maxY, cmd.y1, cmd.y2);
                break;
            case 'bezier':
                minX = Math.min(minX, cmd.x1, cmd.x2, cmd.cx1, cmd.cx2);
                maxX = Math.max(maxX, cmd.x1, cmd.x2, cmd.cx1, cmd.cx2);
                minY = Math.min(minY, cmd.y1, cmd.y2, cmd.cy1, cmd.cy2);
                maxY = Math.max(maxY, cmd.y1, cmd.y2, cmd.cy1, cmd.cy2);
                break;
            case 'text':
                minX = Math.min(minX, cmd.x);
                maxX = Math.max(maxX, cmd.x);
                minY = Math.min(minY, cmd.y);
                maxY = Math.max(maxY, cmd.y);
                break;
        }
    }
    
    const padX = (maxX - minX) * 0.1 || 1;
    const padY = (maxY - minY) * 0.1 || 1;
    
    return { minX: minX - padX, maxX: maxX + padX, minY: minY - padY, maxY: maxY + padY };
}

/**
 * Render a single command to SVG
 * Uses strokeColor/strokeOpacity and fillColor/fillOpacity for proper transparency
 */
function renderCommand(cmd, bounds) {
    const strokeColor = cmd.strokeColor || SVG_CONFIG.defaultStroke;
    const strokeOpacity = cmd.strokeOpacity ?? 1;
    const fillColor = cmd.fillColor || SVG_CONFIG.defaultFill;
    const fillOpacity = cmd.fillOpacity ?? 1;
    const strokeWidth = cmd.strokeWidth || SVG_CONFIG.defaultStrokeWidth;
    
    // Build attribute strings
    const strokeAttr = strokeColor === 'none' ? 'stroke="none"' : 
        `stroke="${strokeColor}" stroke-opacity="${strokeOpacity}"`;
    const fillAttr = fillColor === 'none' ? 'fill="none"' : 
        `fill="${fillColor}" fill-opacity="${fillOpacity}"`;
    const dashAttr = cmd.dashArray ? `stroke-dasharray="${cmd.dashArray}"` : '';
    const markerStart = cmd.markerStart === 'arrow' ? 'marker-start="url(#arrowStart)"' : '';
    const markerEnd = cmd.markerEnd === 'arrow' ? 'marker-end="url(#arrowEnd)"' : '';
    
    switch (cmd.type) {
        case 'line': {
            const p1 = transformCoord(cmd.x1, cmd.y1, bounds);
            const p2 = transformCoord(cmd.x2, cmd.y2, bounds);
            return `<line x1="${p1.x}" y1="${p1.y}" x2="${p2.x}" y2="${p2.y}" ${strokeAttr} stroke-width="${strokeWidth}" ${dashAttr} ${markerStart} ${markerEnd}/>`;
        }
        case 'circle': {
            const center = transformCoord(cmd.cx, cmd.cy, bounds);
            const scaleX = (SVG_CONFIG.viewBoxWidth - 2 * SVG_CONFIG.padding) / (bounds.maxX - bounds.minX || 1);
            const r = cmd.radius * Math.min(scaleX, 50);
            return `<circle cx="${center.x}" cy="${center.y}" r="${Math.max(r, 3)}" ${strokeAttr} ${fillAttr} stroke-width="${strokeWidth}" ${dashAttr}/>`;
        }
        case 'ellipse': {
            const center = transformCoord(cmd.cx, cmd.cy, bounds);
            const scaleX = (SVG_CONFIG.viewBoxWidth - 2 * SVG_CONFIG.padding) / (bounds.maxX - bounds.minX || 1);
            const scaleY = (SVG_CONFIG.viewBoxHeight - 2 * SVG_CONFIG.padding) / (bounds.maxY - bounds.minY || 1);
            const rx = cmd.rx * Math.min(scaleX, 50);
            const ry = cmd.ry * Math.min(scaleY, 50);
            return `<ellipse cx="${center.x}" cy="${center.y}" rx="${Math.max(rx, 3)}" ry="${Math.max(ry, 3)}" ${strokeAttr} ${fillAttr} stroke-width="${strokeWidth}" ${dashAttr}/>`;
        }
        case 'rect': {
            const p1 = transformCoord(cmd.x1, cmd.y1, bounds);
            const p2 = transformCoord(cmd.x2, cmd.y2, bounds);
            const x = Math.min(p1.x, p2.x);
            const y = Math.min(p1.y, p2.y);
            const width = Math.abs(p2.x - p1.x);
            const height = Math.abs(p2.y - p1.y);
            return `<rect x="${x}" y="${y}" width="${width}" height="${height}" ${strokeAttr} ${fillAttr} stroke-width="${strokeWidth}" ${dashAttr}/>`;
        }
        case 'bezier': {
            const p1 = transformCoord(cmd.x1, cmd.y1, bounds);
            const p2 = transformCoord(cmd.x2, cmd.y2, bounds);
            const c1 = transformCoord(cmd.cx1, cmd.cy1, bounds);
            const c2 = transformCoord(cmd.cx2, cmd.cy2, bounds);
            return `<path d="M${p1.x},${p1.y} C${c1.x},${c1.y} ${c2.x},${c2.y} ${p2.x},${p2.y}" ${strokeAttr} fill="none" stroke-width="${strokeWidth}" ${dashAttr} ${markerEnd}/>`;
        }
        case 'text': {
            const p = transformCoord(cmd.x, cmd.y, bounds);
            return `<text x="${p.x}" y="${p.y}" fill="${SVG_CONFIG.textColor}" font-size="${SVG_CONFIG.fontSize}" text-anchor="middle" dominant-baseline="middle">${escapeXml(cmd.text)}</text>`;
        }
        default:
            return '';
    }
}

/**
 * Generate grid lines
 */
function generateGrid() {
    let grid = '<g class="grid" opacity="0.3">';
    const step = 30;
    
    for (let x = SVG_CONFIG.padding; x <= SVG_CONFIG.viewBoxWidth - SVG_CONFIG.padding; x += step) {
        grid += `<line x1="${x}" y1="${SVG_CONFIG.padding}" x2="${x}" y2="${SVG_CONFIG.viewBoxHeight - SVG_CONFIG.padding}" stroke="${SVG_CONFIG.gridColor}" stroke-width="0.5"/>`;
    }
    for (let y = SVG_CONFIG.padding; y <= SVG_CONFIG.viewBoxHeight - SVG_CONFIG.padding; y += step) {
        grid += `<line x1="${SVG_CONFIG.padding}" y1="${y}" x2="${SVG_CONFIG.viewBoxWidth - SVG_CONFIG.padding}" y2="${y}" stroke="${SVG_CONFIG.gridColor}" stroke-width="0.5"/>`;
    }
    
    grid += '</g>';
    return grid;
}

/**
 * Generate placeholder SVG when no graphics detected
 */
function generatePlaceholderSVG(message) {
    return `
        <svg viewBox="0 0 ${SVG_CONFIG.viewBoxWidth} ${SVG_CONFIG.viewBoxHeight}" xmlns="http://www.w3.org/2000/svg" style="width: 100%; height: auto; max-height: 200px;">
            <rect width="100%" height="100%" fill="var(--bg-tertiary, #161b22)"/>
            <text x="50%" y="50%" fill="var(--text-muted, #8b949e)" font-size="14" text-anchor="middle" dominant-baseline="middle">${message}</text>
        </svg>
    `;
}

/**
 * Escape XML special characters
 */
function escapeXml(text) {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&apos;');
}

/**
 * Generate SVG from parsed commands
 */
function generateSVG(commands) {
    if (commands.length === 0) {
        return generatePlaceholderSVG('No graphics detected');
    }
    
    const bounds = calculateBounds(commands);
    
    let svg = `<svg viewBox="0 0 ${SVG_CONFIG.viewBoxWidth} ${SVG_CONFIG.viewBoxHeight}" xmlns="http://www.w3.org/2000/svg" style="width: 100%; height: auto; max-height: 300px;">`;
    
    // Define arrow markers
    svg += `<defs>
        <marker id="arrowEnd" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto" markerUnits="strokeWidth">
            <path d="M0,0 L0,6 L9,3 z" fill="${SVG_CONFIG.defaultStroke}"/>
        </marker>
        <marker id="arrowStart" markerWidth="10" markerHeight="10" refX="0" refY="3" orient="auto" markerUnits="strokeWidth">
            <path d="M9,0 L9,6 L0,3 z" fill="${SVG_CONFIG.defaultStroke}"/>
        </marker>
    </defs>`;
    
    // Background
    svg += `<rect width="100%" height="100%" fill="var(--bg-tertiary, #161b22)"/>`;
    
    // Grid
    svg += generateGrid();
    
    // Render all commands
    for (const cmd of commands) {
        svg += renderCommand(cmd, bounds);
    }
    
    svg += '</svg>';
    return svg;
}

// ===== Public API =====

/**
 * Render CeTZ or TikZ code to SVG
 * @param {string} code - Graphics code
 * @returns {string} SVG markup
 */
export function renderGraphicsToSVG(code) {
    try {
        // Pre-process: normalize whitespace to handle multi-line commands
        const cleanCode = code.replace(/\s+/g, ' ');
        const commands = parseCeTZ(cleanCode);
        return generateSVG(commands);
    } catch (e) {
        // Only log errors, not debug info
        if (import.meta.env?.DEV) {
            console.error('Graphics render error:', e);
        }
        return generatePlaceholderSVG('Render error');
    }
}

/**
 * Check if code contains renderable graphics
 * @param {string} code - Code to check
 * @returns {boolean}
 */
export function hasRenderableGraphics(code) {
    try {
        const cleanCode = code.replace(/\s+/g, ' ');
        const commands = parseCeTZ(cleanCode);
        return commands.length > 0;
    } catch {
        return false;
    }
}
