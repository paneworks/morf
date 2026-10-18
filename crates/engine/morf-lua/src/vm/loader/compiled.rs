//! Chunks compiled once per process: every output's runtime, and every
//! reload, runs the same files, and parsing them again each time was a
//! quarter of what loading a large configuration cost.
//!
//! What is kept is the compiler's output with its strings owned, outside
//! any VM; a runtime takes a chunk by making its prototypes in its own heap
//! from that, which is a copy rather than a parse. A file is found by its
//! chunk name and taken only while its bytes are the same.

use luna::compiler::{self, CompiledPrototype, StringInterner};
use luna::{Closure, CompilerError, Context, FunctionPrototype};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

type Owned = Arc<[u8]>;

struct Kept {
    source: Owned,
    compiled: Arc<CompiledPrototype<Owned>>,
}

fn kept() -> &'static Mutex<HashMap<String, Kept>> {
    static KEPT: OnceLock<Mutex<HashMap<String, Kept>>> = OnceLock::new();
    KEPT.get_or_init(Mutex::default)
}

/// Owned strings, each spelling made once per chunk.
#[derive(Default)]
struct Interner(HashMap<Owned, ()>);

impl StringInterner for Interner {
    type String = Owned;

    fn intern(&mut self, text: &[u8]) -> Owned {
        if let Some((known, ())) = self.0.get_key_value(text) {
            return Arc::clone(known);
        }
        let made: Owned = Arc::from(text);
        self.0.insert(Arc::clone(&made), ());
        made
    }
}

/// `Closure::load`, from the compiled chunk when this name was compiled
/// from these bytes before.
pub(crate) fn load<'gc>(
    ctx: Context<'gc>,
    name: &str,
    source: &[u8],
) -> Result<Closure<'gc>, CompilerError> {
    // The native tier registers what luna compiles itself, and a binary
    // chunk is undumped rather than compiled.
    if cfg!(feature = "jit") || luna::dump::is_binary_chunk(source) {
        return Closure::load(ctx, Some(name), source);
    }
    let compiled = compiled(name, source)?;
    let proto = FunctionPrototype::from_compiled_map_strings(
        &ctx,
        ctx.intern(name.as_bytes()),
        &compiled,
        ctx.debug_locals(),
        |text| ctx.intern(text),
    );
    Ok(Closure::new(&ctx, proto, Some(ctx.globals()))?)
}

fn compiled(name: &str, source: &[u8]) -> Result<Arc<CompiledPrototype<Owned>>, CompilerError> {
    let lock = || {
        kept()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    };
    if let Some(kept) = lock().get(name)
        && *kept.source == *source
    {
        return Ok(Arc::clone(&kept.compiled));
    }
    let mut interner = Interner::default();
    let chunk = compiler::parse_chunk(source, &mut interner)?;
    let compiled = Arc::new(compiler::compile_chunk(&chunk, &mut interner)?);
    lock().insert(
        name.to_owned(),
        Kept {
            source: Arc::from(source),
            compiled: Arc::clone(&compiled),
        },
    );
    Ok(compiled)
}
