//! TeX Macro Expansion Engine (VM)
//!
//! This is the core expansion logic that processes a token stream,
//! recognizes macro invocations, and expands them recursively.

use super::primitives::{self, DefinitionKind, MacroSignature, PatternPart};
use super::token::{TexToken, TokenList};
use super::utils;
use super::ArgumentErrorType;
use super::EngineWarning;
use std::collections::HashMap;

/// Errors that can occur during macro argument parsing
#[derive(Debug, Clone)]
pub enum MacroError {
    /// Argument reading exceeded max_tokens limit without finding delimiter
    RunawayArgument,
    /// Input tokens did not match the expected pattern
    PatternMismatch,
    /// Infinite recursion detected
    MacroLoop(String),
}

/// A macro definition storing its parameters and replacement body
#[derive(Debug, Clone)]
pub struct MacroDef {
    /// Argument parsing signature
    pub signature: MacroSignature,
    /// Optional default value for first argument (for \newcommand[n][default])
    pub default_arg: Option<TokenList>,
    /// The replacement body as tokens (with #1, #2, etc. as Param tokens)
    pub body: TokenList,
}

impl MacroDef {
    /// Create a new macro definition with simple positional arguments
    pub fn simple(num_args: u8, body: TokenList) -> Self {
        MacroDef {
            signature: MacroSignature::Simple(num_args),
            default_arg: None,
            body,
        }
    }

    /// Create with optional default argument (simple signature only)
    pub fn simple_with_default(num_args: u8, default: TokenList, body: TokenList) -> Self {
        MacroDef {
            signature: MacroSignature::Simple(num_args),
            default_arg: Some(default),
            body,
        }
    }

    /// Create a pattern-based macro definition
    pub fn pattern(parts: Vec<PatternPart>, body: TokenList) -> Self {
        MacroDef {
            signature: MacroSignature::Pattern(parts),
            default_arg: None,
            body,
        }
    }

    /// Create a macro definition from a signature
    pub fn from_signature(signature: MacroSignature, body: TokenList) -> Self {
        MacroDef {
            signature,
            default_arg: None,
            body,
        }
    }

    /// Legacy constructor - wraps simple()
    pub fn new(num_args: u8, body: TokenList) -> Self {
        Self::simple(num_args, body)
    }

    /// Legacy constructor - wraps simple_with_default()
    pub fn with_default(num_args: u8, default: TokenList, body: TokenList) -> Self {
        Self::simple_with_default(num_args, default, body)
    }
}

/// Database of macro definitions with scope support
#[derive(Debug, Clone)]
pub struct MacroDb {
    // Stack of scopes. The last element is the current (innermost) scope.
    // The first element is the global scope.
    scopes: Vec<HashMap<String, MacroDef>>,
}

impl Default for MacroDb {
    fn default() -> Self {
        Self::new()
    }
}

impl MacroDb {
    /// Create a new empty macro database with a global scope
    pub fn new() -> Self {
        MacroDb {
            scopes: vec![HashMap::new()],
        }
    }

    /// Push a new local scope
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current local scope
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a macro in the current scope
    pub fn define(&mut self, name: String, def: MacroDef) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, def);
        }
    }

    /// Define a global macro (at the bottom of the stack)
    pub fn define_global(&mut self, name: String, def: MacroDef) {
        if let Some(global_scope) = self.scopes.first_mut() {
            global_scope.insert(name, def);
        }
    }

    /// Look up a macro definition, searching from innermost to outermost scope
    pub fn get(&self, name: &str) -> Option<&MacroDef> {
        for scope in self.scopes.iter().rev() {
            if let Some(def) = scope.get(name) {
                return Some(def);
            }
        }
        None
    }

    /// Check if a macro is defined
    pub fn is_defined(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Remove a macro definition
    pub fn undefine(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.remove(name);
        }
    }
}

// ============================================================================
// Engine Constants
// ============================================================================

/// Default maximum expansion depth (prevents stack overflow on recursive macros).
const DEFAULT_MAX_DEPTH: usize = 200;

/// Default maximum token count (prevents exponential macro expansion).
const DEFAULT_MAX_TOKENS: usize = 100_000;

// ============================================================================
// Engine Component Structures
// ============================================================================

/// Immutable configuration for the expansion engine.
///
/// These settings are typically set once at engine creation and don't change
/// during expansion.
#[derive(Debug, Clone)]
pub struct ExpansionConfig {
    /// Maximum expansion depth to prevent infinite recursion.
    pub max_depth: usize,
    /// Maximum token count to prevent exponential expansion.
    pub max_tokens: usize,
    /// Whether we're in math mode (for `\ifmmode`).
    pub math_mode: bool,
}

impl Default for ExpansionConfig {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            max_tokens: DEFAULT_MAX_TOKENS,
            math_mode: false,
        }
    }
}

impl ExpansionConfig {
    /// Create a config for math mode.
    pub fn math_mode() -> Self {
        Self {
            math_mode: true,
            ..Default::default()
        }
    }
}

/// Persistent state that survives across multiple `process()` calls.
///
/// This includes macro definitions and lexer state flags.
#[derive(Debug, Clone, Default)]
pub struct ExpansionState {
    /// Macro definitions database with scope support.
    pub db: MacroDb,
    /// Whether `@` is treated as a letter (for `\makeatletter`).
    pub at_is_letter: bool,
}

impl ExpansionState {
    /// Create a new empty expansion state.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Transient context for a single expansion run.
///
/// This is reset or collected after each top-level `process()` call.
#[derive(Debug, Clone, Default)]
pub struct ExpansionContext {
    /// Current token count during expansion (for limit checking).
    pub current_token_count: usize,
    /// Collected structured warnings during expansion.
    pub structured_warnings: Vec<EngineWarning>,
}

impl ExpansionContext {
    /// Create a new empty expansion context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the context for a new expansion run.
    pub fn reset(&mut self) {
        self.current_token_count = 0;
        // Note: warnings are NOT reset here - they accumulate across runs.
        // Use `take_warnings()` to collect and clear them.
    }

    /// Add a structured warning.
    pub fn push_warning(&mut self, warning: EngineWarning) {
        self.structured_warnings.push(warning);
    }

    /// Take all collected structured warnings, leaving the internal list empty.
    pub fn take_warnings(&mut self) -> Vec<EngineWarning> {
        std::mem::take(&mut self.structured_warnings)
    }
}

// ============================================================================
// Main Engine Structure
// ============================================================================

/// The macro expansion engine.
///
/// Composed of three focused sub-structures:
/// - `config`: Immutable settings (limits, modes)
/// - `state`: Persistent state (macro definitions, lexer flags)
/// - `context`: Transient run state (counters, warnings)
pub struct Engine {
    /// Immutable configuration for expansion limits and modes.
    pub config: ExpansionConfig,
    /// Persistent state including macro definitions.
    pub state: ExpansionState,
    /// Transient context for the current expansion run.
    pub context: ExpansionContext,
}

impl Engine {
    /// Create a new engine with the specified math mode.
    ///
    /// This is the unified constructor used by both `new()` and `new_math_mode()`.
    fn with_math_mode(math_mode: bool) -> Self {
        Engine {
            config: ExpansionConfig {
                math_mode,
                ..Default::default()
            },
            state: ExpansionState::new(),
            context: ExpansionContext::new(),
        }
    }

    /// Create a new engine (text mode).
    pub fn new() -> Self {
        Self::with_math_mode(false)
    }

    /// Create a new engine with math mode enabled.
    pub fn new_math_mode() -> Self {
        Self::with_math_mode(true)
    }

    /// Set maximum expansion depth (builder pattern).
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.config.max_depth = depth;
        self
    }

    /// Set maximum token count (builder pattern).
    pub fn with_max_tokens(mut self, count: usize) -> Self {
        self.config.max_tokens = count;
        self
    }

    /// Set math mode (affects `\ifmmode`).
    pub fn set_math_mode(&mut self, mode: bool) {
        self.config.math_mode = mode;
    }

    /// Add a structured warning.
    pub fn push_warning(&mut self, warning: EngineWarning) {
        self.context.push_warning(warning);
    }

    /// Take all collected structured warnings, leaving the internal list empty.
    pub fn take_structured_warnings(&mut self) -> Vec<EngineWarning> {
        self.context.take_warnings()
    }

    /// Convert a MacroError to an ArgumentErrorType.
    ///
    /// This centralizes the error conversion logic to avoid duplication.
    fn convert_macro_error(error: &MacroError) -> ArgumentErrorType {
        match error {
            MacroError::RunawayArgument => ArgumentErrorType::RunawayArgument,
            MacroError::PatternMismatch => ArgumentErrorType::PatternMismatch,
            MacroError::MacroLoop(msg) => ArgumentErrorType::Other(format!("Macro loop: {}", msg)),
        }
    }

    /// Handle macro argument parsing failure with graceful rollback.
    fn handle_macro_error(
        &mut self,
        macro_name: &str,
        error: &MacroError,
        consumed: Vec<TexToken>,
        result: &mut Vec<TexToken>,
    ) {
        let error_kind = Self::convert_macro_error(error);

        self.push_warning(EngineWarning::ArgumentParsingFailed {
            macro_name: macro_name.to_string(),
            error_kind,
        });
        result.push(TexToken::ControlSeq(macro_name.to_string()));
        result.extend(consumed);
    }

    /// Process tokens: expand macros
    pub fn process(&mut self, tokens: TokenList) -> TokenList {
        // Reset token count for each top-level process call
        self.context.reset();
        self.expand(tokens, 0)
    }

    /// Expand macros in a token list
    fn expand(&mut self, tokens: TokenList, depth: usize) -> TokenList {
        if depth > self.config.max_depth {
            self.push_warning(EngineWarning::DepthExceeded {
                max_depth: self.config.max_depth,
            });
            return tokens;
        }

        let mut result = Vec::new();
        let mut iter = tokens.into_inner().into_iter().peekable();

        while let Some(token) = iter.next() {
            // Check token limit to prevent exponential expansion
            self.context.current_token_count += 1;
            if self.context.current_token_count > self.config.max_tokens {
                self.push_warning(EngineWarning::TokenLimitExceeded {
                    max_tokens: self.config.max_tokens,
                });
                // Return what we have so far plus remaining tokens
                result.push(token);
                result.extend(iter);
                return TokenList::from_vec(result);
            }
            match &token {
                TexToken::BeginGroup => {
                    self.state.db.push_scope();
                    result.push(token);
                }
                TexToken::EndGroup => {
                    self.state.db.pop_scope();
                    result.push(token);
                }
                // Track $ and $$ to update math_mode for \ifmmode
                TexToken::MathShift => {
                    // Check if this is $$ (display math) by peeking at next token
                    if matches!(iter.peek(), Some(TexToken::MathShift)) {
                        // Consume the second $ for $$
                        iter.next();
                        result.push(TexToken::MathShift);
                        result.push(TexToken::MathShift);
                    } else {
                        // Single $
                        result.push(token);
                    }
                    // Toggle math mode (works for both $ and $$)
                    self.config.math_mode = !self.config.math_mode;
                }
                // Track \( \) \[ \] to update math_mode for \ifmmode
                // These are LaTeX math mode delimiters equivalent to $ and $$
                // Note: We intentionally do NOT handle \ensuremath here because it has
                // a scoped argument {}, and we'd need to restore math_mode after the arg.
                // That's complex and error-prone; let the converter handle \ensuremath.
                TexToken::ControlSeq(name) if matches!(name.as_str(), "[" | "(") => {
                    // \[ \( enter math mode
                    self.config.math_mode = true;
                    result.push(token.clone());
                }
                TexToken::ControlSeq(name) if matches!(name.as_str(), "]" | ")") => {
                    // \] \) exit math mode
                    self.config.math_mode = false;
                    result.push(token.clone());
                }
                TexToken::ControlSeq(name) if name == "global" => {
                    // Handle \global prefix
                    if let Some(TexToken::ControlSeq(next_name)) = iter.peek() {
                        if primitives::is_definition_command(next_name) {
                            if let Some(TexToken::ControlSeq(cmd_name)) = iter.next() {
                                let rest =
                                    self.handle_definition(&cmd_name, &mut iter, true, depth);
                                result.extend(rest.into_inner());
                            }
                        } else {
                            result.push(token.clone());
                        }
                    } else {
                        result.push(token.clone());
                    }
                }
                TexToken::ControlSeq(name) if primitives::is_definition_command(name) => {
                    let rest = self.handle_definition(name, &mut iter, false, depth);
                    result.extend(rest.into_inner());
                }
                TexToken::ControlSeq(name) if name == "begin" => {
                    if let Some(env_name) = self.read_env_name(&mut iter) {
                        let (tokens, _scope_pushed) =
                            self.handle_begin_environment(&env_name, &mut iter, depth);
                        result.extend(tokens);
                    } else {
                        result.push(token);
                    }
                }
                TexToken::ControlSeq(name) if name == "end" => {
                    if let Some(env_name) = self.read_env_name(&mut iter) {
                        let tokens = self.handle_end_environment(&env_name, &mut iter, depth);
                        result.extend(tokens);
                    } else {
                        result.push(token.clone());
                    }
                }
                TexToken::ControlSeq(name) if name == "expandafter" => {
                    if let Some(t1) = iter.next() {
                        if let Some(t2) = iter.next() {
                            // Expand t2 ONCE
                            let expanded_t2 = if let Some(exp) =
                                self.expand_token_once(t2.clone(), &mut iter, depth)
                            {
                                exp
                            } else {
                                TokenList::from_vec(vec![t2])
                            };

                            // Reconstruct stream: t1 + expanded_t2 + Rest
                            let mut new_tokens = vec![t1];
                            new_tokens.extend(expanded_t2.into_inner());
                            new_tokens.extend(iter); // Consumes the rest

                            // Continue expansion
                            return self.expand(TokenList::from_vec(new_tokens), depth);
                        } else {
                            result.push(t1);
                        }
                    }
                }
                TexToken::ControlSeq(name) if name == "csname" => {
                    if let Some(expanded_cs) = self.process_csname(&mut iter, depth) {
                        // Reconstruct stream: [expanded_cs] + Rest
                        let mut new_tokens = vec![expanded_cs];
                        new_tokens.extend(iter);
                        return self.expand(TokenList::from_vec(new_tokens), depth);
                    } else {
                        result.push(token.clone());
                    }
                }
                TexToken::ControlSeq(name) if name == "makeatletter" => {
                    // Enable @ as a letter in control sequence names
                    self.state.at_is_letter = true;
                    // Don't output anything - this is a state change command
                }
                TexToken::ControlSeq(name) if name == "makeatother" => {
                    // Disable @ as a letter in control sequence names
                    self.state.at_is_letter = false;
                    // Don't output anything - this is a state change command
                }
                // Handle LaTeX3 ExplSyntax blocks - skip entirely with warning
                TexToken::ControlSeq(name) if name == "ExplSyntaxOn" => {
                    let skipped_content = self.skip_explsyntax_block(&mut iter);
                    self.push_warning(EngineWarning::LaTeX3Skipped {
                        token_count: skipped_content.len(),
                    });
                    // Output a comment indicating skipped content
                    result.push(TexToken::Comment(" [LaTeX3 block skipped] ".to_string()));
                }
                // Handle unsupported primitives that would corrupt output
                TexToken::ControlSeq(name) if Self::is_unsupported_primitive(name) => {
                    self.push_warning(EngineWarning::UnsupportedPrimitive { name: name.clone() });
                    // Output as a comment to preserve visibility
                    result.push(TexToken::Comment(format!(" Unsupported: \\{} ", name)));
                    // Try to consume any arguments to prevent cascading issues
                    self.skip_primitive_args(name, &mut iter);
                }
                TexToken::ControlSeq(name) => {
                    // If at_is_letter is true, try to merge @ and subsequent letters
                    let merged_name = if self.state.at_is_letter {
                        self.merge_at_letters(name.clone(), &mut iter)
                    } else {
                        name.clone()
                    };

                    if let Some(macro_def) = self.state.db.get(&merged_name).cloned() {
                        match self.parse_arguments(&mut iter, &macro_def) {
                            Ok(args) => {
                                // Check depth limit BEFORE recursing - output args directly if exceeded
                                if depth + 1 > self.config.max_depth {
                                    self.push_warning(EngineWarning::DepthExceeded {
                                        max_depth: self.config.max_depth,
                                    });
                                    // Output arguments directly to preserve content (e.g., "x")
                                    for arg in args {
                                        result.extend(arg.into_inner());
                                    }
                                    continue;
                                }

                                let expanded_body = self.substitute_args(&macro_def.body, &args);
                                // TeX semantics: insert expanded tokens at front of input stream
                                // and continue processing. This is crucial for macros that expand
                                // to special commands like \iftrue, \iffalse, etc.
                                let mut new_tokens = expanded_body.into_inner();
                                new_tokens.extend(iter);
                                let fully_expanded =
                                    self.expand(TokenList::from_vec(new_tokens), depth + 1);
                                result.extend(fully_expanded.into_inner());
                                return TokenList::from_vec(result);
                            }
                            Err((err, consumed)) => {
                                // Push warning and rollback: output macro name and consumed tokens as raw text
                                self.handle_macro_error(&merged_name, &err, consumed, &mut result);
                                // Continue processing remaining tokens
                            }
                        }
                    } else if let Some(special_result) =
                        self.handle_special_macro(&merged_name, &mut iter)
                    {
                        let fully_expanded = self.expand(special_result, depth + 1);
                        result.extend(fully_expanded.into_inner());
                    } else {
                        result.push(TexToken::ControlSeq(merged_name));
                    }
                }
                _ => {
                    result.push(token.clone());
                }
            }
        }

        TokenList::from_vec(result)
    }

    /// Try to expand a single token once. Consumes args from iter if needed.
    fn expand_token_once<I>(
        &mut self,
        token: TexToken,
        iter: &mut std::iter::Peekable<I>,
        depth: usize,
    ) -> Option<TokenList>
    where
        I: Iterator<Item = TexToken>,
    {
        match token {
            TexToken::ControlSeq(ref name) if name == "expandafter" => {
                // Recurse expandafter
                if let Some(t1) = iter.next() {
                    if let Some(t2) = iter.next() {
                        let expanded_t2 =
                            if let Some(exp) = self.expand_token_once(t2.clone(), iter, depth) {
                                exp
                            } else {
                                TokenList::from_vec(vec![t2])
                            };
                        let mut result = vec![t1];
                        result.extend(expanded_t2.into_inner());
                        Some(TokenList::from_vec(result))
                    } else {
                        Some(TokenList::from_vec(vec![t1]))
                    }
                } else {
                    Some(TokenList::new())
                }
            }
            TexToken::ControlSeq(ref name) if name == "csname" => self
                .process_csname(iter, depth)
                .map(|cs| TokenList::from_vec(vec![cs])),
            TexToken::ControlSeq(ref name) => {
                if let Some(macro_def) = self.state.db.get(name).cloned() {
                    match self.parse_arguments(iter, &macro_def) {
                        Ok(args) => Some(self.substitute_args(&macro_def.body, &args)),
                        Err((err, consumed)) => {
                            // Push warning and return original token + consumed as unexpanded
                            self.push_warning(EngineWarning::ArgumentParsingFailed {
                                macro_name: name.clone(),
                                error_kind: ArgumentErrorType::Other(format!(
                                    "{:?} (in expandafter)",
                                    err
                                )),
                            });
                            let mut result = vec![token.clone()];
                            result.extend(consumed);
                            Some(TokenList::from_vec(result))
                        }
                    }
                } else {
                    self.handle_special_macro(name, iter)
                }
            }
            _ => None,
        }
    }

    /// Merge @ and subsequent letters into a control sequence name when at_is_letter is true.
    /// This reconstructs `\foo@bar` from tokens `\foo`, `@`, `b`, `a`, `r`.
    fn merge_at_letters<I>(&self, mut name: String, iter: &mut std::iter::Peekable<I>) -> String
    where
        I: Iterator<Item = TexToken>,
    {
        // Keep consuming @ and alphabetic characters
        loop {
            match iter.peek() {
                Some(TexToken::Char('@')) => {
                    name.push('@');
                    iter.next();
                }
                Some(TexToken::Char(c)) if c.is_ascii_alphabetic() => {
                    name.push(*c);
                    iter.next();
                }
                _ => break,
            }
        }
        name
    }

    /// Pre-process a token list to merge @ characters into control sequences.
    /// Used when at_is_letter is true to handle definitions like \def\foo@bar{...}
    fn merge_at_in_tokens(&self, tokens: Vec<TexToken>) -> TokenList {
        let mut result = Vec::new();
        let mut iter = tokens.into_iter().peekable();

        while let Some(token) = iter.next() {
            match token {
                TexToken::ControlSeq(mut name) => {
                    // Try to merge subsequent @ and letters
                    loop {
                        match iter.peek() {
                            Some(TexToken::Char('@')) => {
                                name.push('@');
                                iter.next();
                            }
                            Some(TexToken::Char(c)) if c.is_ascii_alphabetic() => {
                                name.push(*c);
                                iter.next();
                            }
                            _ => break,
                        }
                    }
                    result.push(TexToken::ControlSeq(name));
                }
                _ => result.push(token),
            }
        }

        TokenList::from_vec(result)
    }

    /// Process \csname ... \endcsname and return the resulting control sequence token
    fn process_csname<I>(&mut self, iter: &mut I, depth: usize) -> Option<TexToken>
    where
        I: Iterator<Item = TexToken>,
    {
        let mut cs_tokens = Vec::new();
        let mut found_end = false;

        for t in iter.by_ref() {
            if let TexToken::ControlSeq(ref n) = t {
                if n == "endcsname" {
                    found_end = true;
                    break;
                }
            }
            cs_tokens.push(t);
        }

        if found_end {
            // Expand the content
            let expanded_cs = self.expand(TokenList::from_vec(cs_tokens), depth + 1);
            let mut cs_name = String::new();
            for t in expanded_cs.into_inner() {
                match t {
                    TexToken::Char(c) => cs_name.push(c),
                    TexToken::Space => cs_name.push(' '),
                    TexToken::ControlSeq(n) => cs_name.push_str(&n),
                    _ => {}
                }
            }
            Some(TexToken::ControlSeq(cs_name))
        } else {
            None
        }
    }

    /// Helper to handle definition commands
    fn handle_definition<I>(
        &mut self,
        name: &str,
        iter: &mut std::iter::Peekable<I>,
        is_global: bool,
        depth: usize,
    ) -> TokenList
    where
        I: Iterator<Item = TexToken>,
    {
        let remaining: Vec<TexToken> = iter.collect();

        // If at_is_letter is true, pre-process tokens to merge @ into control sequences
        let remaining_list = if self.state.at_is_letter {
            self.merge_at_in_tokens(remaining)
        } else {
            TokenList::from_vec(remaining)
        };

        match primitives::parse_definition(name, remaining_list) {
            Ok((def_kind, rest)) => {
                match def_kind {
                    DefinitionKind::NewCommand {
                        name,
                        num_args,
                        default,
                        body,
                    }
                    | DefinitionKind::RenewCommand {
                        name,
                        num_args,
                        default,
                        body,
                    }
                    | DefinitionKind::ProvideCommand {
                        name,
                        num_args,
                        default,
                        body,
                    } => {
                        let macro_def = if let Some(def) = default {
                            MacroDef::with_default(num_args, def, body)
                        } else {
                            MacroDef::new(num_args, body)
                        };
                        if is_global {
                            self.state.db.define_global(name, macro_def);
                        } else {
                            self.state.db.define(name, macro_def);
                        }
                    }
                    DefinitionKind::Def {
                        name,
                        signature,
                        body,
                    } => {
                        let def = MacroDef::from_signature(signature, body);
                        if is_global {
                            self.state.db.define_global(name, def);
                        } else {
                            self.state.db.define(name, def);
                        }
                    }
                    DefinitionKind::Edef {
                        name,
                        signature,
                        body,
                    } => {
                        let expanded_body = self.expand(body, depth + 1);
                        let def = MacroDef::from_signature(signature, expanded_body);
                        if is_global {
                            self.state.db.define_global(name, def);
                        } else {
                            self.state.db.define(name, def);
                        }
                    }
                    DefinitionKind::Let { name, target } => {
                        if let Some(def) = self.state.db.get(&target).cloned() {
                            if is_global {
                                self.state.db.define_global(name.clone(), def);
                            } else {
                                self.state.db.define(name.clone(), def);
                            }
                        } else {
                            // Target not found - warn user that \let to built-in commands doesn't work
                            self.push_warning(EngineWarning::LetTargetNotFound {
                                name: name.clone(),
                                target: target.clone(),
                            });
                        }
                    }
                    DefinitionKind::NewEnvironment {
                        name,
                        num_args,
                        default,
                        begin_body,
                        end_body,
                    }
                    | DefinitionKind::RenewEnvironment {
                        name,
                        num_args,
                        default,
                        begin_body,
                        end_body,
                    } => {
                        let begin_def = if let Some(def) = default {
                            MacroDef::with_default(num_args, def, begin_body)
                        } else {
                            MacroDef::new(num_args, begin_body)
                        };
                        if is_global {
                            self.state.db.define_global(name.clone(), begin_def);
                        } else {
                            self.state.db.define(name.clone(), begin_def);
                        }

                        let end_def = MacroDef::new(0, end_body);
                        if is_global {
                            self.state.db.define_global(format!("end{}", name), end_def);
                        } else {
                            self.state.db.define(format!("end{}", name), end_def);
                        }
                    }
                    DefinitionKind::NewIf { base_name } => {
                        // \newif\iffoo creates:
                        // 1. \iffoo -> \iffalse (initial state)
                        // 2. \footrue -> \def\iffoo{\iftrue}
                        // 3. \foofalse -> \def\iffoo{\iffalse}

                        let if_name = format!("if{}", base_name);

                        // \iffoo initially expands to \iffalse
                        let initial_def = MacroDef::new(
                            0,
                            TokenList::from_vec(vec![TexToken::ControlSeq("iffalse".to_string())]),
                        );

                        // \footrue defines \iffoo as \iftrue
                        let true_body = TokenList::from_vec(vec![
                            TexToken::ControlSeq("def".to_string()),
                            TexToken::ControlSeq(if_name.clone()),
                            TexToken::BeginGroup,
                            TexToken::ControlSeq("iftrue".to_string()),
                            TexToken::EndGroup,
                        ]);
                        let true_def = MacroDef::new(0, true_body);

                        // \foofalse defines \iffoo as \iffalse
                        let false_body = TokenList::from_vec(vec![
                            TexToken::ControlSeq("def".to_string()),
                            TexToken::ControlSeq(if_name.clone()),
                            TexToken::BeginGroup,
                            TexToken::ControlSeq("iffalse".to_string()),
                            TexToken::EndGroup,
                        ]);
                        let false_def = MacroDef::new(0, false_body);

                        if is_global {
                            self.state.db.define_global(if_name, initial_def);
                            self.state
                                .db
                                .define_global(format!("{}true", base_name), true_def);
                            self.state
                                .db
                                .define_global(format!("{}false", base_name), false_def);
                        } else {
                            self.state.db.define(if_name, initial_def);
                            self.state.db.define(format!("{}true", base_name), true_def);
                            self.state
                                .db
                                .define(format!("{}false", base_name), false_def);
                        }
                    }
                    DefinitionKind::DeclareMathOperator {
                        name,
                        body,
                        is_starred,
                    } => {
                        // \DeclareMathOperator{\name}{text} expands to:
                        // \newcommand{\name}{\operatorname{text}}
                        // This produces op("text") in Typst, which is the correct representation.
                        // \DeclareMathOperator*{\name}{text} expands to:
                        // \newcommand{\name}{\operatorname*{text}}
                        // This produces limits(op("text")) in Typst.
                        let op_cmd = if is_starred {
                            "operatorname*"
                        } else {
                            "operatorname"
                        };

                        let mut expanded_body = vec![
                            TexToken::ControlSeq(op_cmd.to_string()),
                            TexToken::BeginGroup,
                        ];
                        // Filter out spacing commands like \, from the operator name
                        for token in body.into_inner() {
                            match &token {
                                TexToken::ControlSeq(cs)
                                    if cs == "," || cs == ";" || cs == "!" || cs == " " =>
                                {
                                    // Skip thin/medium/negative spaces in operator names
                                }
                                _ => expanded_body.push(token),
                            }
                        }
                        expanded_body.push(TexToken::EndGroup);

                        let def = MacroDef::new(0, TokenList::from_vec(expanded_body));
                        if is_global {
                            self.state.db.define_global(name, def);
                        } else {
                            self.state.db.define(name, def);
                        }
                    }
                }
                self.expand(rest, depth)
            }
            Err(remaining) => self.expand(remaining, depth),
        }
    }

    /// Read an environment name from {envname}
    fn read_env_name<I>(&self, iter: &mut std::iter::Peekable<I>) -> Option<String>
    where
        I: Iterator<Item = TexToken>,
    {
        utils::skip_spaces(iter);

        if iter.peek() != Some(&TexToken::BeginGroup) {
            return None;
        }
        iter.next();

        let mut name = String::new();
        let mut depth = 1;

        for token in iter.by_ref() {
            match token {
                TexToken::BeginGroup => depth += 1,
                TexToken::EndGroup => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                TexToken::Char(c) => name.push(c),
                TexToken::ControlSeq(s) => {
                    name.push('\\');
                    name.push_str(&s);
                }
                TexToken::Space => name.push(' '),
                _ => {}
            }
        }

        if name.is_empty() {
            None
        } else {
            Some(name.trim().to_string())
        }
    }

    // ========================================================================
    // Environment Handling Helpers
    // ========================================================================

    /// Handle `\begin{envname}` - expands environment start if defined as a macro.
    ///
    /// Returns tokens to add to result, and whether a scope was pushed.
    fn handle_begin_environment<I>(
        &mut self,
        env_name: &str,
        iter: &mut std::iter::Peekable<I>,
        depth: usize,
    ) -> (Vec<TexToken>, bool)
    where
        I: Iterator<Item = TexToken>,
    {
        if let Some(macro_def) = self.state.db.get(env_name).cloned() {
            match self.parse_arguments(iter, &macro_def) {
                Ok(args) => {
                    let expanded_body = self.substitute_args(&macro_def.body, &args);
                    let fully_expanded = self.expand(expanded_body, depth + 1);
                    self.state.db.push_scope();
                    let mut tokens = vec![TexToken::BeginGroup];
                    tokens.extend(fully_expanded.into_inner());
                    (tokens, true)
                }
                Err((err, consumed)) => {
                    let error_kind = Self::convert_macro_error(&err);
                    self.push_warning(EngineWarning::ArgumentParsingFailed {
                        macro_name: env_name.to_string(),
                        error_kind,
                    });
                    let mut tokens = self.emit_begin_env_tokens(env_name);
                    tokens.extend(consumed);
                    (tokens, false)
                }
            }
        } else {
            (self.emit_begin_env_tokens(env_name), false)
        }
    }

    /// Handle `\end{envname}` - expands environment end if defined as a macro.
    fn handle_end_environment<I>(
        &mut self,
        env_name: &str,
        iter: &mut std::iter::Peekable<I>,
        depth: usize,
    ) -> Vec<TexToken>
    where
        I: Iterator<Item = TexToken>,
    {
        let _ = iter; // iter not used currently but kept for future extensibility
        let end_macro_name = format!("end{}", env_name);
        if let Some(macro_def) = self.state.db.get(&end_macro_name).cloned() {
            let expanded_body = macro_def.body.clone();
            let fully_expanded = self.expand(expanded_body, depth + 1);
            let mut tokens = fully_expanded.into_inner();
            tokens.push(TexToken::EndGroup);
            self.state.db.pop_scope();
            tokens
        } else {
            self.emit_end_env_tokens(env_name)
        }
    }

    /// Emit raw tokens for `\begin{envname}` (when not expanding).
    fn emit_begin_env_tokens(&self, env_name: &str) -> Vec<TexToken> {
        let mut tokens = vec![
            TexToken::ControlSeq("begin".to_string()),
            TexToken::BeginGroup,
        ];
        for c in env_name.chars() {
            tokens.push(TexToken::Char(c));
        }
        tokens.push(TexToken::EndGroup);
        tokens
    }

    /// Emit raw tokens for `\end{envname}` (when not expanding).
    fn emit_end_env_tokens(&self, env_name: &str) -> Vec<TexToken> {
        let mut tokens = vec![
            TexToken::ControlSeq("end".to_string()),
            TexToken::BeginGroup,
        ];
        for c in env_name.chars() {
            tokens.push(TexToken::Char(c));
        }
        tokens.push(TexToken::EndGroup);
        tokens
    }

    // ========================================================================
    // Special Macro Handling
    // ========================================================================

    /// Handle special macros that require hardcoded logic (conditionals, etc.)
    fn handle_special_macro<I>(
        &self,
        name: &str,
        iter: &mut std::iter::Peekable<I>,
    ) -> Option<TokenList>
    where
        I: Iterator<Item = TexToken>,
    {
        match name {
            "xspace" => {
                // xspace inserts a space if the next token is alphanumeric
                let insert_space =
                    matches!(iter.peek(), Some(TexToken::Char(c)) if c.is_alphanumeric());
                if insert_space {
                    Some(TokenList::from_vec(vec![TexToken::Space]))
                } else {
                    Some(TokenList::new())
                }
            }
            "iftrue" => Some(self.handle_if_conditional(true, iter)),
            "iffalse" => Some(self.handle_if_conditional(false, iter)),
            "ifmmode" => Some(self.handle_if_conditional(self.config.math_mode, iter)),
            "ifx" => self.handle_ifx(iter),
            "ifstrequal" => self.handle_ifstrequal(iter),
            "else" | "fi" => Some(TokenList::new()),
            _ => None,
        }
    }

    /// Handle \iftrue or \iffalse (or any boolean conditional)
    fn handle_if_conditional<I>(&self, condition: bool, iter: &mut I) -> TokenList
    where
        I: Iterator<Item = TexToken>,
    {
        let mut true_branch = Vec::new();
        let mut false_branch = Vec::new();
        let mut depth = 1;
        let mut in_else = false;

        for token in iter.by_ref() {
            match &token {
                TexToken::ControlSeq(cmd) => match cmd.as_str() {
                    "iftrue" | "iffalse" | "ifmmode" | "ifstrequal" | "ifx" | "if" | "ifnum"
                    | "ifdim" | "ifcat" | "ifvoid" | "ifhbox" | "ifvbox" | "ifinner" | "ifcase" => {
                        depth += 1;
                        if in_else {
                            false_branch.push(token);
                        } else {
                            true_branch.push(token);
                        }
                    }
                    "else" if depth == 1 => {
                        in_else = true;
                    }
                    "else" => {
                        if in_else {
                            false_branch.push(token);
                        } else {
                            true_branch.push(token);
                        }
                    }
                    "fi" => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        if in_else {
                            false_branch.push(token);
                        } else {
                            true_branch.push(token);
                        }
                    }
                    _ => {
                        if in_else {
                            false_branch.push(token);
                        } else {
                            true_branch.push(token);
                        }
                    }
                },
                _ => {
                    if in_else {
                        false_branch.push(token);
                    } else {
                        true_branch.push(token);
                    }
                }
            }
        }

        if condition {
            TokenList::from_vec(true_branch)
        } else {
            TokenList::from_vec(false_branch)
        }
    }

    /// Handle \ifstrequal{str1}{str2}{true}{false}
    fn handle_ifstrequal<I>(&self, iter: &mut std::iter::Peekable<I>) -> Option<TokenList>
    where
        I: Iterator<Item = TexToken>,
    {
        utils::skip_spaces(iter);
        let str1 = utils::read_argument(iter);
        utils::skip_spaces(iter);
        let str2 = utils::read_argument(iter);
        utils::skip_spaces(iter);
        let true_branch = utils::read_argument(iter);
        utils::skip_spaces(iter);
        let false_branch = utils::read_argument(iter);

        let str1_text: String = str1.as_slice().iter().map(|t| format!("{}", t)).collect();
        let str2_text: String = str2.as_slice().iter().map(|t| format!("{}", t)).collect();

        if str1_text.trim() == str2_text.trim() {
            Some(true_branch)
        } else {
            Some(false_branch)
        }
    }

    /// Handle \ifx - compare two tokens without expansion
    /// \ifx compares the *next two tokens* for equality.
    /// Two control sequences are equal if they have the same meaning (same macro definition).
    /// Two character tokens are equal if they have the same character and catcode.
    fn handle_ifx<I>(&self, iter: &mut std::iter::Peekable<I>) -> Option<TokenList>
    where
        I: Iterator<Item = TexToken>,
    {
        utils::skip_spaces(iter);

        // Read first token
        let tok1 = iter.next()?;

        utils::skip_spaces(iter);

        // Read second token
        let tok2 = iter.next()?;

        // Compare the tokens
        let equal = self.tokens_equal_ifx(&tok1, &tok2);

        // Now collect the conditional branches
        Some(self.handle_if_conditional(equal, iter))
    }

    /// Check if two tokens are equal for \ifx purposes.
    /// For control sequences, we check if they have the same macro definition.
    /// For other tokens, we check structural equality.
    fn tokens_equal_ifx(&self, tok1: &TexToken, tok2: &TexToken) -> bool {
        match (tok1, tok2) {
            // Two control sequences - compare their definitions
            (TexToken::ControlSeq(name1), TexToken::ControlSeq(name2)) => {
                // First check if names are identical
                if name1 == name2 {
                    return true;
                }
                // Then check if they have the same definition
                match (self.state.db.get(name1), self.state.db.get(name2)) {
                    (Some(def1), Some(def2)) => {
                        // Compare definitions: same signature and same body
                        def1.signature == def2.signature && def1.body == def2.body
                    }
                    (None, None) => {
                        // Both undefined - considered equal if same name (already checked above)
                        // Different undefined control sequences are NOT equal
                        false
                    }
                    _ => false, // One defined, one not
                }
            }
            // Other tokens - use structural equality
            _ => tok1 == tok2,
        }
    }

    /// Skip tokens until \ExplSyntaxOff is found
    /// Returns the skipped tokens for diagnostic purposes
    fn skip_explsyntax_block<I>(&self, iter: &mut std::iter::Peekable<I>) -> Vec<TexToken>
    where
        I: Iterator<Item = TexToken>,
    {
        let mut skipped = Vec::new();
        let mut depth = 1; // Track nested ExplSyntaxOn/Off

        for token in iter.by_ref() {
            match &token {
                TexToken::ControlSeq(name) if name == "ExplSyntaxOn" => {
                    depth += 1;
                    skipped.push(token);
                }
                TexToken::ControlSeq(name) if name == "ExplSyntaxOff" => {
                    depth -= 1;
                    if depth == 0 {
                        // Found matching ExplSyntaxOff
                        break;
                    }
                    skipped.push(token);
                }
                _ => {
                    skipped.push(token);
                }
            }
        }

        skipped
    }

    /// Check if a control sequence name is an unsupported primitive
    fn is_unsupported_primitive(name: &str) -> bool {
        matches!(
            name,
            // Category code manipulation
            "catcode" | "lccode" | "uccode" | "sfcode" | "mathcode" |
            // Token manipulation primitives
            "scantokens" | "toks" | "everyhbox" | "everypar" | "everymath" |
            // Lookahead primitives (complex to implement correctly)
            "futurelet" | "afterassignment" | "aftergroup" |
            // Low-level box manipulation
            "setbox" | "box" | "copy" | "unhbox" | "unvbox" |
            // Macro parameter manipulation
            "meaning" | "the" | "romannumeral" |
            // Register operations that could affect expansion
            "advance" | "multiply" | "divide"
        )
    }

    /// Try to skip arguments for known unsupported primitives
    fn skip_primitive_args<I>(&self, name: &str, iter: &mut std::iter::Peekable<I>)
    where
        I: Iterator<Item = TexToken>,
    {
        match name {
            // \catcode`\@=11 format - skip until newline or space-separated
            "catcode" | "lccode" | "uccode" | "sfcode" | "mathcode" => {
                // Skip tokens until we see something that looks like end of assignment
                // Typically: \catcode`\@=11 or \catcode`@=11
                let mut found_equals = false;
                while let Some(token) = iter.peek() {
                    match token {
                        TexToken::Char('=') => {
                            iter.next();
                            found_equals = true;
                        }
                        TexToken::Char(c) if c.is_ascii_digit() && found_equals => {
                            iter.next();
                        }
                        TexToken::Char('`') | TexToken::Char('\\') => {
                            iter.next();
                        }
                        TexToken::ControlSeq(_) if !found_equals => {
                            iter.next();
                        }
                        TexToken::Space => {
                            if found_equals {
                                break;
                            }
                            iter.next();
                        }
                        _ => {
                            if found_equals {
                                break;
                            }
                            iter.next();
                        }
                    }
                }
            }
            // \futurelet\cs\next token - skip 2 tokens
            "futurelet" => {
                iter.next(); // Skip first token
                iter.next(); // Skip second token
            }
            // Default: don't skip anything
            _ => {}
        }
    }

    /// Parse macro arguments from the token stream
    /// Returns Ok(args) on success, or Err((error, consumed_tokens)) on failure
    fn parse_arguments<I>(
        &self,
        iter: &mut std::iter::Peekable<I>,
        macro_def: &MacroDef,
    ) -> Result<Vec<TokenList>, (MacroError, Vec<TexToken>)>
    where
        I: Iterator<Item = TexToken>,
    {
        match &macro_def.signature {
            MacroSignature::Simple(num_args) => {
                let mut args = Vec::new();

                let start_idx = if macro_def.default_arg.is_some() {
                    utils::skip_spaces(iter);
                    if iter.peek() == Some(&TexToken::Char('[')) {
                        iter.next();
                        args.push(utils::read_until_char(iter, ']'));
                    } else if let Some(default) = &macro_def.default_arg {
                        args.push(default.clone());
                    }
                    1
                } else {
                    0
                };

                for _ in start_idx..*num_args {
                    utils::skip_spaces(iter);
                    args.push(utils::read_argument(iter));
                }

                Ok(args)
            }
            MacroSignature::Pattern(parts) => self.parse_arguments_pattern(iter, parts),
        }
    }

    /// Parse arguments according to a delimited pattern
    fn parse_arguments_pattern<I>(
        &self,
        iter: &mut std::iter::Peekable<I>,
        parts: &[PatternPart],
    ) -> Result<Vec<TokenList>, (MacroError, Vec<TexToken>)>
    where
        I: Iterator<Item = TexToken>,
    {
        // Pre-allocate args (max 9 arguments in TeX)
        let mut args: Vec<TokenList> = vec![TokenList::new(); 9];
        let mut consumed: Vec<TexToken> = Vec::new();
        let mut skip_next = false;

        for (idx, part) in parts.iter().enumerate() {
            // Skip this part if it was already consumed as a delimiter
            if skip_next {
                skip_next = false;
                continue;
            }

            match part {
                PatternPart::Literal(expected) => {
                    // Skip leading spaces if expected doesn't start with space
                    if !expected
                        .first()
                        .is_some_and(|t| matches!(t, TexToken::Space))
                    {
                        while let Some(t) = iter.peek() {
                            if matches!(t, TexToken::Space | TexToken::Comment(_)) {
                                // SAFETY: peek() returned Some, so next() is guaranteed to return Some
                                consumed.push(iter.next().expect("peek succeeded"));
                            } else {
                                break;
                            }
                        }
                    }

                    // Match exact tokens
                    for exp_token in expected {
                        match iter.next() {
                            Some(tok) => {
                                consumed.push(tok.clone());
                                if tok != *exp_token {
                                    return Err((MacroError::PatternMismatch, consumed));
                                }
                            }
                            None => return Err((MacroError::RunawayArgument, consumed)),
                        }
                    }
                }
                PatternPart::Argument(arg_idx) => {
                    // Check next part to see if this is a delimited argument
                    let next_part = parts.get(idx + 1);
                    match next_part {
                        Some(PatternPart::Literal(delimiter)) => {
                            // Delimited argument - read until delimiter
                            // The delimiter is consumed by read_delimited_argument
                            match self.read_delimited_argument(iter, delimiter, &mut consumed) {
                                Ok(arg_tokens) => {
                                    if *arg_idx > 0 && (*arg_idx as usize) <= 9 {
                                        args[(*arg_idx as usize) - 1] =
                                            TokenList::from_vec(arg_tokens);
                                    }
                                    // Skip the next Literal part since we already consumed the delimiter
                                    skip_next = true;
                                }
                                Err(e) => return Err((e, consumed)),
                            }
                        }
                        _ => {
                            // Undelimited argument (standard brace-delimited or single token)
                            let (arg_tokens, arg_consumed) = self.read_argument_tracked(iter);
                            consumed.extend(arg_consumed);
                            if *arg_idx > 0 && (*arg_idx as usize) <= 9 {
                                args[(*arg_idx as usize) - 1] = TokenList::from_vec(arg_tokens);
                            }
                        }
                    }
                }
            }
        }

        Ok(args)
    }

    /// Read a single argument and track consumed tokens
    fn read_argument_tracked<I>(
        &self,
        iter: &mut std::iter::Peekable<I>,
    ) -> (Vec<TexToken>, Vec<TexToken>)
    where
        I: Iterator<Item = TexToken>,
    {
        let mut consumed = Vec::new();

        // Skip spaces
        while let Some(t) = iter.peek() {
            if matches!(t, TexToken::Space | TexToken::Comment(_)) {
                // SAFETY: peek() returned Some, so next() is guaranteed to return Some
                consumed.push(iter.next().expect("peek succeeded"));
            } else {
                break;
            }
        }

        let content = match iter.peek() {
            Some(TexToken::BeginGroup) => {
                // SAFETY: peek() returned Some(BeginGroup), so next() is guaranteed to return Some
                consumed.push(iter.next().expect("peek succeeded")); // {
                let (group_content, group_consumed) = self.read_balanced_group_tracked(iter);
                consumed.extend(group_consumed);
                group_content
            }
            Some(_) => {
                // SAFETY: peek() returned Some, so next() is guaranteed to return Some
                let token = iter.next().expect("peek succeeded");
                consumed.push(token.clone());
                vec![token]
            }
            None => Vec::new(),
        };

        (content, consumed)
    }

    /// Read a balanced group and track consumed tokens
    fn read_balanced_group_tracked<I>(&self, iter: &mut I) -> (Vec<TexToken>, Vec<TexToken>)
    where
        I: Iterator<Item = TexToken>,
    {
        let mut content = Vec::new();
        let mut consumed = Vec::new();
        let mut depth = 1;

        for token in iter.by_ref() {
            consumed.push(token.clone());
            match token {
                TexToken::BeginGroup => {
                    depth += 1;
                    content.push(token);
                }
                TexToken::EndGroup => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    content.push(token);
                }
                _ => {
                    content.push(token);
                }
            }
        }

        (content, consumed)
    }

    /// Read tokens until delimiter is found (respecting brace depth)
    fn read_delimited_argument<I>(
        &self,
        iter: &mut std::iter::Peekable<I>,
        delimiter: &[TexToken],
        consumed: &mut Vec<TexToken>,
    ) -> Result<Vec<TexToken>, MacroError>
    where
        I: Iterator<Item = TexToken>,
    {
        let mut arg_content = Vec::new();
        let mut depth: i32 = 0;
        let mut count = 0;
        let max_arg_tokens = 10000; // Safety limit

        loop {
            if count > max_arg_tokens {
                return Err(MacroError::RunawayArgument);
            }

            // At depth 0, try to match delimiter
            if depth == 0 && !delimiter.is_empty() {
                // Check if first delimiter token matches
                if let Some(first_delim) = delimiter.first() {
                    if let Some(next) = iter.peek() {
                        if next == first_delim {
                            // Potential match - try to match entire delimiter
                            let mut match_buffer = Vec::new();
                            let mut matched = true;

                            for expected in delimiter {
                                if let Some(tok) = iter.next() {
                                    match_buffer.push(tok.clone());
                                    if tok != *expected {
                                        matched = false;
                                        break;
                                    }
                                } else {
                                    matched = false;
                                    break;
                                }
                            }

                            if matched {
                                // Full match - delimiter found
                                consumed.extend(match_buffer);
                                return Ok(arg_content);
                            } else {
                                // Partial match - these tokens are part of argument
                                for tok in match_buffer {
                                    consumed.push(tok.clone());
                                    match &tok {
                                        TexToken::BeginGroup => depth += 1,
                                        TexToken::EndGroup => depth = (depth - 1).max(0),
                                        _ => {}
                                    }
                                    arg_content.push(tok);
                                }
                                count += 1;
                                continue;
                            }
                        }
                    }
                }
            }

            // Normal token reading
            if let Some(token) = iter.next() {
                consumed.push(token.clone());
                match &token {
                    TexToken::BeginGroup => {
                        depth += 1;
                        arg_content.push(token);
                    }
                    TexToken::EndGroup => {
                        depth = (depth - 1).max(0);
                        arg_content.push(token);
                    }
                    _ => {
                        arg_content.push(token);
                    }
                }
                count += 1;
            } else {
                return Err(MacroError::RunawayArgument);
            }
        }
    }

    /// Substitute parameter tokens with actual arguments
    fn substitute_args(&self, body: &TokenList, args: &[TokenList]) -> TokenList {
        let mut result = Vec::new();

        for token in body.as_slice() {
            match token {
                TexToken::Param(n) => {
                    let idx = (*n as usize).saturating_sub(1);
                    if let Some(arg) = args.get(idx) {
                        result.extend(arg.as_slice().iter().cloned());
                    }
                }
                TexToken::DeferredParam(n) => {
                    // Degrade DeferredParam to Param during expansion.
                    // This is the key mechanism for nested macro definitions:
                    // \def\outer#1{\def\inner##1{##1 and #1}}
                    // When \outer is expanded, ##1 becomes #1 for \inner.
                    result.push(TexToken::Param(*n));
                }
                _ => {
                    result.push(token.clone());
                }
            }
        }

        TokenList::from_vec(result)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::latex2typst::engine::lexer::{detokenize, tokenize};

    #[test]
    fn test_simple_macro() {
        let mut engine = Engine::new();
        engine
            .state
            .db
            .define("foo".into(), MacroDef::new(0, tokenize("bar")));

        let input = tokenize("\\foo");
        let output = engine.expand(input, 0);
        assert_eq!(detokenize(&output), "bar");
    }

    #[test]
    fn test_macro_with_arg() {
        let mut engine = Engine::new();
        engine
            .state
            .db
            .define("double".into(), MacroDef::new(1, tokenize("#1#1")));

        let input = tokenize("\\double{x}");
        let output = engine.expand(input, 0);
        assert_eq!(detokenize(&output), "xx");
    }

    #[test]
    fn test_macro_with_two_args() {
        let mut engine = Engine::new();
        engine.state.db.define(
            "pair".into(),
            MacroDef::new(2, tokenize("\\langle #1, #2\\rangle")),
        );

        let input = tokenize("\\pair{a}{b}");
        let output = engine.expand(input, 0);
        assert_eq!(detokenize(&output), "\\langle a, b\\rangle");
    }

    #[test]
    fn test_nested_braces_preserved() {
        let mut engine = Engine::new();
        engine.state.db.define(
            "pair".into(),
            MacroDef::new(2, tokenize("\\langle #1, #2\\rangle")),
        );

        let input = tokenize("\\pair{a^2}{\\frac{\\pi}{2}}");
        let output = engine.expand(input, 0);
        assert_eq!(detokenize(&output), "\\langle a^2, \\frac{\\pi}{2}\\rangle");
    }

    #[test]
    fn test_recursive_expansion() {
        let mut engine = Engine::new();
        // Test that nested macro calls are expanded recursively
        engine
            .state
            .db
            .define("outer".into(), MacroDef::new(0, tokenize("[\\inner]")));
        engine
            .state
            .db
            .define("inner".into(), MacroDef::new(0, tokenize("INNER")));

        let input = tokenize("\\outer");
        let output = engine.expand(input, 0);
        assert_eq!(detokenize(&output), "[INNER]");
    }

    #[test]
    fn test_environment_basic() {
        let mut engine = Engine::new();
        // Define a simple environment: \begin{myenv} -> BEGIN, \end{myenv} -> END
        engine
            .state
            .db
            .define("myenv".into(), MacroDef::new(0, tokenize("BEGIN")));
        engine
            .state
            .db
            .define("endmyenv".into(), MacroDef::new(0, tokenize("END")));

        let input = tokenize("\\begin{myenv}content\\end{myenv}");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(result.contains("BEGIN"), "Expected BEGIN in: {}", result);
        assert!(result.contains("END"), "Expected END in: {}", result);
        assert!(
            result.contains("content"),
            "Expected content in: {}",
            result
        );
    }

    #[test]
    fn test_environment_with_args() {
        let mut engine = Engine::new();
        // Environment with one argument: \begin{myenv}{arg} -> [arg]
        engine
            .state
            .db
            .define("myenv".into(), MacroDef::new(1, tokenize("[#1]")));
        engine
            .state
            .db
            .define("endmyenv".into(), MacroDef::new(0, tokenize("END")));

        let input = tokenize("\\begin{myenv}{hello}body\\end{myenv}");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(
            result.contains("[hello]"),
            "Expected [hello] in: {}",
            result
        );
        assert!(result.contains("body"), "Expected body in: {}", result);
        assert!(result.contains("END"), "Expected END in: {}", result);
    }

    #[test]
    fn test_newenvironment_integration() {
        // Full integration test using process()
        let mut engine = Engine::new();
        let input = tokenize(
            r"\newenvironment{mybox}{\fbox\bgroup}{\egroup} \begin{mybox}content\end{mybox}",
        );
        let output = engine.process(input);
        let result = detokenize(&output);
        assert!(result.contains("\\fbox"), "Expected \\fbox in: {}", result);
        assert!(
            result.contains("content"),
            "Expected content in: {}",
            result
        );
    }

    #[test]
    fn test_iftrue() {
        let mut engine = Engine::new();
        let input = tokenize(r"\iftrue YES\else NO\fi");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(result.contains("YES"), "Expected YES in: {}", result);
        assert!(!result.contains("NO"), "Unexpected NO in: {}", result);
    }

    #[test]
    fn test_iffalse() {
        let mut engine = Engine::new();
        let input = tokenize(r"\iffalse YES\else NO\fi");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(!result.contains("YES"), "Unexpected YES in: {}", result);
        assert!(result.contains("NO"), "Expected NO in: {}", result);
    }

    #[test]
    fn test_nested_conditionals() {
        let mut engine = Engine::new();
        let input = tokenize(r"\iftrue A\iftrue B\fi C\else D\fi");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(result.contains("A"), "Expected A in: {}", result);
        assert!(result.contains("B"), "Expected B in: {}", result);
        assert!(result.contains("C"), "Expected C in: {}", result);
        assert!(!result.contains("D"), "Unexpected D in: {}", result);
    }

    #[test]
    fn test_ifmmode() {
        // Default: not in math mode
        let mut engine = Engine::new();
        let input = tokenize(r"\ifmmode MATH\else TEXT\fi");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(result.contains("TEXT"), "Expected TEXT in: {}", result);
        assert!(!result.contains("MATH"), "Unexpected MATH in: {}", result);

        // Set math mode
        let mut engine2 = Engine::new();
        engine2.set_math_mode(true);
        let input2 = tokenize(r"\ifmmode MATH\else TEXT\fi");
        let output2 = engine2.expand(input2, 0);
        let result2 = detokenize(&output2);
        assert!(result2.contains("MATH"), "Expected MATH in: {}", result2);
        assert!(!result2.contains("TEXT"), "Unexpected TEXT in: {}", result2);
    }

    #[test]
    fn test_ifmmode_dynamic_tracking() {
        // Test that $ toggles math mode dynamically during expansion
        let mut engine = Engine::new();

        // Define a macro that uses \ifmmode
        let input = tokenize(
            r"\def\smart{\ifmmode x^2\else x squared\fi}Text: \smart. Math: $\smart$. Text again: \smart.",
        );
        let output = engine.process(input);
        let result = detokenize(&output);

        // First \smart should output "x squared" (text mode)
        // Second \smart inside $ should output "x^2" (math mode)
        // Third \smart should output "x squared" (back to text mode)
        assert!(
            result.contains("x squared"),
            "Expected 'x squared' in text mode: {}",
            result
        );
        assert!(
            result.contains("$x^2$"),
            "Expected '$x^2$' in math mode: {}",
            result
        );
    }

    #[test]
    fn test_ifmmode_display_math() {
        // Test that $$ (display math) also toggles math mode correctly
        let mut engine = Engine::new();

        let input = tokenize(r"\def\smart{\ifmmode MATH\else TEXT\fi}Before $$\smart$$ After");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("$$MATH$$"),
            "Expected '$$MATH$$' in display math: {}",
            result
        );
    }

    #[test]
    fn test_ifmmode_nested_macros() {
        // Test that \ifmmode works correctly in nested macro expansions
        // This is the exact pattern from the user's LaTeX document:
        // \newcommand{\strong}[1]{\ifmmode \mathbf{#1} \else \textbf{#1} \fi}
        // \newcommand{\xvec}{\strong{x}}
        let mut engine = Engine::new();

        let input = tokenize(
            r"\newcommand{\strong}[1]{\ifmmode \mathbf{#1}\else \textbf{#1}\fi}\newcommand{\xvec}{\strong{x}}Text: \xvec. Math: $\xvec$.",
        );
        let output = engine.process(input);
        let result = detokenize(&output);

        // Debug output
        eprintln!("Nested macro result: {}", result);

        // In text mode, \xvec -> \strong{x} -> \textbf{x}
        assert!(
            result.contains(r"\textbf{x}"),
            "Expected \\textbf{{x}} in text mode, got: {}",
            result
        );
        // In math mode, \xvec -> \strong{x} -> \mathbf{x}
        assert!(
            result.contains(r"$\mathbf{x}$"),
            "Expected $\\mathbf{{x}}$ in math mode, got: {}",
            result
        );
    }

    #[test]
    fn test_ifmmode_bracket_math() {
        // Test that \[ \] (display math) also toggles math mode correctly
        // This is critical for documents using \[ \] instead of $$ $$
        let mut engine = Engine::new();

        let input =
            tokenize(r"\def\smart{\ifmmode MATH\else TEXT\fi}Before \[\smart\] After \smart");
        let output = engine.process(input);
        let result = detokenize(&output);

        eprintln!("Bracket math result: {}", result);

        // Inside \[ \], should be math mode
        assert!(
            result.contains(r"\[MATH\]"),
            "Expected '\\[MATH\\]' in display math: {}",
            result
        );
        // After \], should be back to text mode
        assert!(
            result.contains("After TEXT"),
            "Expected 'After TEXT' after \\]: {}",
            result
        );
    }

    #[test]
    fn test_ifmmode_paren_math() {
        // Test that \( \) (inline math) also toggles math mode correctly
        let mut engine = Engine::new();

        let input = tokenize(r"\def\smart{\ifmmode MATH\else TEXT\fi}Before \(\smart\) After");
        let output = engine.process(input);
        let result = detokenize(&output);

        eprintln!("Paren math result: {}", result);

        // Inside \( \), should be math mode
        assert!(
            result.contains(r"\(MATH\)"),
            "Expected '\\(MATH\\)' in inline math: {}",
            result
        );
    }

    #[test]
    fn test_ifstrequal() {
        let mut engine = Engine::new();

        // Equal strings
        let input = tokenize(r"\ifstrequal{foo}{foo}{EQUAL}{NOTEQUAL}");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(result.contains("EQUAL"), "Expected EQUAL in: {}", result);
        assert!(
            !result.contains("NOTEQUAL"),
            "Unexpected NOTEQUAL in: {}",
            result
        );

        // Different strings
        let input2 = tokenize(r"\ifstrequal{foo}{bar}{EQUAL}{NOTEQUAL}");
        let output2 = engine.expand(input2, 0);
        let result2 = detokenize(&output2);
        assert!(
            !result2.contains("EQUAL") || result2.contains("NOTEQUAL"),
            "Expected NOTEQUAL in: {}",
            result2
        );
    }

    #[test]
    fn test_iftrue_no_else() {
        let mut engine = Engine::new();
        let input = tokenize(r"\iftrue YES\fi");
        let output = engine.expand(input, 0);
        let result = detokenize(&output);
        assert!(result.contains("YES"), "Expected YES in: {}", result);
    }

    #[test]
    fn test_edef_expands_at_definition() {
        let mut engine = Engine::new();

        // Define \inner first
        engine
            .state
            .db
            .define("inner".into(), MacroDef::new(0, tokenize("INNERVALUE")));

        // Now use \edef to define \outer with \inner in the body
        let input = tokenize(r"\edef\outer{\inner} \outer");
        let output = engine.process(input);
        let result = detokenize(&output);

        // \outer should expand to INNERVALUE because \inner was expanded at definition time
        assert!(
            result.contains("INNERVALUE"),
            "Expected INNERVALUE in: {}",
            result
        );
    }

    #[test]
    fn test_def_vs_edef() {
        let mut engine = Engine::new();

        engine
            .state
            .db
            .define("x".into(), MacroDef::new(0, tokenize("ORIGINAL")));

        // Define \deftest with \def - should NOT expand \x at definition time
        // Note: process() now parses definitions inline, so we just run the whole string
        let input = tokenize(r"\def\deftest{\x} \def\x{NEWVALUE} \deftest");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("NEWVALUE"),
            "\\def should use current \\x: {}",
            result
        );
    }

    #[test]
    fn test_let_copies_definition() {
        let mut engine = Engine::new();

        // Define \foo
        engine
            .state
            .db
            .define("foo".into(), MacroDef::new(0, tokenize("FOOVALUE")));

        // Use \let\bar=\foo
        let input = tokenize(r"\let\bar=\foo \bar");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("FOOVALUE"),
            "Expected FOOVALUE in: {}",
            result
        );
    }

    #[test]
    fn test_let_target_not_found_warning() {
        let mut engine = Engine::new();

        // Try to \let to an undefined target (simulates \let\myfrac\frac)
        let input = tokenize(r"\let\myfrac\frac");
        let _output = engine.process(input);

        // Should have a warning about target not found
        let warnings = engine.take_structured_warnings();
        assert!(
            warnings.iter().any(|w| matches!(
                w,
                EngineWarning::LetTargetNotFound { name, target }
                if name == "myfrac" && target == "frac"
            )),
            "Expected LetTargetNotFound warning, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_scoping() {
        let mut engine = Engine::new();

        // Global definition
        engine
            .state
            .db
            .define("x".into(), MacroDef::new(0, tokenize("GLOBAL")));

        // Local redefinition inside group
        let input = tokenize(r"\x {\def\x{LOCAL} \x} \x");
        let output = engine.process(input);
        let result = detokenize(&output);

        // Should be GLOBAL LOCAL GLOBAL
        assert!(result.contains("GLOBAL"), "Expected GLOBAL at start");
        assert!(result.contains("LOCAL"), "Expected LOCAL inside group");
        // Check ordering
        let first_global = result.find("GLOBAL").unwrap();
        let local = result.find("LOCAL").unwrap();
        let last_global = result.rfind("GLOBAL").unwrap();

        assert!(first_global < local);
        assert!(local < last_global);
    }

    #[test]
    fn test_expandafter() {
        let mut engine = Engine::new();
        engine
            .state
            .db
            .define("a".into(), MacroDef::new(0, tokenize("A")));
        engine
            .state
            .db
            .define("b".into(), MacroDef::new(0, tokenize("B")));

        // \expandafter\a\b should expand \b first, then \a
        // Result is AB
        let input = tokenize(r"\expandafter\a\b");
        let output = engine.process(input);
        assert_eq!(detokenize(&output), "AB");

        // More complex: \def\eat#1{} \expandafter\eat\b
        // \b expands to B. Stream becomes \eat B. \eat eats B. Result empty.
        engine
            .state
            .db
            .define("eat".into(), MacroDef::new(1, tokenize("")));
        let input2 = tokenize(r"\expandafter\eat\b");
        let output2 = engine.process(input2);
        assert_eq!(detokenize(&output2), "");

        // \def\show#1{(#1)}
        engine
            .state
            .db
            .define("show".into(), MacroDef::new(1, tokenize("(#1)")));
        // \expandafter\show\b -> \b expands to B. Stream: \show B. -> (B)
        let input3 = tokenize(r"\expandafter\show\b");
        let output3 = engine.process(input3);
        assert_eq!(detokenize(&output3), "(B)");
    }

    #[test]
    fn test_newif() {
        let mut engine = Engine::new();

        // \newif\iffoo creates \iffoo, \footrue, \foofalse
        let input = tokenize(r"\newif\iffoo \iffoo YES\else NO\fi");
        let output = engine.process(input);
        let result = detokenize(&output);

        // Initial state is false
        assert!(
            result.contains("NO"),
            "Expected NO (initial false state) in: {}",
            result
        );
        assert!(!result.contains("YES"), "Unexpected YES in: {}", result);

        // Now set to true
        let input2 = tokenize(r"\newif\ifbar \bartrue \ifbar YES\else NO\fi");
        let output2 = engine.process(input2);
        let result2 = detokenize(&output2);

        assert!(
            result2.contains("YES"),
            "Expected YES (after \\bartrue) in: {}",
            result2
        );
    }

    #[test]
    fn test_declare_math_operator() {
        let mut engine = Engine::new();

        // \DeclareMathOperator{\myop}{sin} should expand to \operatorname{sin}
        let input = tokenize(r"\DeclareMathOperator{\myop}{sin} \myop");
        let output = engine.process(input);
        let result = detokenize(&output);

        // Should expand to \operatorname{sin} (produces op("sin") in Typst)
        assert!(
            result.contains("\\operatorname"),
            "Expected \\operatorname in: {}",
            result
        );
        assert!(result.contains("sin"), "Expected sin in: {}", result);
    }

    #[test]
    fn test_csname() {
        let mut engine = Engine::new();
        // \csname foo\endcsname -> \foo
        // If \foo is defined, it expands.
        engine
            .state
            .db
            .define("foo".into(), MacroDef::new(0, tokenize("BAR")));

        let input = tokenize(r"\csname foo\endcsname");
        let output = engine.process(input);
        assert_eq!(detokenize(&output), "BAR");

        // Define using \expandafter\def\csname ...
        let input2 = tokenize(r"\expandafter\def\csname myvar\endcsname{VAL}\myvar");
        let output2 = engine.process(input2);
        assert_eq!(detokenize(&output2), "VAL");
    }

    #[test]
    fn test_delimited_arguments() {
        let mut engine = Engine::new();
        // \def\foo#1.{<#1>}
        let input = tokenize(r"\def\foo#1.{<#1>} \foo hello. world");
        let output = engine.process(input);
        let result = detokenize(&output);
        assert!(
            result.contains("<hello>"),
            "Expected <hello> in: {}",
            result
        );
        assert!(result.contains("world"), "Expected world in: {}", result);
    }

    #[test]
    fn test_delimited_nested_braces() {
        let mut engine = Engine::new();
        // \def\foo#1.{<#1>}
        // Dot inside braces should be ignored (brace depth tracking)
        let input = tokenize(r"\def\foo#1.{<#1>} \foo {a.b}.");
        let output = engine.process(input);
        let result = detokenize(&output);
        // The argument should be "{a.b}" (the dot inside braces is not a delimiter)
        assert!(result.contains("<"), "Expected < in: {}", result);
        assert!(result.contains(">"), "Expected > in: {}", result);
    }

    #[test]
    fn test_delimited_rollback() {
        let mut engine = Engine::new();
        // \def\foo#1.{<#1>}
        // Missing delimiter -> should output raw tokens (rollback)
        let input = tokenize(r"\def\foo#1.{<#1>} \foo hello world");
        let output = engine.process(input);
        let result = detokenize(&output);
        // Without the trailing dot, the macro should fail to match
        // and should output the raw tokens
        assert!(
            result.contains("\\foo") || result.contains("foo"),
            "Expected rollback with \\foo in: {}",
            result
        );
    }

    #[test]
    fn test_xspace_before_letter() {
        let mut engine = Engine::new();
        // \def\foo{bar\xspace}
        let input = tokenize(r"\def\foo{bar\xspace} \foo baz");
        let output = engine.process(input);
        let result = detokenize(&output);
        // Should have space between bar and baz
        assert!(
            result.contains("bar baz") || result.contains("bar  baz"),
            "Expected space between bar and baz in: {}",
            result
        );
    }

    #[test]
    fn test_xspace_before_punctuation() {
        let mut engine = Engine::new();
        // \def\foo{bar\xspace}
        let input = tokenize(r"\def\foo{bar\xspace} \foo.");
        let output = engine.process(input);
        let result = detokenize(&output);
        // Should NOT have space before punctuation
        assert!(
            result.contains("bar."),
            "Expected bar. (no space) in: {}",
            result
        );
    }

    #[test]
    fn test_two_delimiters() {
        let mut engine = Engine::new();
        // \def\foo#1=#2.{#1 equals #2}
        let input = tokenize(r"\def\foo#1=#2.{#1 equals #2} \foo x=y.");
        let output = engine.process(input);
        let result = detokenize(&output);
        assert!(
            result.contains("x equals y"),
            "Expected 'x equals y' in: {}",
            result
        );
    }

    // =====================================================================
    // Robustness Tests - DeferredParam, makeatletter, ifx
    // =====================================================================

    #[test]
    fn test_deferred_param_nested_macro() {
        let mut engine = Engine::new();

        // Define an outer macro that defines an inner macro.
        // ##1 should become #1 for the inner macro.
        // \def\outer#1{\def\inner##1{##1 and #1}}
        let input =
            tokenize(r"\def\outer#1{\def\inner##1{##1 and #1}} \outer{OUTER} \inner{INNER}");
        let output = engine.process(input);
        let result = detokenize(&output);

        // After \outer{OUTER}, \inner is defined as \def\inner#1{#1 and OUTER}
        // Then \inner{INNER} should produce "INNER and OUTER"
        assert!(result.contains("INNER"), "Expected INNER in: {}", result);
        assert!(result.contains("OUTER"), "Expected OUTER in: {}", result);
        assert!(result.contains("and"), "Expected 'and' in: {}", result);
    }

    #[test]
    fn test_deferred_param_preserved_in_body() {
        let mut engine = Engine::new();

        // Test that ##1 becomes #1 after outer expansion
        let input = tokenize(r"\def\makeinner{\def\inner##1{[##1]}} \makeinner \inner{X}");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(result.contains("[X]"), "Expected [X] in: {}", result);
    }

    #[test]
    fn test_makeatletter_basic() {
        let mut engine = Engine::new();

        // Without \makeatletter, \foo@bar should NOT be recognized as one command
        // But \foo should be followed by @bar as separate tokens
        let input = tokenize(r"\makeatletter \def\foo@bar{SUCCESS} \foo@bar \makeatother");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("SUCCESS"),
            "Expected SUCCESS in: {}",
            result
        );
    }

    #[test]
    fn test_makeatletter_complex_name() {
        let mut engine = Engine::new();

        // More complex @-names
        let input =
            tokenize(r"\makeatletter \def\my@internal@cmd{INTERNAL} \my@internal@cmd \makeatother");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("INTERNAL"),
            "Expected INTERNAL in: {}",
            result
        );
    }

    #[test]
    fn test_makeatletter_state_toggle() {
        let mut engine = Engine::new();

        // Define a macro with @, then use it, then turn off makeatletter
        let input = tokenize(r"\makeatletter \def\x@y{AT} \x@y \makeatother");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(result.contains("AT"), "Expected AT in: {}", result);
    }

    #[test]
    fn test_ifx_same_undefined() {
        let mut engine = Engine::new();

        // \ifx on two undefined control sequences with the same name
        // (They're the same token, so equal)
        let input = tokenize(r"\ifx\undefined\undefined YES\else NO\fi");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("YES"),
            "Expected YES for same undefined in: {}",
            result
        );
    }

    #[test]
    fn test_ifx_different_undefined() {
        let mut engine = Engine::new();

        // \ifx on two different undefined control sequences
        // (Different tokens, so not equal)
        let input = tokenize(r"\ifx\foo\bar YES\else NO\fi");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("NO"),
            "Expected NO for different undefined in: {}",
            result
        );
    }

    #[test]
    fn test_ifx_same_definition() {
        let mut engine = Engine::new();

        // Define two macros with the same body
        let input = tokenize(r"\def\a{X} \def\b{X} \ifx\a\b YES\else NO\fi");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("YES"),
            "Expected YES for same definition in: {}",
            result
        );
    }

    #[test]
    fn test_ifx_different_definition() {
        let mut engine = Engine::new();

        // Define two macros with different bodies
        let input = tokenize(r"\def\a{X} \def\b{Y} \ifx\a\b YES\else NO\fi");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("NO"),
            "Expected NO for different definition in: {}",
            result
        );
    }

    #[test]
    fn test_ifx_char_tokens() {
        let mut engine = Engine::new();

        // \ifx on character tokens - compare 'a' and 'a'
        // Note: We use \def to create single-char macros for the test
        let input = tokenize(r"\def\a{same} \def\b{same} \ifx\a\b YES\else NO\fi");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("YES"),
            "Expected YES for same char in: {}",
            result
        );
    }

    #[test]
    fn test_ifx_let_equality() {
        let mut engine = Engine::new();

        // \let creates a copy of a macro definition
        // \ifx should consider them equal
        let input = tokenize(r"\def\a{TEXT} \let\b=\a \ifx\a\b YES\else NO\fi");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("YES"),
            "Expected YES after \\let in: {}",
            result
        );
    }

    // ========================================================================
    // Scope isolation tests
    // ========================================================================

    #[test]
    fn test_scope_inner_def_not_visible_outside() {
        let mut engine = Engine::new();

        // Definition inside a group should not be visible outside
        // \inner should remain unexpanded after the group closes
        let input = tokenize(r"{\def\inner{INSIDE} \inner} \inner");
        let output = engine.process(input);
        let result = detokenize(&output);

        // Inside the group, \inner should expand to INSIDE
        assert!(
            result.contains("INSIDE"),
            "Expected INSIDE inside group in: {}",
            result
        );
        // Outside the group, \inner should remain as \inner (unexpanded)
        // The output should have \inner appearing after the closing brace
        assert!(
            result.contains("\\inner"),
            "Expected \\inner to remain unexpanded outside group in: {}",
            result
        );
    }

    #[test]
    fn test_scope_redefinition_does_not_affect_outer() {
        let mut engine = Engine::new();

        // Redefining a macro inside a group should not affect the outer definition
        let input = tokenize(r"\def\x{OUTER} {\def\x{INNER} \x} \x");
        let output = engine.process(input);
        let result = detokenize(&output);

        // Inside the group, \x should be INNER
        assert!(
            result.contains("INNER"),
            "Expected INNER inside group in: {}",
            result
        );
        // Outside the group, \x should still be OUTER
        // Count occurrences: should have exactly one INNER and one OUTER
        let outer_count = result.matches("OUTER").count();
        assert_eq!(
            outer_count, 1,
            "Expected exactly one OUTER outside group in: {}",
            result
        );
    }

    #[test]
    fn test_scope_nested_groups() {
        let mut engine = Engine::new();

        // Test nested group scoping
        let input = tokenize(r"\def\x{L0} {\def\x{L1} {\def\x{L2} \x} \x} \x");
        let output = engine.process(input);
        let result = detokenize(&output);

        // L2 inside innermost group, L1 in middle, L0 outside
        assert!(
            result.contains("L2"),
            "Expected L2 in innermost group: {}",
            result
        );
        assert!(
            result.contains("L1"),
            "Expected L1 in middle group: {}",
            result
        );
        assert!(
            result.contains("L0"),
            "Expected L0 outside all groups: {}",
            result
        );
    }

    #[test]
    fn test_scope_global_escapes_group() {
        let mut engine = Engine::new();

        // \global\def should define in the global scope, visible outside groups
        let input = tokenize(r"{\global\def\x{GLOBAL}} \x");
        let output = engine.process(input);
        let result = detokenize(&output);

        // \x should expand to GLOBAL even outside the group
        assert!(
            result.contains("GLOBAL"),
            "Expected GLOBAL from \\global\\def in: {}",
            result
        );
        // Should NOT contain unexpanded \x
        assert!(
            !result.contains("\\x"),
            "\\x should be expanded outside group with \\global: {}",
            result
        );
    }

    // ========================================================================
    // Plan-specified edge case tests (from audit plan)
    // ========================================================================

    #[test]
    fn test_plan_direct_recursion_warning() {
        // Plan case: \def\a{\a} \a -> should trigger DepthExceeded
        let mut engine = Engine::new().with_max_depth(10); // Low depth for fast test
        let input = tokenize(r"\def\a{\a} \a");
        let _ = engine.process(input);

        // Check that a warning was generated
        let warnings = engine.take_structured_warnings();
        let has_depth_warning = warnings
            .iter()
            .any(|w| matches!(w, EngineWarning::DepthExceeded { .. }));
        assert!(
            has_depth_warning,
            "Direct recursion should trigger DepthExceeded warning. Got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_plan_indirect_recursion_warning() {
        // Plan case: \def\a{\b}\def\b{\a} \a -> should trigger DepthExceeded
        let mut engine = Engine::new().with_max_depth(10);
        let input = tokenize(r"\def\a{\b}\def\b{\a} \a");
        let _ = engine.process(input);

        let warnings = engine.take_structured_warnings();
        let has_depth_warning = warnings
            .iter()
            .any(|w| matches!(w, EngineWarning::DepthExceeded { .. }));
        assert!(
            has_depth_warning,
            "Indirect recursion should trigger DepthExceeded warning. Got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_plan_deep_valid_chain() {
        // Plan case: \def\a{\b}\def\b{\c}\def\c{x} \a -> should output x
        let mut engine = Engine::new();
        let input = tokenize(r"\def\a{\b}\def\b{\c}\def\c{x} \a");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("x"),
            "Deep valid chain should resolve to x. Got: {}",
            result
        );
        // Should have no warnings
        let warnings = engine.take_structured_warnings();
        assert!(
            warnings.is_empty(),
            "Valid chain should have no warnings. Got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_plan_nested_braces_in_arg() {
        // Plan case: \newcommand{\cmd}[1]{<#1>} \cmd{a{b}c} -> <a{b}c>
        let mut engine = Engine::new();
        let input = tokenize(r"\newcommand{\cmd}[1]{<#1>} \cmd{a{b}c}");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("<a{b}c>"),
            "Nested braces should be preserved. Got: {}",
            result
        );
    }

    #[test]
    fn test_plan_optional_arg_with_default() {
        // Plan case: \newcommand{\x}[2][default]{#1-#2} \x{arg} -> default-arg
        let mut engine = Engine::new();
        let input = tokenize(r"\newcommand{\x}[2][default]{#1-#2} \x{arg}");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("default-arg"),
            "Optional arg should use default value. Got: {}",
            result
        );
    }

    #[test]
    fn test_plan_optional_arg_provided() {
        // When optional arg is provided: \x[custom]{arg} -> custom-arg
        let mut engine = Engine::new();
        let input = tokenize(r"\newcommand{\x}[2][default]{#1-#2} \x[custom]{arg}");
        let output = engine.process(input);
        let result = detokenize(&output);

        assert!(
            result.contains("custom-arg"),
            "Provided optional arg should be used. Got: {}",
            result
        );
    }

    #[test]
    fn test_plan_nested_brackets_in_arg() {
        // Plan case: \newcommand{\x}[1]{[#1]} \x{[\inner]} -> [[\inner]]
        let mut engine = Engine::new();
        let input = tokenize(r"\newcommand{\x}[1]{[#1]} \x{[\inner]}");
        let output = engine.process(input);
        let result = detokenize(&output);

        // The inner brackets should be preserved
        assert!(
            result.contains("[[") && result.contains("]]"),
            "Nested brackets should be preserved. Got: {}",
            result
        );
        assert!(
            result.contains("\\inner"),
            "Inner content should be preserved. Got: {}",
            result
        );
    }
}
