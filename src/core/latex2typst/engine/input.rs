//! Input stack for the expansion engine.
//!
//! As in TeX, the replacement text of a macro is pushed in front of the
//! remaining input and read from there, instead of being expanded by a
//! recursive call over "replacement + rest of the document". The input stack
//! therefore holds only what is actually pending.
//!
//! Every token on the stack carries its [`Place`]: the nesting level of the
//! expansion that produced it (0 for the text passed to the engine, one more
//! than the level of the macro or environment whose replacement text it
//! belongs to), that expansion itself (see [`Expansion`]), and whether it is
//! document text: the text passed to the engine, also where the engine reads
//! it and puts it back, and in the first copy of a macro argument (or in an
//! `\edef` body), at whatever level the macro put it. Tokens that the engine
//! reads and puts back (conditional branches, `\expandafter`, `\csname`)
//! keep their place, so a runaway recursion keeps nesting deeper however its
//! recursive call is reached, while macros used one after another never
//! accumulate depth.
//!
//! Text that is expanded on its own (environment code, an `\edef` body) is
//! pushed as a collecting frame: reading stops at its end until the engine
//! has finished it, and the engine writes its expansion apart. The name
//! inside `\csname` is collected the same way, but, as in TeX, it is read
//! from the input with expansion up to the `\endcsname` that ends it. Such
//! expansions therefore nest on this stack too, not in Rust recursion.
//!
//! Each frame holds tokens that were written next to each other; `@` is
//! glued onto a control sequence (under `\makeatletter`) only within a frame
//! (see [`InputStack::merge_at_letters`]).
//!
//! The stack also keeps what is needed to undo an expansion (see
//! [`InputStack::pending`]): the tokens read since the last checkpoint from
//! below it.

use super::primitives::MacroSignature;
use super::token::TexToken;
use super::utils::{TokenSource, MAX_ARG_TOKENS};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

/// Marks, in the tokens of the document and of macro bodies, where a space
/// separated a control word from the `@` after it (see
/// [`super::lexer::tokenize_marked`]): the two are never glued into one name.
/// It never reaches the input stack (see [`Runs`]) or the output.
pub(crate) const BOUNDARY: TexToken = TexToken::EndOfInput;

/// What the engine does with the expansion of a collecting frame.
#[derive(Debug)]
pub(crate) enum Collect {
    /// The body of an `\edef`: defines `name`.
    Edef {
        name: String,
        signature: MacroSignature,
        global: bool,
    },
    /// Environment begin code: written out after an opening brace.
    BeginEnv,
    /// The end code of environment `name`: written out, then the group of
    /// the environment is closed (see [`InputStack::close_env`]).
    EndEnv { name: String },
}

/// One expansion of a macro or environment (`name`), or of an `\edef` body
/// (no name): the expansion its use was read from (`parent`), so the chain
/// of expansions a token is nested in can be followed, and what is needed
/// to tell a loop that goes on through pending input from a recursion that
/// does not (see [`InputStack::progressed`]).
#[derive(Debug)]
pub(crate) struct Expansion {
    pub(crate) name: Option<String>,
    /// The level the expansion was used at.
    pub(crate) level: usize,
    pub(crate) parent: Option<Rc<Expansion>>,
    /// Frames pushed from its start on have this serial number or a higher
    /// one.
    serial: u64,
    /// Number of tokens read before it started.
    clock: u64,
    /// Number of its tokens on the stack or set aside to be put back.
    pending: Cell<usize>,
    /// A macro use read from it was refused.
    refused: Cell<bool>,
}

impl Expansion {
    /// Whether all of its tokens have been read.
    pub(crate) fn is_exhausted(&self) -> bool {
        self.pending.get() == 0
    }

    /// Whether a macro use read from it was refused.
    pub(crate) fn had_refusal(&self) -> bool {
        self.refused.get()
    }

    /// The outermost expansion of `name` among `src` and the expansions it
    /// is nested in.
    pub(crate) fn outermost(src: Option<&Rc<Expansion>>, name: &str) -> Option<Rc<Expansion>> {
        let mut found = None;
        let mut current = src;
        while let Some(e) = current {
            if e.name.as_deref() == Some(name) {
                found = Some(e.clone());
            }
            current = e.parent.as_ref();
        }
        found
    }

    /// Whether `name` is among `src` and the expansions it is nested in.
    pub(crate) fn contains(src: Option<&Rc<Expansion>>, name: &str) -> bool {
        let mut current = src;
        while let Some(e) = current {
            if e.name.as_deref() == Some(name) {
                return true;
            }
            current = e.parent.as_ref();
        }
        false
    }

    /// Whether a use of `name` read from `src` is a runaway recursion: some
    /// macro is being expanded inside an expansion of itself.
    pub(crate) fn is_runaway(src: Option<&Rc<Expansion>>, name: &str) -> bool {
        let mut seen = HashSet::from([name]);
        let mut current = src;
        while let Some(e) = current {
            if e.name.as_deref().is_some_and(|n| !seen.insert(n)) {
                return true;
            }
            current = e.parent.as_ref();
        }
        false
    }
}

impl Drop for Expansion {
    /// Drop a long chain of expansions without recursion.
    fn drop(&mut self) {
        let mut parent = self.parent.take();
        while let Some(p) = parent {
            parent = Rc::try_unwrap(p).ok().and_then(|mut p| p.parent.take());
        }
    }
}

/// Where a token comes from: the nesting level and the expansion that
/// produced it, and whether it is document text. A token read from the
/// stack also tells whether it starts the frame it was read from (`sep`):
/// nothing written before it can be glued to it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Place {
    pub(crate) level: usize,
    pub(crate) doc: bool,
    pub(crate) src: Option<Rc<Expansion>>,
    pub(crate) sep: bool,
}

impl Place {
    /// Document text at `level`.
    pub(crate) fn text(level: usize) -> Place {
        Place {
            level,
            doc: true,
            ..Place::default()
        }
    }

    /// The place of a token made from tokens at `self` and at `other`: the
    /// deeper of the two (`other` if they are as deep).
    pub(crate) fn join(self, other: &Place) -> Place {
        let deeper = if self.level > other.level {
            self.src
        } else {
            other.src.clone()
        };
        Place {
            level: self.level.max(other.level),
            doc: self.doc && other.doc,
            src: deeper,
            sep: false,
        }
    }

    /// A token read from here is set aside, to be put back later: it is
    /// still pending.
    pub(crate) fn hold(&self) {
        if let Some(src) = &self.src {
            src.pending.set(src.pending.get() + 1);
        }
    }

    /// A token set aside (see [`Place::hold`]) is put back.
    pub(crate) fn unhold(&self) {
        if let Some(src) = &self.src {
            src.pending.set(src.pending.get() - 1);
        }
    }

    /// A macro use read from here was refused.
    pub(crate) fn note_refused(&self) {
        if let Some(src) = &self.src {
            src.refused.set(true);
        }
    }

    fn same_frame(&self, other: &Place) -> bool {
        self.key() == other.key()
            && match (&self.src, &other.src) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (a, b) => a.is_none() && b.is_none(),
            }
    }

    /// What the place is compared by when replaying an undone expansion.
    fn key(&self) -> (usize, bool) {
        (self.level, self.doc)
    }

    fn bare(&self) -> Place {
        Place {
            level: self.level,
            doc: self.doc,
            ..Place::default()
        }
    }
}

/// Builds the frames of a replacement text: runs of tokens that are document
/// text or not, a new run starting wherever tokens were not written next to
/// each other (a [`BOUNDARY`], or a token that started its frame).
#[derive(Debug, Default)]
pub(crate) struct Runs {
    runs: Vec<(Vec<TexToken>, bool)>,
    open: bool,
}

impl Runs {
    /// `tokens` (document text if `doc`) as runs.
    pub(crate) fn of(tokens: impl IntoIterator<Item = TexToken>, doc: bool) -> Self {
        let mut runs = Runs::default();
        for token in tokens {
            runs.push(token, doc, false);
        }
        runs
    }

    /// Add a token; `sep` if it starts a new run whatever it follows.
    pub(crate) fn push(&mut self, token: TexToken, doc: bool, sep: bool) {
        if token == BOUNDARY {
            self.split();
            return;
        }
        match self.runs.last_mut() {
            Some((run, run_doc)) if self.open && !sep && *run_doc == doc => run.push(token),
            _ => {
                self.runs.push((vec![token], doc));
                self.open = true;
            }
        }
    }

    /// What follows starts a new run.
    pub(crate) fn split(&mut self) {
        self.open = false;
    }

    pub(crate) fn finish(self) -> Vec<(Vec<TexToken>, bool)> {
        self.runs
    }
}

/// The engine's state at a checkpoint, to undo what follows it.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Checkpoint {
    /// Length of the output being written.
    pub(crate) out_len: usize,
    /// Number of outputs of enclosing collecting frames.
    pub(crate) saved_len: usize,
    /// Number of open macro scopes.
    pub(crate) scopes: usize,
    /// Version of the macro definitions.
    pub(crate) generation: u64,
    /// Tokens charged to the current top-level expansion.
    pub(crate) count: usize,
    pub(crate) math_mode: bool,
    pub(crate) at_letter: bool,
    /// Number of refused names (set by [`InputStack::commit`]).
    pub(crate) refused: usize,
}

/// An expansion undone for exceeding the token limit, started at a
/// checkpoint with nothing charged or refused yet: the tokens it read from
/// below the checkpoint, the token after them, the state it started in and
/// whether it was a runaway recursion. Reading the same tokens in the same
/// state undoes it again.
#[derive(Debug)]
struct Undone {
    tokens: Vec<(TexToken, Place)>,
    next: Option<(TexToken, Place)>,
    generation: u64,
    math_mode: bool,
    runaway: bool,
}

/// A name refused within an expansion: from level `from` on, as long as the
/// expansion it is anchored to has tokens pending (always, without one); or,
/// if `recursive`, only where it is used inside an expansion of itself.
#[derive(Debug)]
struct Refusal {
    name: String,
    from: usize,
    anchor: Option<Weak<Expansion>>,
    recursive: bool,
    seq: u64,
}

impl Refusal {
    fn is_alive(&self) -> bool {
        self.anchor.as_ref().is_none_or(|a| a.strong_count() > 0)
    }

    fn refuses(&self, name: &str, level: usize, src: Option<&Rc<Expansion>>) -> bool {
        self.name == name
            && if self.recursive {
                Expansion::contains(src, name)
            } else {
                level >= self.from
            }
    }
}

/// A `\csname` whose name is being read: the place of the control sequence
/// it makes, the frames that were pending when it started (`serial`), the
/// number of collecting frames open then, and the tokens `\expandafter` set
/// aside, put back after the control sequence.
#[derive(Debug)]
struct Name {
    place: Place,
    serial: u64,
    frames: usize,
    then: Vec<(TexToken, Place)>,
}

/// An open environment, and the ones it is nested in.
#[derive(Debug)]
struct Env {
    name: String,
    up: Option<Rc<Env>>,
}

impl Drop for Env {
    fn drop(&mut self) {
        let mut up = self.up.take();
        while let Some(e) = up {
            up = Rc::try_unwrap(e).ok().and_then(|mut e| e.up.take());
        }
    }
}

#[derive(Debug)]
pub(crate) struct Frame {
    /// Remaining tokens, stored in reverse so the next token is at the end.
    tokens: Vec<TexToken>,
    /// Where every token in the frame comes from.
    pub(crate) place: Place,
    /// Frames pushed later have higher serial numbers.
    serial: u64,
    /// A token has been read from it.
    started: bool,
    /// Index of the frame that stands for the expansion this frame belongs
    /// to (its first frame), or of the frame itself.
    owner: usize,
    /// It stands for the expansion of its place.
    origin: bool,
    /// Tokens read from the expansion, and refused macro uses among them.
    reads: usize,
    refusals: usize,
    /// Part of an expansion that started after the checkpoint.
    live: bool,
    pub(crate) collect: Option<Collect>,
}

impl Frame {
    pub(crate) fn is_exhausted(&self) -> bool {
        self.tokens.is_empty()
    }
}

/// A stack of token frames, read from the top down.
#[derive(Debug)]
pub(crate) struct InputStack {
    frames: Vec<Frame>,
    /// Number of collecting frames on the stack.
    collectors: usize,
    /// Place of the token read last, and index of its frame.
    place: Place,
    last_frame: usize,
    /// Places of the tokens read since [`InputStack::start_recording`].
    recorded: Option<Vec<Place>>,
    /// Tokens read since [`InputStack::keep_tokens`].
    kept: Option<Vec<TexToken>>,
    at_letter: bool,
    /// An argument ran into the end of the input (see [`TokenSource`]).
    ran_off_end: bool,
    /// A definition is being read: its bodies keep [`BOUNDARY`]s.
    pub(crate) marking: bool,
    /// Serial number of the next frame, number of tokens read, and for the
    /// tokens read so far, the lowest frame serial read from since each
    /// time (see [`InputStack::progressed`]).
    next_serial: u64,
    clock: u64,
    lows: Vec<(u64, u64)>,
    /// `\csname` names being read, innermost last.
    names: Vec<Name>,
    /// The innermost open environment, at present and at the checkpoint.
    envs: Option<Rc<Env>>,
    checkpoint_envs: Option<Rc<Env>>,
    /// Frames below this index were on the stack at the checkpoint.
    base: usize,
    /// Number of non-exhausted expansion frames (and collecting frames)
    /// pushed since the checkpoint.
    live: usize,
    /// Tokens read from below `base` since the checkpoint.
    log: Vec<(TexToken, Place)>,
    /// The end of the input (or of a collecting frame below `base`) was
    /// reached since the checkpoint.
    hit_end: bool,
    /// The engine's state at the checkpoint.
    pub(crate) checkpoint: Checkpoint,
    /// Expansions undone for exceeding the token limit, by first token.
    undone: HashMap<String, Vec<Undone>>,
    /// The token limit was exceeded: undo back to the checkpoint.
    pub(crate) exceeded: bool,
    /// ... by a runaway recursion.
    pub(crate) runaway: bool,
    /// Refuse every expansion until the next token is read.
    pub(crate) refusing: bool,
    /// `DepthExceeded` was reported in the current top-level expansion.
    pub(crate) depth_warned: bool,
    /// `TokenLimitExceeded` was reported in the current top-level expansion.
    pub(crate) token_warned: bool,
    /// Names refused in the current top-level expansion; number of
    /// refusals made so far, and at the checkpoint.
    refused: Vec<Refusal>,
    refusal_seq: u64,
    checkpoint_seq: u64,
}

impl InputStack {
    /// Create a stack reading document text `tokens` at nesting level
    /// `level`.
    pub(crate) fn new(tokens: Vec<TexToken>, level: usize) -> Self {
        let place = Place::text(level);
        let mut stack = InputStack {
            frames: Vec::new(),
            collectors: 0,
            place: place.clone(),
            last_frame: 0,
            recorded: None,
            kept: None,
            at_letter: false,
            ran_off_end: false,
            marking: false,
            next_serial: 0,
            clock: 0,
            lows: Vec::new(),
            names: Vec::new(),
            envs: None,
            checkpoint_envs: None,
            base: 0,
            live: 0,
            log: Vec::new(),
            hit_end: false,
            checkpoint: Checkpoint::default(),
            undone: HashMap::new(),
            exceeded: false,
            runaway: false,
            refusing: false,
            depth_warned: false,
            token_warned: false,
            refused: Vec::new(),
            refusal_seq: 0,
            checkpoint_seq: 0,
        };
        let runs = Runs::of(tokens, true).finish();
        if runs.is_empty() {
            stack.push_frame(Vec::new(), place.clone(), None);
        }
        for (run, _) in runs.into_iter().rev() {
            stack.push_frame(run, place.clone(), None);
        }
        stack
    }

    fn push_frame(&mut self, mut tokens: Vec<TexToken>, place: Place, owner: Option<usize>) {
        tokens.reverse();
        let live = owner.is_some() && !tokens.is_empty();
        self.live += usize::from(live);
        if let Some(src) = &place.src {
            src.pending.set(src.pending.get() + tokens.len());
        }
        let index = self.frames.len();
        self.frames.push(Frame {
            tokens,
            place,
            serial: self.next_serial,
            started: false,
            owner: owner.unwrap_or(index),
            origin: false,
            reads: 0,
            refusals: 0,
            live,
            collect: None,
        });
        self.next_serial += 1;
    }

    /// Put `tokens`, from `place`, in front of the remaining input.
    pub(crate) fn push(&mut self, tokens: Vec<TexToken>, place: Place) {
        self.drop_exhausted();
        self.push_frame(tokens, place, None);
    }

    /// A new expansion of `name` (`None` for an `\edef` body), used at
    /// `level` inside `parent`; its text is pushed next (see
    /// [`InputStack::push_expansion`]).
    pub(crate) fn new_expansion(
        &self,
        name: Option<&str>,
        level: usize,
        parent: Option<Rc<Expansion>>,
    ) -> Rc<Expansion> {
        Rc::new(Expansion {
            name: name.map(str::to_string),
            level,
            parent,
            serial: self.next_serial,
            clock: self.clock,
            pending: Cell::new(0),
            refused: Cell::new(false),
        })
    }

    /// Put the replacement text of `expansion` in front of the remaining
    /// input at nesting level `level`, collecting its expansion if `collect`
    /// is given. The text is given as runs of tokens that are document text
    /// or not (see [`Runs`]); the frame of the last run stands for the whole
    /// expansion.
    pub(crate) fn push_expansion(
        &mut self,
        runs: Vec<(Vec<TexToken>, bool)>,
        level: usize,
        expansion: Rc<Expansion>,
        collect: Option<Collect>,
    ) {
        self.drop_exhausted();
        let owner = self.frames.len();
        let place = |doc| Place {
            level,
            doc,
            src: Some(expansion.clone()),
            sep: false,
        };
        let mut runs = runs.into_iter().rev();
        let (tokens, doc) = runs.next().unwrap_or_default();
        self.push_frame(tokens, place(doc), Some(owner));
        let frame = &mut self.frames[owner];
        if collect.is_some() {
            self.collectors += 1;
            self.live += usize::from(!frame.live);
            frame.live = true;
        }
        frame.origin = true;
        frame.collect = collect;
        for (tokens, doc) in runs {
            self.push_frame(tokens, place(doc), Some(owner));
        }
    }

    /// Put tokens that keep their own places back in front of the input;
    /// a token that started its frame starts one again.
    pub(crate) fn push_leveled(&mut self, tokens: Vec<(TexToken, Place)>) {
        let mut runs: Vec<(Vec<TexToken>, Place)> = Vec::new();
        for (token, place) in tokens {
            match runs.last_mut() {
                Some((run, run_place)) if !place.sep && run_place.same_frame(&place) => {
                    run.push(token)
                }
                _ => runs.push((vec![token], place)),
            }
        }
        for (run, mut place) in runs.into_iter().rev() {
            place.sep = false;
            self.push(run, place);
        }
    }

    /// Number of collecting frames on the stack and `\csname` names being
    /// read: the expansions whose output is written apart.
    pub(crate) fn collectors(&self) -> usize {
        self.collectors + self.names.len()
    }

    /// Remove the collecting frame on top once all of it has been read.
    pub(crate) fn finish_collector(&mut self) -> Option<Frame> {
        self.drop_exhausted();
        let top = self.frames.last()?;
        if top.collect.is_none() || !top.is_exhausted() {
            return None;
        }
        self.collectors -= 1;
        let frame = self.frames.pop()?;
        self.release(self.frames.len(), &frame);
        Some(frame)
    }

    /// Frame `index`, which was `frame`, is done with.
    fn release(&mut self, index: usize, frame: &Frame) {
        if frame.live && index >= self.base {
            self.live -= 1;
        }
    }

    /// Start reading the name of a `\csname` read at `place`; `then` are
    /// the tokens `\expandafter` set aside.
    pub(crate) fn open_name(&mut self, place: Place, then: Vec<(TexToken, Place)>) {
        self.names.push(Name {
            place,
            serial: self.next_serial,
            frames: self.collectors,
            then,
        });
    }

    /// Whether a `\csname` name is being read.
    pub(crate) fn reading_name(&self) -> bool {
        !self.names.is_empty()
    }

    /// Whether the innermost expansion written apart is a `\csname` name.
    pub(crate) fn in_name(&self) -> bool {
        self.names
            .last()
            .is_some_and(|n| n.frames == self.collectors)
    }

    /// End the innermost `\csname` name: the place of the control sequence
    /// it makes (the deepest of the tokens it was read from, document text
    /// if they all are), and the tokens set aside for after it.
    pub(crate) fn close_name(&mut self) -> Option<(Place, Vec<(TexToken, Place)>)> {
        let name = self.names.pop()?;
        if let Some(outer) = self.names.last_mut() {
            let place = std::mem::take(&mut outer.place);
            outer.place = place.join(&name.place);
        }
        Some((name.place, name.then))
    }

    /// Open environment `name`.
    pub(crate) fn open_env(&mut self, name: &str) {
        self.envs = Some(Rc::new(Env {
            name: name.to_string(),
            up: self.envs.take(),
        }));
    }

    /// Close environment `name`, and the environments opened inside it that
    /// are still open; returns how many were closed (none if `name` is not
    /// open).
    pub(crate) fn close_env(&mut self, name: &str) -> usize {
        let mut count = 0;
        let mut env = self.envs.as_ref();
        while let Some(e) = env {
            count += 1;
            if e.name == name {
                break;
            }
            env = e.up.as_ref();
        }
        if env.is_none() {
            return 0;
        }
        for _ in 0..count {
            self.envs = self.envs.take().and_then(|e| match Rc::try_unwrap(e) {
                Ok(mut e) => e.up.take(),
                Err(e) => e.up.clone(),
            });
        }
        count
    }

    /// Place of the token read last.
    pub(crate) fn place(&self) -> Place {
        self.place.clone()
    }

    /// Whether, since `expansion` started, a token has been read from the
    /// input that was pending below it then.
    pub(crate) fn progressed(&self, expansion: &Expansion) -> bool {
        let since = self
            .lows
            .partition_point(|&(clock, _)| clock <= expansion.clock);
        self.lows
            .get(since)
            .is_some_and(|&(_, serial)| serial < expansion.serial)
    }

    /// Start recording the place of every token read.
    pub(crate) fn start_recording(&mut self) {
        self.recorded = Some(Vec::new());
    }

    /// Start keeping every token read.
    pub(crate) fn keep_tokens(&mut self) {
        self.kept = Some(Vec::new());
    }

    /// Stop keeping tokens; the tokens read since
    /// [`InputStack::keep_tokens`], in reading order.
    pub(crate) fn take_kept_tokens(&mut self) -> Vec<TexToken> {
        self.kept.take().unwrap_or_default()
    }

    /// Number of places recorded so far.
    pub(crate) fn recorded_len(&self) -> usize {
        self.recorded.as_ref().map_or(0, Vec::len)
    }

    /// Stop recording; the places of the tokens read since
    /// [`InputStack::start_recording`], in reading order.
    pub(crate) fn take_recorded(&mut self) -> Vec<Place> {
        self.recorded.take().unwrap_or_default()
    }

    /// A document token was read: a new top-level expansion starts.
    pub(crate) fn start_top_level(&mut self) {
        self.depth_warned = false;
        self.token_warned = false;
        self.refused.clear();
    }

    /// Whether `name`, used at `level` in expansion `src`, is refused.
    pub(crate) fn is_refused(
        &mut self,
        name: &str,
        level: usize,
        src: Option<&Rc<Expansion>>,
    ) -> bool {
        if self.refused.is_empty() {
            return false;
        }
        self.refused.retain(Refusal::is_alive);
        self.refused.iter().any(|r| r.refuses(name, level, src))
    }

    /// Refuse `name`, used at `level` in expansion `src`. If it is being
    /// expanded already (a runaway recursion), it is refused deeper than the
    /// outermost such expansion as long as that one has tokens pending, so
    /// a recursion that branches stops at once, while a later use of the
    /// name expands; for the rest of the top-level expansion, only a use of
    /// it inside an expansion of itself (a recursion again) is then refused
    /// at once. Else (nesting too deep) it is refused from `level` on for
    /// the rest of the top-level expansion, so that a finite expansion that
    /// branches stops at once too.
    pub(crate) fn refuse(&mut self, name: &str, level: usize, src: Option<&Rc<Expansion>>) {
        let root = Expansion::outermost(src, name);
        let runaway = root.is_some();
        let (from, anchor) = match root {
            Some(root) => (root.level + 1, Some(Rc::downgrade(&root))),
            None => (level, None),
        };
        self.add_refusal(name, from, anchor, false);
        if runaway && !self.refused.iter().any(|r| r.recursive && r.name == name) {
            self.add_refusal(name, 0, None, true);
        }
    }

    fn add_refusal(
        &mut self,
        name: &str,
        from: usize,
        anchor: Option<Weak<Expansion>>,
        recursive: bool,
    ) {
        self.refused.push(Refusal {
            name: name.to_string(),
            from,
            anchor,
            recursive,
            seq: self.refusal_seq,
        });
        self.refusal_seq += 1;
    }

    /// The macro use read last was refused.
    pub(crate) fn note_refusal(&mut self) {
        if let Some(owner) = self.frames.get(self.last_frame).map(|f| f.owner) {
            if let Some(frame) = self.frames.get_mut(owner) {
                frame.refusals += 1;
            }
        }
    }

    /// Treat `@` as a letter (`\makeatletter`) or not when reading control
    /// sequence names.
    pub(crate) fn set_at_letter(&mut self, at_letter: bool) {
        self.at_letter = at_letter;
    }

    /// Whether an expansion pushed since the checkpoint is still pending
    /// (or a `\csname` name is being read).
    pub(crate) fn pending(&self) -> bool {
        self.live > 0 || !self.names.is_empty()
    }

    /// Take a checkpoint: what is on the stack now stays if what follows is
    /// undone. If `first` (the token read last) is given, it counts as read
    /// after the checkpoint. Never taken while a `\csname` name is read.
    pub(crate) fn commit(&mut self, checkpoint: Checkpoint, first: Option<&TexToken>) {
        self.checkpoint = Checkpoint {
            refused: self.refused.len(),
            ..checkpoint
        };
        self.checkpoint_seq = self.refusal_seq;
        self.checkpoint_envs = self.envs.clone();
        let first = match first {
            Some(_) if self.last_frame < self.base => self.log.pop(),
            first => first.map(|t| (t.clone(), self.place.clone())),
        };
        self.base = self.frames.len();
        self.live = 0;
        self.hit_end = false;
        self.log.clear();
        self.log.extend(first);
    }

    /// Undo what was read and pushed since the checkpoint: drop the frames
    /// pushed since and put back the tokens read from below it, every one at
    /// its place. Remembered if the expansion started with nothing charged
    /// or refused, so that the same expansion in the same state is undone
    /// at once next time (see [`InputStack::was_undone`]).
    pub(crate) fn undo(&mut self) {
        for frame in self.frames.drain(self.base..) {
            self.collectors -= usize::from(frame.collect.is_some());
            if let Some(src) = &frame.place.src {
                src.pending.set(src.pending.get() - frame.tokens.len());
            }
        }
        let checkpoint_seq = self.checkpoint_seq;
        self.refused.retain(|r| r.seq < checkpoint_seq);
        for name in self.names.drain(..) {
            for (_, place) in &name.then {
                place.unhold();
            }
        }
        self.envs = self.checkpoint_envs.clone();
        let log = std::mem::take(&mut self.log);
        let checkpoint = self.checkpoint;
        let unchanged = checkpoint.count == 0 && checkpoint.refused == 0;
        let replayable = !self.hit_end && !checkpoint.at_letter && !self.ran_off_end;
        if let (true, true, Some((TexToken::ControlSeq(name), _))) =
            (unchanged, replayable, log.first())
        {
            let undone = Undone {
                next: self.upcoming().next().map(|(t, p)| (t.clone(), p.bare())),
                tokens: log.iter().map(|(t, p)| (t.clone(), p.bare())).collect(),
                generation: checkpoint.generation,
                math_mode: checkpoint.math_mode,
                runaway: self.runaway,
            };
            self.undone.entry(name.clone()).or_default().push(undone);
        }
        self.runaway = false;
        self.push_leveled(log);
        self.commit(checkpoint, None);
        self.refusing = true;
    }

    /// Whether the expansion of `name`, just read, was undone before in the
    /// same state with the same tokens following; if so, whether it was a
    /// runaway recursion.
    pub(crate) fn was_undone(&self, name: &str, generation: u64, math_mode: bool) -> Option<bool> {
        if self.undone.is_empty()
            || self.checkpoint.count != 0
            || self.checkpoint.at_letter
            || self.log.len() != 1
        {
            return None;
        }
        let list = self.undone.get(name)?;
        list.iter()
            .find(|u| {
                u.generation == generation
                    && u.math_mode == math_mode
                    && u.tokens[0].1.key() == self.place.key()
                    && self.upcoming_is(&u.tokens[1..], u.next.as_ref())
            })
            .map(|u| u.runaway)
    }

    /// The tokens that will be read next, with their places, up to the end
    /// of the input or of a collecting frame.
    fn upcoming(&self) -> impl Iterator<Item = (&TexToken, &Place)> + '_ {
        let reach = self
            .frames
            .iter()
            .rposition(|f| f.collect.is_some())
            .unwrap_or(0);
        self.frames[reach..]
            .iter()
            .rev()
            .flat_map(|f| f.tokens.iter().rev().map(move |t| (t, &f.place)))
    }

    fn upcoming_is(&self, tokens: &[(TexToken, Place)], next: Option<&(TexToken, Place)>) -> bool {
        let same = |a: Option<(&TexToken, &Place)>, b: Option<&(TexToken, Place)>| match (a, b) {
            (Some((t, p)), Some((u, q))) => t == u && p.key() == q.key(),
            (a, b) => a.is_none() && b.is_none(),
        };
        let mut upcoming = self.upcoming();
        tokens.iter().all(|t| same(upcoming.next(), Some(t))) && same(upcoming.next(), next)
    }

    /// Drop exhausted frames above the bottom frame. A collecting frame stays
    /// until the engine finishes it, so reading stops at its end. An
    /// expansion that read nothing but refused macro uses refuses its own
    /// name where it was used.
    fn drop_exhausted(&mut self) {
        while self.frames.len() > 1
            && self
                .frames
                .last()
                .is_some_and(|f| f.is_exhausted() && f.collect.is_none())
        {
            let Some(frame) = self.frames.pop() else {
                break;
            };
            if frame.origin && frame.refusals > 0 && frame.refusals == frame.reads {
                if let Some(e) = &frame.place.src {
                    if let Some(name) = &e.name {
                        self.refuse(name, e.level, e.parent.as_ref());
                    }
                }
            }
        }
        self.base = self.base.min(self.frames.len());
    }

    /// Under `\makeatletter`, glue `@` and the letters around it onto the
    /// control sequence that is about to be read (`\foo` `@` `b` `a` `r`
    /// becomes `\foo@bar`). Only tokens written right after it (in the same
    /// frame) are glued, and only the next token is affected, so text that
    /// is read later is left alone; letters are glued only once an `@` has
    /// been seen, so `\alpha x` stays `\alpha` followed by `x`.
    fn merge_at_letters(&mut self) {
        let Some(frame) = self.frames.last_mut() else {
            return;
        };
        let top = frame.tokens.len();
        let Some(TexToken::ControlSeq(name)) = frame.tokens.last() else {
            return;
        };
        let mut glue = name.ends_with('@');
        let mut start = top - 1;
        while start > 0 {
            match &frame.tokens[start - 1] {
                TexToken::Char('@') => glue = true,
                TexToken::Char(c) if glue && c.is_ascii_alphabetic() => {}
                _ => break,
            }
            start -= 1;
        }
        if start == top - 1 {
            return;
        }
        let suffix: String = frame
            .tokens
            .drain(start..top - 1)
            .rev()
            .filter_map(|t| match t {
                TexToken::Char(c) => Some(c),
                _ => None,
            })
            .collect();
        if let Some(src) = &frame.place.src {
            src.pending.set(src.pending.get() - suffix.len());
        }
        if let Some(TexToken::ControlSeq(name)) = frame.tokens.last_mut() {
            name.push_str(&suffix);
        }
    }

    /// Prepare the next token; `false` at the end of the input or of a
    /// collecting frame.
    fn prepare_top(&mut self) -> bool {
        self.drop_exhausted();
        if self.at_letter {
            self.merge_at_letters();
        }
        let more = self.frames.last().is_some_and(|f| !f.is_exhausted());
        if !more && self.frames.len() <= self.base {
            self.hit_end = true;
        }
        more
    }
}

impl Iterator for InputStack {
    type Item = TexToken;

    fn next(&mut self) -> Option<TexToken> {
        if !self.prepare_top() {
            return None;
        }
        let index = self.frames.len() - 1;
        let frame = &mut self.frames[index];
        let token = frame.tokens.pop()?;
        let mut place = frame.place.clone();
        place.sep = !std::mem::replace(&mut frame.started, true);
        let (owner, serial) = (frame.owner, frame.serial);
        if frame.is_exhausted() && frame.collect.is_none() && frame.live && index >= self.base {
            self.live -= 1;
        }
        if let Some(src) = &place.src {
            src.pending.set(src.pending.get() - 1);
        }
        self.frames[owner].reads += 1;
        self.last_frame = index;
        // The lowest frame read from since each earlier token
        self.clock += 1;
        while self.lows.last().is_some_and(|&(_, low)| low >= serial) {
            self.lows.pop();
        }
        self.lows.push((self.clock, serial));
        // A token that was pending when a `\csname` started is part of what
        // its name is read from
        if let Some(name) = self.names.last_mut() {
            if serial < name.serial {
                let joined = std::mem::take(&mut name.place);
                name.place = joined.join(&place);
            }
        }
        if let Some(recorded) = &mut self.recorded {
            recorded.push(place.clone());
        }
        if let Some(kept) = &mut self.kept {
            kept.push(token.clone());
        }
        // A token other than a control sequence read first after the
        // checkpoint starts no expansion: it is not needed to undo one
        let first = self.log.is_empty() && self.live == 0;
        if index < self.base && !(first && !matches!(token, TexToken::ControlSeq(_))) {
            self.log.push((token.clone(), place.clone()));
        }
        self.place = place;
        Some(token)
    }
}

impl TokenSource for InputStack {
    fn peek_token(&mut self) -> Option<&TexToken> {
        if !self.prepare_top() {
            return None;
        }
        self.frames.last()?.tokens.last()
    }

    fn unbounded(&self) -> bool {
        !self.ran_off_end
    }

    /// The tokens given back were the last ones read: each goes back at its
    /// place, and is no longer recorded.
    fn give_back(&mut self, tokens: Vec<TexToken>) {
        if tokens.len() > MAX_ARG_TOKENS {
            self.ran_off_end = true;
        }
        let read = match &mut self.recorded {
            Some(recorded) => recorded.split_off(recorded.len().saturating_sub(tokens.len())),
            None => Vec::new(),
        };
        if let Some(kept) = &mut self.kept {
            kept.truncate(kept.len().saturating_sub(tokens.len()));
        }
        // Tokens read before recording started go back at the last place
        let mut places = vec![self.place.clone(); tokens.len() - read.len()];
        places.extend(read);
        self.push_leveled(tokens.into_iter().zip(places).collect());
    }

    /// In a definition, the token read last starts a frame.
    fn boundary(&self) -> bool {
        self.marking && self.place.sep
    }
}
