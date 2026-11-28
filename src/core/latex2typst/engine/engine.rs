//! TeX Macro Expansion Engine (VM)
//!
//! This is the core expansion logic that processes a token stream,
//! recognizes macro invocations, and expands them recursively.

use super::input::{Checkpoint, Collect, Expansion, Frame, InputStack, Place, Runs, BOUNDARY};
use super::primitives::{self, DefinitionKind, MacroSignature, PatternPart};
use super::token::{TexToken, TokenList};
use super::utils::{self, TokenSource};
use super::ArgumentErrorType;
use super::EngineWarning;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

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
    // Changes whenever a lookup may give a different result.
    generation: u64,
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
            generation: 0,
        }
    }

    /// Push a new local scope
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current local scope
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 && self.scopes.pop().is_some_and(|scope| !scope.is_empty()) {
            self.generation += 1;
        }
    }

    /// Define a macro in the current scope
    pub fn define(&mut self, name: String, def: MacroDef) {
        self.generation += 1;
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, def);
        }
    }

    /// Define a global macro (at the bottom of the stack)
    pub fn define_global(&mut self, name: String, def: MacroDef) {
        self.generation += 1;
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
        self.generation += 1;
        if let Some(scope) = self.scopes.last_mut() {
            scope.remove(name);
        }
    }
}

// ============================================================================
// Engine Constants
// ============================================================================

/// The arguments of a macro use.
struct Args {
    list: Vec<TokenList>,
    /// The first argument is the default of an omitted optional argument.
    default_used: bool,
    /// For each argument, the places its tokens were read at (none for a
    /// default argument).
    places: Vec<Vec<Place>>,
    /// Nesting level of the replacement text.
    level: usize,
    /// The expansion the replacement text is nested in.
    parent: Option<Rc<Expansion>>,
}

/// Why arguments could not be read, and the tokens read for them.
type ArgsError = (MacroError, Vec<TexToken>);

/// Where the tokens of an argument are among the places recorded while
/// reading the arguments (`None` for a default argument).
type Span = Option<Range<usize>>;

/// Read an argument (see [`utils::read_argument_in`]); also returns where
/// its tokens are among the places recorded.
fn read_recorded_argument(input: &mut InputStack) -> (TokenList, Range<usize>) {
    utils::skip_spaces_in(input);
    let braced = input.peek_token() == Some(&TexToken::BeginGroup);
    let start = input.recorded_len() + usize::from(braced);
    let arg = utils::read_argument_in(input);
    let span = start..start + arg.len();
    (arg, span)
}

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

    /// Expand macros in a token list whose tokens are at nesting level
    /// `depth`.
    ///
    /// Depth is the nesting of expansions, not the number of macros used: a
    /// macro read at level `d` puts its replacement text in at level `d + 1`
    /// (environment begin/end code and `\edef` bodies likewise open one
    /// level). Tokens that are put back unchanged (conditional branches,
    /// `\expandafter`, the result of `\csname`) keep their level, so a
    /// recursive call nests deeper however it is reached, while macros used
    /// one after another stay at the level of the text that uses them. A
    /// macro whose arguments take document text from a lower level, like a
    /// TeX loop over the document, is nested only in that text, and a tail
    /// call that goes on through pending input (a loop over a list held in a
    /// macro) only at its caller's level (see [`Engine::read_arguments`]). A
    /// macro may be expanded while its replacement text nests at most
    /// `max_depth` deep, so a chain of macros nested exactly `max_depth` deep
    /// expands fully; one level more is refused (see [`Engine::may_expand`]).
    ///
    /// Each document token read (text given to the engine, also when a macro
    /// passes it on as the first copy of an argument, or takes it from a
    /// lower level as an argument) starts a new top-level expansion with the
    /// whole `max_tokens` budget for itself and everything nested in it (see
    /// [`Engine::charge`]).
    ///
    /// Whenever no expansion is pending, and at every document token, the
    /// engine takes a checkpoint, so that an expansion that exceeds the
    /// token limit can be undone as a whole (see [`Engine::undo`]).
    ///
    /// Expansion uses an explicit input stack instead of recursion, so the
    /// Rust stack does not grow with the length of the input, the nesting
    /// depth or the number of macros, definitions, environments,
    /// `\expandafter`s or `\csname`s in it. Environment code and `\edef`
    /// bodies are pushed as collecting frames, whose expansion is written to
    /// an output of its own; so is the name of a `\csname`, read from the
    /// input up to its `\endcsname` (see [`Engine::finish_name`]).
    fn expand(&mut self, tokens: TokenList, depth: usize) -> TokenList {
        let mut input = InputStack::new(tokens.into_inner(), depth);
        self.expand_input(&mut input)
    }

    fn expand_input(&mut self, input: &mut InputStack) -> TokenList {
        // Output of the innermost collecting frame (or of the whole input)
        let mut result = Vec::new();
        // Outputs of the enclosing collecting frames
        let mut saved: Vec<Vec<TexToken>> = Vec::new();

        loop {
            // A collecting frame was pushed: its expansion is written apart
            while saved.len() < input.collectors() {
                saved.push(std::mem::take(&mut result));
            }
            if std::mem::take(&mut input.exceeded) {
                self.undo(input, &mut result, &mut saved);
            } else {
                input.refusing = false;
            }
            // Nothing to undo before this point
            let fresh = !input.pending();
            if fresh {
                input.commit(self.checkpoint(&result, &saved), None);
            }
            // `@` is read as a letter only while \makeatletter is in effect
            input.set_at_letter(self.state.at_is_letter);
            let Some(token) = input.next() else {
                // The end of the input, or of a collecting frame. A `\csname`
                // name still being read in it has no `\endcsname`: it is
                // written out after `\csname` as read so far
                if input.in_name() {
                    let content = std::mem::replace(&mut result, saved.pop().unwrap_or_default());
                    if let Some((_, then)) = input.close_name() {
                        result.push(TexToken::ControlSeq("csname".to_string()));
                        result.extend(content);
                        then.iter().for_each(|(_, place)| place.unhold());
                        input.push_leveled(then);
                    }
                    continue;
                }
                let Some(frame) = input.finish_collector() else {
                    break;
                };
                let content = std::mem::replace(&mut result, saved.pop().unwrap_or_default());
                self.finish(frame, content, input, &mut result);
                continue;
            };
            let place = input.place();
            if place.doc {
                // A new top-level expansion (undone as a whole with the
                // `\csname` whose name it is in, if any)
                input.start_top_level();
                self.context.current_token_count = 0;
                if fresh {
                    input.checkpoint.count = 0;
                    input.checkpoint.refused = 0;
                } else if !input.reading_name() {
                    input.commit(self.checkpoint(&result, &saved), Some(&token));
                }
            }
            if let TexToken::ControlSeq(name) = &token {
                let generation = self.state.db.generation;
                if !input.refusing {
                    if let Some(runaway) = input.was_undone(name, generation, self.config.math_mode)
                    {
                        // The same expansion exceeded the token limit before
                        self.warn_token_limit(input);
                        if runaway {
                            self.warn_depth(input);
                        }
                        input.refusing = true;
                    }
                }
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
                    if matches!(input.peek_token(), Some(TexToken::MathShift)) {
                        // Consume the second $ for $$
                        input.next();
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
                    let definition = match input.peek_token() {
                        Some(TexToken::ControlSeq(next_name))
                            if primitives::is_definition_command(next_name) =>
                        {
                            Some(next_name.clone())
                        }
                        _ => None,
                    };
                    if let Some(cmd_name) = definition {
                        input.next();
                        self.handle_definition(&cmd_name, input, true, &place);
                    } else {
                        result.push(token.clone());
                    }
                }
                TexToken::ControlSeq(name) if primitives::is_definition_command(name) => {
                    self.handle_definition(name, input, false, &place);
                }
                TexToken::ControlSeq(name) if name == "begin" => {
                    if let Some(env_name) = self.read_env_name(input) {
                        self.handle_begin_environment(&env_name, input, &place, &mut result);
                    } else {
                        result.push(token);
                    }
                }
                TexToken::ControlSeq(name) if name == "end" => {
                    if let Some(env_name) = self.read_env_name(input) {
                        self.handle_end_environment(&env_name, input, &place, &mut result);
                    } else {
                        result.push(token.clone());
                    }
                }
                TexToken::ControlSeq(name) if name == "expandafter" => {
                    if !self.expand_after(input) {
                        // Only one token follows: nothing to expand
                        result.extend(input.next());
                    }
                }
                TexToken::ControlSeq(name) if name == "csname" => {
                    if self.may_expand(&place, None, false, input) {
                        input.open_name(place.clone(), Vec::new());
                    } else if place.doc {
                        result.push(token.clone());
                    }
                }
                TexToken::ControlSeq(name) if name == "endcsname" && input.in_name() => {
                    let content = std::mem::replace(&mut result, saved.pop().unwrap_or_default());
                    self.finish_name(content, input);
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
                    let skipped_content = self.skip_explsyntax_block(input);
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
                    self.skip_primitive_args(name, input);
                }
                // Under \makeatletter the input stack has already glued `@`
                // and the letters after it onto the name
                TexToken::ControlSeq(name) => {
                    if let Some(macro_def) = self.state.db.get(name).cloned() {
                        match self.read_use(input, &macro_def, &place, name) {
                            // A refused use is dropped and its arguments stay
                            // in the input; document text stays as it is
                            None => {
                                if place.doc {
                                    result.push(token.clone());
                                }
                            }
                            // TeX semantics: insert expanded tokens at front of input stream
                            // and continue processing. This is crucial for macros that expand
                            // to special commands like \iftrue, \iffalse, etc.
                            Some((Ok(args), _)) => {
                                self.push_replacement(name, &macro_def, args, &place, input)
                            }
                            // Push warning and rollback: output macro name and consumed tokens as raw text
                            Some((Err((err, consumed)), _)) => {
                                self.handle_macro_error(name, &err, consumed, &mut result)
                            }
                        }
                    } else if let Some(special_result) =
                        self.handle_special_macro(name, input, &place)
                    {
                        // TeX semantics: the selected tokens are read next
                        input.push_leveled(special_result);
                    } else {
                        result.push(token.clone());
                    }
                }
                _ => {
                    result.push(token);
                }
            }
        }

        TokenList::from_vec(result)
    }

    /// Use the expansion of a collecting frame, `content`; `result` is the
    /// output of the enclosing frame.
    fn finish(
        &mut self,
        frame: Frame,
        content: Vec<TexToken>,
        input: &mut InputStack,
        result: &mut Vec<TexToken>,
    ) {
        match frame.collect {
            Some(Collect::Edef {
                name,
                signature,
                global,
            }) => {
                let def = MacroDef::from_signature(signature, TokenList::from_vec(content));
                if global {
                    self.state.db.define_global(name, def);
                } else {
                    self.state.db.define(name, def);
                }
            }
            Some(Collect::BeginEnv) => {
                self.state.db.push_scope();
                result.push(TexToken::BeginGroup);
                result.extend(content);
            }
            Some(Collect::EndEnv { name }) => {
                // Only the group of an open environment of that name is
                // closed (with those opened inside it and left open)
                result.extend(content);
                self.close_env(&name, input, result);
            }
            None => {}
        }
    }

    /// The engine's state now, for a checkpoint; `result` is the output
    /// being written and `saved` the outputs of the enclosing collecting
    /// frames.
    fn checkpoint(&self, result: &[TexToken], saved: &[Vec<TexToken>]) -> Checkpoint {
        Checkpoint {
            out_len: result.len(),
            saved_len: saved.len(),
            scopes: self.state.db.scopes.len(),
            generation: self.state.db.generation,
            count: self.context.current_token_count,
            math_mode: self.config.math_mode,
            at_letter: self.state.at_is_letter,
            refused: 0,
        }
    }

    /// Undo the expansion that exceeded the token limit (see
    /// [`Engine::charge`]) back to the last checkpoint, which was taken
    /// before it started: what it wrote and pushed is dropped, the scopes it
    /// opened are closed, the math mode, `@` setting and token count it
    /// changed are reset, and the tokens it read from the input are put back
    /// (see [`InputStack::undo`]). Its first token is then read again with
    /// every expansion refused (see [`Engine::may_expand`]): a use of a
    /// macro written in the document stays as it is, and its arguments and
    /// the text after it are expanded. Definitions it made stay.
    fn undo(
        &mut self,
        input: &mut InputStack,
        result: &mut Vec<TexToken>,
        saved: &mut Vec<Vec<TexToken>>,
    ) {
        let checkpoint = input.checkpoint;
        if saved.len() > checkpoint.saved_len {
            *result = saved
                .drain(checkpoint.saved_len..)
                .next()
                .unwrap_or_default();
        }
        result.truncate(checkpoint.out_len);
        while self.state.db.scopes.len() > checkpoint.scopes.max(1) {
            self.state.db.pop_scope();
        }
        while self.state.db.scopes.len() < checkpoint.scopes {
            self.state.db.push_scope();
        }
        self.config.math_mode = checkpoint.math_mode;
        self.state.at_is_letter = checkpoint.at_letter;
        self.context.current_token_count = checkpoint.count;
        input.undo();
    }

    /// Push the replacement text of macro `name`, read at `place`, whose
    /// arguments have been read, in front of the remaining input, unless the
    /// token limit stops it (see [`Engine::charge`]).
    fn push_replacement(
        &mut self,
        name: &str,
        macro_def: &MacroDef,
        args: Args,
        place: &Place,
        input: &mut InputStack,
    ) {
        let cost = Self::expansion_cost(&macro_def.body, &args.list, args.default_used);
        if self.charge(cost, name, place, input) {
            let body = self.substitute_args(&macro_def.body, &args);
            let expansion = input.new_expansion(Some(name), place.level, args.parent);
            input.push_expansion(body, args.level, expansion, None);
        }
    }

    /// Whether `name` (a macro or an environment's code; `None` for an
    /// `\edef` body or a `\csname` name), read at `place`, may be expanded.
    ///
    /// Refused are:
    /// - every expansion while the first token of an undone expansion is
    ///   read again (see [`Engine::undo`]);
    /// - expansion whose replacement text would nest deeper than
    ///   `max_depth` (a runaway recursion, or nesting too deep), with a
    ///   [`EngineWarning::DepthExceeded`] reported once per top-level
    ///   expansion. A use read at `max_depth` from an expansion whose text
    ///   has all been read may be a tail call that adds no depth (see
    ///   [`Engine::read_arguments`]): with `tail`, it is allowed here and
    ///   decided once its arguments have been read (see
    ///   [`Engine::read_use`]). If the name is being expanded already (a
    ///   runaway recursion), it is then refused deeper than its outermost
    ///   expansion until that expansion is over, else deeper in the
    ///   expansion it was read from (see [`InputStack::refuse`]); so is the
    ///   name of an expansion that read nothing but refused uses. A
    ///   recursion that branches (`\def\a{\a\a}`) thus stops at once, while
    ///   the text pending around it, and a later use of the same macro, are
    ///   still expanded.
    ///
    /// A refused use is dropped, and its arguments stay in the input;
    /// document text is written out as it is.
    fn may_expand(
        &mut self,
        place: &Place,
        name: Option<&str>,
        tail: bool,
        input: &mut InputStack,
    ) -> bool {
        if input.refusing {
            return false;
        }
        let Some(name) = name else {
            return true;
        };
        if !input.is_refused(name, place.level, place.src.as_ref()) {
            if place.level < self.config.max_depth {
                return true;
            }
            let caller = place.src.as_ref();
            if tail && caller.is_some_and(|e| e.is_exhausted() && !e.had_refusal()) {
                return true;
            }
            self.refuse_too_deep(name, place, input);
        }
        input.note_refusal();
        place.note_refused();
        false
    }

    /// Refuse `name`, read at `place`, for nesting deeper than `max_depth`.
    fn refuse_too_deep(&mut self, name: &str, place: &Place, input: &mut InputStack) {
        self.warn_depth(input);
        input.refuse(name, place.level, place.src.as_ref());
    }

    /// Read the arguments of a use of macro (or environment) `name`, read at
    /// `place`, if it may be expanded (see [`Engine::may_expand`]); `None` if
    /// it is refused, with its arguments left in the input. A use at
    /// `max_depth` is refused once its arguments are read unless its
    /// replacement text nests no deeper than `max_depth` (a tail call).
    fn read_use(
        &mut self,
        input: &mut InputStack,
        macro_def: &MacroDef,
        place: &Place,
        name: &str,
    ) -> Option<(Result<Args, ArgsError>, Vec<Place>)> {
        if !self.may_expand(place, Some(name), true, input) {
            return None;
        }
        if place.level < self.config.max_depth {
            return Some(self.read_arguments(input, macro_def, place));
        }
        input.keep_tokens();
        let (parsed, places) = self.read_arguments(input, macro_def, place);
        let tokens = input.take_kept_tokens();
        if matches!(&parsed, Ok(args) if args.level <= self.config.max_depth) {
            return Some((parsed, places));
        }
        self.refuse_too_deep(name, place, input);
        place.note_refused();
        input.push_leveled(tokens.into_iter().zip(places).collect());
        None
    }

    /// Report [`EngineWarning::DepthExceeded`], once per top-level
    /// expansion.
    fn warn_depth(&mut self, input: &mut InputStack) {
        if !std::mem::replace(&mut input.depth_warned, true) {
            self.push_warning(EngineWarning::DepthExceeded {
                max_depth: self.config.max_depth,
            });
        }
    }

    /// Charge an expansion of `name`, read at `place`, that adds `cost`
    /// tokens to the current top-level expansion. Returns `false` if it must
    /// not happen.
    ///
    /// Only tokens produced by expansion count against `max_tokens` (see
    /// [`Engine::expansion_cost`]): macro bodies, environment code and
    /// default arguments. Document text never does, as the count starts
    /// again with each top-level expansion. The limit thus bounds what a
    /// single use expands to (e.g. exponential expansion), not the length of
    /// the document. An expansion that would exceed it is undone back to the
    /// last checkpoint (see [`Engine::undo`]), and a
    /// [`EngineWarning::TokenLimitExceeded`] is reported once per top-level
    /// expansion; if a macro is being expanded inside an expansion of itself
    /// there (a runaway recursion with a large body), a
    /// [`EngineWarning::DepthExceeded`] is reported too.
    fn charge(&mut self, cost: usize, name: &str, place: &Place, input: &mut InputStack) -> bool {
        let total = self.context.current_token_count.saturating_add(cost);
        if total > self.config.max_tokens {
            self.warn_token_limit(input);
            if Expansion::is_runaway(place.src.as_ref(), name) {
                self.warn_depth(input);
                input.runaway = true;
            }
            input.exceeded = true;
            return false;
        }
        self.context.current_token_count = total;
        true
    }

    /// Report [`EngineWarning::TokenLimitExceeded`], once per top-level
    /// expansion.
    fn warn_token_limit(&mut self, input: &mut InputStack) {
        if !std::mem::replace(&mut input.token_warned, true) {
            self.push_warning(EngineWarning::TokenLimitExceeded {
                max_tokens: self.config.max_tokens,
            });
        }
    }

    /// Number of tokens a macro expansion adds to the input, for `max_tokens`.
    ///
    /// Arguments read from the input were already there, as document text or
    /// as the output of an earlier expansion that was counted then, so the
    /// first copy of each only moves those tokens. The body's own tokens,
    /// every further copy of an argument and every copy of a default
    /// argument (`default_used`: the optional first argument was not given)
    /// are new, and are counted. The cost is thus at least what the expansion
    /// grows the input by: document text passed as an argument is free
    /// however deeply it is nested, while a body that repeats its arguments
    /// (`\def\a#1{#1#1}` nested) is caught.
    fn expansion_cost(body: &TokenList, args: &[TokenList], default_used: bool) -> usize {
        let mut copied = vec![false; args.len()];
        if let Some(first) = copied.first_mut() {
            *first = default_used;
        }
        body.as_slice()
            .iter()
            .map(|token| match token {
                TexToken::Param(n) => {
                    let idx = (*n as usize).saturating_sub(1);
                    match args.get(idx) {
                        Some(arg) if std::mem::replace(&mut copied[idx], true) => arg.len(),
                        _ => 0,
                    }
                }
                _ if *token == BOUNDARY => 0,
                _ => 1,
            })
            .sum()
    }

    /// `\expandafter`: read the next two tokens, expand the second one once
    /// and put the first back in front of the result. Returns `false`, with
    /// the token put back, if only one token follows.
    ///
    /// When the second token is itself `\expandafter`, it acts on the tokens
    /// after it in the same way, and so on: the first tokens are set aside on
    /// a stack and put back, in order, once the last token of the chain has
    /// been expanded. A chain of any length thus takes no Rust recursion.
    /// Every token put back keeps its own place.
    fn expand_after(&mut self, input: &mut InputStack) -> bool {
        let mut set_aside = Vec::new();
        let expanded = loop {
            let Some(t1) = input.next() else {
                break true;
            };
            let place = input.place();
            place.hold();
            set_aside.push((t1, place));
            let Some(t2) = input.next() else {
                break set_aside.len() > 1;
            };
            let place = input.place();
            if matches!(&t2, TexToken::ControlSeq(name) if name == "expandafter") {
                continue;
            }
            if !self.expand_once(&t2, &place, input, &mut set_aside) {
                input.push(vec![t2], place);
            }
            break true;
        };
        for (token, place) in set_aside.into_iter().rev() {
            place.unhold();
            input.push(vec![token], place);
        }
        expanded
    }

    /// Expand `token`, read at `place`, once, consuming its arguments from
    /// `input` and putting the result back in front of it. Returns `false` if
    /// the token is not expandable, or is document text whose expansion is
    /// refused (it is then read again, and refused again, as it is). The
    /// tokens in `set_aside` are put back after a `\csname` name has been
    /// read.
    fn expand_once(
        &mut self,
        token: &TexToken,
        place: &Place,
        input: &mut InputStack,
        set_aside: &mut Vec<(TexToken, Place)>,
    ) -> bool {
        let TexToken::ControlSeq(name) = token else {
            return false;
        };
        if name == "csname" {
            if !self.may_expand(place, None, false, input) {
                return !place.doc;
            }
            input.open_name(place.clone(), std::mem::take(set_aside));
            return true;
        }
        if let Some(macro_def) = self.state.db.get(name).cloned() {
            let Some((parsed, places)) = self.read_use(input, &macro_def, place, name) else {
                return !place.doc;
            };
            match parsed {
                Ok(args) => self.push_replacement(name, &macro_def, args, place, input),
                Err((err, consumed)) => {
                    // Push warning and put original token + consumed back
                    // unexpanded, each token at the place it was read at
                    self.push_warning(EngineWarning::ArgumentParsingFailed {
                        macro_name: name.clone(),
                        error_kind: ArgumentErrorType::Other(format!("{:?} (in expandafter)", err)),
                    });
                    let places = places.into_iter().chain(std::iter::repeat(place.clone()));
                    let consumed = consumed.into_iter().zip(places);
                    input.push_leveled(
                        std::iter::once((token.clone(), place.clone()))
                            .chain(consumed)
                            .collect(),
                    );
                }
            }
            return true;
        }
        match self.handle_special_macro(name, input, place) {
            Some(tokens) => {
                input.push_leveled(tokens);
                true
            }
            None => false,
        }
    }

    /// End the `\csname` name being read, whose expansion is `content`: the
    /// control sequence it names is put in front of the input, at the
    /// deepest place of the tokens it was read from (document text if they
    /// all are), followed by the tokens `\expandafter` set aside.
    ///
    /// As in TeX, the name is read from the input with expansion until the
    /// `\endcsname` that ends it, so a `\csname` or macro inside it is
    /// expanded first; names nest on the input stack, not in Rust recursion.
    fn finish_name(&mut self, content: Vec<TexToken>, input: &mut InputStack) {
        let Some((place, then)) = input.close_name() else {
            return;
        };
        let mut cs_name = String::new();
        for t in content {
            match t {
                TexToken::Char(c) => cs_name.push(c),
                TexToken::Space => cs_name.push(' '),
                TexToken::ControlSeq(n) => cs_name.push_str(&n),
                _ => {}
            }
        }
        input.push(vec![TexToken::ControlSeq(cs_name)], place);
        for (token, place) in then.into_iter().rev() {
            place.unhold();
            input.push(vec![token], place);
        }
    }

    /// Close the group of environment `name` (see [`InputStack::close_env`]).
    fn close_env(&mut self, name: &str, input: &mut InputStack, result: &mut Vec<TexToken>) {
        for _ in 0..input.close_env(name) {
            result.push(TexToken::EndGroup);
            self.state.db.pop_scope();
        }
    }

    /// Helper to handle definition commands (read at `place`)
    ///
    /// Only the definition itself (name, parameter text and body) is read
    /// from the input; the input after it is left in place for the main loop.
    /// Its bodies keep where a space separated a control word from an `@`
    /// (see [`TokenSource::boundary`]).
    fn handle_definition(
        &mut self,
        name: &str,
        input: &mut InputStack,
        is_global: bool,
        place: &Place,
    ) {
        input.start_recording();
        input.marking = true;
        let read = primitives::read_definition(name, input);
        input.marking = false;
        // Recording only lets an argument given back at the end of the
        // input return to its own places.
        input.take_recorded();
        match read {
            Some(def_kind) => {
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
                        // The body is expanded on the input stack (see
                        // `Engine::finish`) as produced text, so a body
                        // over the token limit undoes the whole use that
                        // defined it and its document text is kept. A
                        // refused body is defined unexpanded.
                        if self.may_expand(place, None, false, input) {
                            let collect = Collect::Edef {
                                name,
                                signature,
                                global: is_global,
                            };
                            let body = Runs::of(body.into_inner(), false).finish();
                            let expansion =
                                input.new_expansion(None, place.level, place.src.clone());
                            input.push_expansion(body, place.level + 1, expansion, Some(collect));
                            return;
                        }
                        let def = MacroDef::from_signature(signature, body);
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
            }
            None => {
                // A malformed definition is dropped, with the tokens read for it
            }
        }
    }

    /// Read an environment name from {envname}
    fn read_env_name<S: TokenSource>(&self, iter: &mut S) -> Option<String> {
        utils::skip_spaces_in(iter);

        if iter.peek_token() != Some(&TexToken::BeginGroup) {
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

    /// Handle `\begin{envname}` (read at `place`) - expands environment start
    /// if defined as a macro, writing to `result`. The begin code counts as
    /// a replacement text for the limits; a refused environment is opened
    /// without it. Either way the environment is recorded as open (see
    /// [`InputStack::open_env`]).
    fn handle_begin_environment(
        &mut self,
        env_name: &str,
        iter: &mut InputStack,
        place: &Place,
        result: &mut Vec<TexToken>,
    ) {
        let Some(macro_def) = self.state.db.get(env_name).cloned() else {
            result.extend(self.emit_begin_env_tokens(env_name));
            return;
        };
        let Some((parsed, _)) = self.read_use(iter, &macro_def, place, env_name) else {
            self.state.db.push_scope();
            result.push(TexToken::BeginGroup);
            iter.open_env(env_name);
            return;
        };
        match parsed {
            Ok(args) => {
                let cost = Self::expansion_cost(&macro_def.body, &args.list, args.default_used);
                if self.charge(cost, env_name, place, iter) {
                    let body = self.substitute_args(&macro_def.body, &args);
                    let expansion = iter.new_expansion(Some(env_name), place.level, args.parent);
                    iter.push_expansion(body, args.level, expansion, Some(Collect::BeginEnv));
                    iter.open_env(env_name);
                }
            }
            Err((err, consumed)) => {
                let error_kind = Self::convert_macro_error(&err);
                self.push_warning(EngineWarning::ArgumentParsingFailed {
                    macro_name: env_name.to_string(),
                    error_kind,
                });
                result.extend(self.emit_begin_env_tokens(env_name));
                result.extend(consumed);
            }
        }
    }

    /// Handle `\end{envname}` (read at `place`) - expands environment end if
    /// defined as a macro, writing to `result`. The end code is limited like
    /// the begin code; a refused environment is closed without it. Only the
    /// group of an environment of that name that is open is closed, after
    /// the end code (as LaTeX checks the name after running it): an `\end`
    /// that has no `\begin`, such as one that ends itself again in its own
    /// end code, closes no group.
    fn handle_end_environment(
        &mut self,
        env_name: &str,
        iter: &mut InputStack,
        place: &Place,
        result: &mut Vec<TexToken>,
    ) {
        let end_macro_name = format!("end{}", env_name);
        let Some(macro_def) = self.state.db.get(&end_macro_name).cloned() else {
            result.extend(self.emit_end_env_tokens(env_name));
            return;
        };
        if !self.may_expand(place, Some(&end_macro_name), false, iter) {
            self.close_env(env_name, iter, result);
            return;
        }
        let cost = macro_def
            .body
            .as_slice()
            .iter()
            .filter(|t| **t != BOUNDARY)
            .count();
        if self.charge(cost, &end_macro_name, place, iter) {
            let body = Runs::of(macro_def.body.into_inner(), false).finish();
            let expansion =
                iter.new_expansion(Some(&end_macro_name), place.level, place.src.clone());
            let collect = Collect::EndEnv {
                name: env_name.to_string(),
            };
            iter.push_expansion(body, place.level + 1, expansion, Some(collect));
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

    /// Handle special macros that require hardcoded logic (conditionals,
    /// etc.), read at `place`. Returns the tokens to read next, each with
    /// the place it had (or `place` for an inserted space).
    fn handle_special_macro(
        &self,
        name: &str,
        iter: &mut InputStack,
        place: &Place,
    ) -> Option<Vec<(TexToken, Place)>> {
        match name {
            "xspace" => {
                // xspace inserts a space if the next token is alphanumeric
                let insert_space =
                    matches!(iter.peek_token(), Some(TexToken::Char(c)) if c.is_alphanumeric());
                if insert_space {
                    Some(vec![(TexToken::Space, place.clone())])
                } else {
                    Some(Vec::new())
                }
            }
            "iftrue" => Some(self.handle_if_conditional(true, iter)),
            "iffalse" => Some(self.handle_if_conditional(false, iter)),
            "ifmmode" => Some(self.handle_if_conditional(self.config.math_mode, iter)),
            "ifx" => self.handle_ifx(iter),
            "ifstrequal" => {
                iter.start_recording();
                let (branch, span) = self.handle_ifstrequal(iter);
                let places = iter.take_recorded();
                // Each token of the branch at the place it was read at
                let places = places.get(span).unwrap_or_default().iter().cloned();
                let places = places.chain(std::iter::repeat(place.clone()));
                Some(branch.into_inner().into_iter().zip(places).collect())
            }
            "else" | "fi" => Some(Vec::new()),
            _ => None,
        }
    }

    /// Handle \iftrue or \iffalse (or any boolean conditional): read up to
    /// the matching `\fi` and return the selected branch, each token with
    /// the level it was read at.
    fn handle_if_conditional(
        &self,
        condition: bool,
        iter: &mut InputStack,
    ) -> Vec<(TexToken, Place)> {
        let mut branch = Vec::new();
        let mut depth = 1;
        let mut in_else = false;

        while let Some(token) = iter.next() {
            if let TexToken::ControlSeq(cmd) = &token {
                match cmd.as_str() {
                    "iftrue" | "iffalse" | "ifmmode" | "ifstrequal" | "ifx" | "if" | "ifnum"
                    | "ifdim" | "ifcat" | "ifvoid" | "ifhbox" | "ifvbox" | "ifinner" | "ifcase" => {
                        depth += 1;
                    }
                    "else" if depth == 1 => {
                        in_else = true;
                        continue;
                    }
                    "fi" => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            // The true branch is what comes before the `\else`
            if in_else != condition {
                branch.push((token, iter.place()));
            }
        }
        branch
    }

    /// Handle \ifstrequal{str1}{str2}{true}{false}: the selected branch, and
    /// where its tokens are among the places recorded.
    fn handle_ifstrequal(&self, iter: &mut InputStack) -> (TokenList, Range<usize>) {
        let (str1, _) = read_recorded_argument(iter);
        let (str2, _) = read_recorded_argument(iter);
        let true_branch = read_recorded_argument(iter);
        let false_branch = read_recorded_argument(iter);

        let str1_text: String = str1.as_slice().iter().map(|t| format!("{}", t)).collect();
        let str2_text: String = str2.as_slice().iter().map(|t| format!("{}", t)).collect();

        if str1_text.trim() == str2_text.trim() {
            true_branch
        } else {
            false_branch
        }
    }

    /// Handle \ifx - compare two tokens without expansion
    /// \ifx compares the *next two tokens* for equality.
    /// Two control sequences are equal if they have the same meaning (same macro definition).
    /// Two character tokens are equal if they have the same character and catcode.
    fn handle_ifx(&self, iter: &mut InputStack) -> Option<Vec<(TexToken, Place)>> {
        utils::skip_spaces_in(iter);

        // Read first token
        let tok1 = iter.next()?;

        utils::skip_spaces_in(iter);

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
    fn skip_explsyntax_block<S: TokenSource>(&self, iter: &mut S) -> Vec<TexToken> {
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
    fn skip_primitive_args<S: TokenSource>(&self, name: &str, iter: &mut S) {
        match name {
            // \catcode`\@=11 format - skip until newline or space-separated
            "catcode" | "lccode" | "uccode" | "sfcode" | "mathcode" => {
                // Skip tokens until we see something that looks like end of assignment
                // Typically: \catcode`\@=11 or \catcode`@=11
                let mut found_equals = false;
                while let Some(token) = iter.peek_token() {
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
    /// Returns Ok((args, default_used, spans)) on success, where
    /// `default_used` tells that the first argument is the default of an
    /// omitted optional argument and `spans` where each argument's tokens
    /// are among the places recorded (see [`Engine::read_arguments`]), or
    /// Err((error, consumed_tokens)) on failure
    fn parse_arguments(
        &self,
        iter: &mut InputStack,
        macro_def: &MacroDef,
    ) -> Result<(Vec<TokenList>, bool, Vec<Span>), ArgsError> {
        match &macro_def.signature {
            MacroSignature::Simple(num_args) => {
                let mut args = Vec::new();
                let mut spans = Vec::new();
                let mut default_used = false;

                let start_idx = if macro_def.default_arg.is_some() {
                    utils::skip_spaces_in(iter);
                    if iter.peek_token() == Some(&TexToken::Char('[')) {
                        iter.next();
                        let start = iter.recorded_len();
                        let arg = utils::read_until_char_in(iter, ']');
                        spans.push(Some(start..start + arg.len()));
                        args.push(arg);
                    } else if let Some(default) = &macro_def.default_arg {
                        args.push(default.clone());
                        spans.push(None);
                        default_used = true;
                    }
                    1
                } else {
                    0
                };

                for _ in start_idx..*num_args {
                    let (arg, span) = read_recorded_argument(iter);
                    spans.push(Some(span));
                    args.push(arg);
                }

                Ok((args, default_used, spans))
            }
            MacroSignature::Pattern(parts) => self
                .parse_arguments_pattern(iter, parts)
                .map(|(args, spans)| (args, false, spans)),
        }
    }

    /// Parse arguments according to a delimited pattern
    fn parse_arguments_pattern(
        &self,
        iter: &mut InputStack,
        parts: &[PatternPart],
    ) -> Result<(Vec<TokenList>, Vec<Span>), ArgsError> {
        // Pre-allocate args (max 9 arguments in TeX)
        let mut args: Vec<TokenList> = vec![TokenList::new(); 9];
        let mut spans: Vec<Span> = vec![None; 9];
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
                        while let Some(t) = iter.peek_token() {
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
                            let start = iter.recorded_len();
                            match self.read_delimited_argument(iter, delimiter, &mut consumed) {
                                Ok(arg_tokens) => {
                                    if *arg_idx > 0 && (*arg_idx as usize) <= 9 {
                                        let span = start..start + arg_tokens.len();
                                        spans[(*arg_idx as usize) - 1] = Some(span);
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
                            let start = iter.recorded_len();
                            let (arg_tokens, arg_consumed) = self.read_argument_tracked(iter);
                            let skipped = arg_consumed
                                .iter()
                                .take_while(|t| matches!(t, TexToken::Space | TexToken::Comment(_)))
                                .count();
                            let braced = arg_consumed.get(skipped) == Some(&TexToken::BeginGroup);
                            let start = start + skipped + usize::from(braced);
                            consumed.extend(arg_consumed);
                            if *arg_idx > 0 && (*arg_idx as usize) <= 9 {
                                spans[(*arg_idx as usize) - 1] =
                                    Some(start..start + arg_tokens.len());
                                args[(*arg_idx as usize) - 1] = TokenList::from_vec(arg_tokens);
                            }
                        }
                    }
                }
            }
        }

        Ok((args, spans))
    }

    /// Read a single argument and track consumed tokens
    fn read_argument_tracked<S: TokenSource>(
        &self,
        iter: &mut S,
    ) -> (Vec<TexToken>, Vec<TexToken>) {
        let mut consumed = Vec::new();

        // Skip spaces
        while let Some(t) = iter.peek_token() {
            if matches!(t, TexToken::Space | TexToken::Comment(_)) {
                // SAFETY: peek() returned Some, so next() is guaranteed to return Some
                consumed.push(iter.next().expect("peek succeeded"));
            } else {
                break;
            }
        }

        let content = match iter.peek_token() {
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
    fn read_delimited_argument<S: TokenSource>(
        &self,
        iter: &mut S,
        delimiter: &[TexToken],
        consumed: &mut Vec<TexToken>,
    ) -> Result<Vec<TexToken>, MacroError> {
        let mut arg_content = Vec::new();
        let mut depth: i32 = 0;
        let mut count = 0;
        // Where a limited reader gives up (see `TokenSource::unbounded`)
        let mut cut = None;

        loop {
            if count > utils::MAX_ARG_TOKENS {
                if !iter.unbounded() {
                    return Err(MacroError::RunawayArgument);
                }
                cut.get_or_insert(consumed.len());
            }

            // At depth 0, try to match delimiter
            if depth == 0 && !delimiter.is_empty() {
                // Check if first delimiter token matches
                if let Some(first_delim) = delimiter.first() {
                    if let Some(next) = iter.peek_token() {
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
                // The input ended: what a limited reader would not have
                // read is read again
                if let Some(cut) = cut {
                    iter.give_back(consumed.split_off(cut));
                }
                return Err(MacroError::RunawayArgument);
            }
        }
    }

    /// Read the arguments of a use of `macro_def`, read at `place`, from
    /// `input`. Also returns the places of the tokens read.
    ///
    /// The replacement text goes one level deeper than the use, or than the
    /// lowest document text the arguments take: a macro that goes on through
    /// the document, a token or group at a time (`\def\x#1{...\x}`), nests
    /// only in that text, and each such step starts a new top-level
    /// expansion. A tail call adds no depth either: a use read from an
    /// expansion whose text has all been read (with these arguments), when
    /// that expansion has, since it started, read input that was pending
    /// below it, as a loop over a list stored in a macro does
    /// (`\expandafter\loop\items`); its text replaces the caller's, at the
    /// caller's level. Recursion that reads no such input nests deeper.
    fn read_arguments(
        &mut self,
        input: &mut InputStack,
        macro_def: &MacroDef,
        place: &Place,
    ) -> (Result<Args, ArgsError>, Vec<Place>) {
        let level = place.level;
        input.start_recording();
        let parsed = self.parse_arguments(input, macro_def);
        let places = input.take_recorded();
        let lower = places
            .iter()
            .filter(|p| p.doc && p.level < level)
            .map(|p| p.level)
            .min();
        if lower.is_some() && parsed.is_ok() {
            input.start_top_level();
            self.context.current_token_count = 0;
        }
        let (level, parent) = match (lower, &place.src) {
            (Some(lower), _) => (lower + 1, None),
            (None, Some(caller)) if caller.is_exhausted() && input.progressed(caller) => {
                (level, caller.parent.clone())
            }
            (None, src) => (level + 1, src.clone()),
        };
        let places_of = |span: Span| {
            span.and_then(|span| places.get(span))
                .unwrap_or_default()
                .to_vec()
        };
        let parsed = parsed.map(|(list, default_used, spans)| Args {
            list,
            default_used,
            places: spans.into_iter().map(places_of).collect(),
            level,
            parent,
        });
        (parsed, places)
    }

    /// Substitute parameter tokens with actual arguments. The result is given
    /// as runs of tokens that are document text or not: in the first copy of
    /// an argument, its document text stays document text, whatever braces
    /// or delimiters it was read with (see [`Engine::expansion_cost`]);
    /// further copies and a default argument are not. Every copy of an
    /// argument is a run of its own (and so is every part of it read from a
    /// frame of its own), so `@` is never glued across them.
    fn substitute_args(&self, body: &TokenList, args: &Args) -> Vec<(Vec<TexToken>, bool)> {
        let mut copied = vec![false; args.list.len()];
        let mut runs = Runs::default();
        for token in body.as_slice() {
            match token {
                TexToken::Param(n) => {
                    let idx = (*n as usize).saturating_sub(1);
                    if let Some(arg) = args.list.get(idx) {
                        let places = args.places.get(idx).map_or(&[][..], Vec::as_slice);
                        let first = !std::mem::replace(&mut copied[idx], true);
                        runs.split();
                        for (i, t) in arg.as_slice().iter().enumerate() {
                            let place = places.get(i);
                            let doc = first && place.is_some_and(|p| p.doc);
                            runs.push(t.clone(), doc, place.is_some_and(|p| p.sep));
                        }
                        runs.split();
                    }
                }
                TexToken::DeferredParam(n) => {
                    // Degrade DeferredParam to Param during expansion.
                    // This is the key mechanism for nested macro definitions:
                    // \def\outer#1{\def\inner##1{##1 and #1}}
                    // When \outer is expanded, ##1 becomes #1 for \inner.
                    runs.push(TexToken::Param(*n), false, false);
                }
                _ => runs.push(token.clone(), false, false),
            }
        }
        runs.finish()
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
