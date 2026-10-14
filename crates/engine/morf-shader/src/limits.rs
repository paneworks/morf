//! What a shader is not allowed to exceed.
//!
//! A hung shader loses the wgpu device, and losing the device kills the
//! compositor — the bar, the lock screen, the session. There is no "restart the
//! app". Every number here exists because the blast radius of getting it wrong
//! is the user's whole desktop, so each is enforced as a diagnostic naming the
//! cap and the measured value, never as a panic and never silently.

/// How many times any one loop may run before it is cut off.
///
/// Emitted as a counter the shader cannot reach around, because the author
/// never writes the loop — they write a `while`, and the compiler decides what
/// it becomes.
pub const MAX_ITERATIONS: u32 = 4096;

/// How deeply loops may nest.
pub const MAX_LOOP_NESTING: u32 = 4;

/// How many iterations a nest of loops may run per invocation, all levels
/// multiplied together.
///
/// The per-loop guard alone does not bound the work: four nested loops at
/// [`MAX_ITERATIONS`] come to about 2.8e14 turns, which is a GPU timeout and a
/// lost device. Every emitted loop also counts into one per-invocation budget
/// and stops when it runs out; nests whose constant trip counts already
/// multiply past it are refused at compile time.
pub const MAX_LOOP_WORK: u64 = 65_536;

/// How many operations one shader may hold.
///
/// Shader compilation is superlinear in program size, so an enormous shader
/// does not fail — it stalls the session while the driver thinks.
pub const MAX_IR_NODES: u32 = 100_000;

/// How long a shader's source may be.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;

/// How many parameters one shader may declare.
pub const MAX_PARAMS: usize = 32;
