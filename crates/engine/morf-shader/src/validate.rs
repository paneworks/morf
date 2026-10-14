use crate::diagnostics::Diagnostic;
use crate::ir::*;
use crate::types::*;
use crate::{ShaderKind, ShaderSpec};

/// Checks what lowering could not check locally.
///
/// Two things need the whole program in view: that every path returns, and that
/// no texture is sampled under non-uniform control flow.
pub(crate) fn check(program: &Program, spec: &ShaderSpec, diagnostics: &mut Vec<Diagnostic>) {
    let entry = program.entry_line;
    returned_types(
        &program.entry.body,
        program.entry.returns,
        entry,
        diagnostics,
    );
    if !returns(&program.entry.body) {
        diagnostics.push(
            Diagnostic::new(entry, "this shader can finish without returning a colour")
                .note("every branch needs its own `return`, or move one to the end"),
        );
    }
    if let Some(line) = program.samples_behind
        && spec.kind != ShaderKind::Effect
    {
        diagnostics.push(
            Diagnostic::new(line, "only an effect shader can read what is underneath").note(
                format!(
                    "this one is `{}`; declare `kind = \"effect\"`",
                    spec.kind.name()
                ),
            ),
        );
    }
    // WGSL forbids both sampling and derivatives under non-uniform control
    // flow — they read the neighbouring pixels, which have to have taken the
    // same path — and naga's own message for it is close to unreadable.
    // Catching it here costs a walk and buys a diagnostic that names the rule.
    //
    // One check for both, because it is one rule. A check per builtin is how
    // the two would drift.
    if program.samples_behind.is_some() || program.takes_derivative.is_some() {
        let mut offender = None;
        uniformity(&program.entry.body, false, entry, &mut offender);
        if let Some((name, line)) = offender {
            diagnostics.push(
                Diagnostic::new(
                    line,
                    format!("`{name}` cannot be called inside an `if` or a loop"),
                )
                .note(
                    "it reads the neighbouring pixels, which have to have taken the \
                     same path; compute it first, then branch on the result",
                ),
            );
        }
    }
}

/// Whether every path through a block ends in a `return`.
fn returns(block: &Block) -> bool {
    block.0.iter().any(|statement| match statement {
        Stmt::Return(_) => true,
        Stmt::If { arms, otherwise } => {
            // A branch only settles the question when it has an else: without
            // one, falling through is a path that reaches the end.
            otherwise.as_ref().is_some_and(returns) && arms.iter().all(|(_, body)| returns(body))
        }
        _ => false,
    })
}

fn uniformity(
    block: &Block,
    inside: bool,
    mut line: u32,
    offender: &mut Option<(&'static str, u32)>,
) {
    for statement in &block.0 {
        match statement {
            Stmt::Line(at) => line = *at,
            Stmt::If { arms, otherwise } => {
                for (condition, body) in arms {
                    if inside && let Some(name) = non_uniform(condition) {
                        *offender = Some((name, line));
                    }
                    uniformity(body, true, line, offender);
                }
                if let Some(body) = otherwise {
                    uniformity(body, true, line, offender);
                }
            }
            Stmt::Loop { body, .. } => uniformity(body, true, line, offender),
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Return(value) => {
                if inside && let Some(name) = non_uniform(value) {
                    *offender = Some((name, line));
                }
            }
            Stmt::Break | Stmt::Continue | Stmt::Discard => {}
        }
    }
}

/// The name of the first call in this expression that needs uniform control
/// flow, if there is one.
fn non_uniform(expression: &Expr) -> Option<&'static str> {
    match expression {
        Expr::Call { builtin, args, .. } => {
            let own = match builtin {
                Builtin::Texture => Some("texture"),
                Builtin::Dpdx
                | Builtin::DpdxCoarse
                | Builtin::DpdxFine
                | Builtin::Dpdy
                | Builtin::DpdyCoarse
                | Builtin::DpdyFine
                | Builtin::Fwidth
                | Builtin::FwidthCoarse
                | Builtin::FwidthFine => Some(builtin.wgsl()),
                _ => None,
            };
            own.or_else(|| args.iter().find_map(non_uniform))
        }
        Expr::Unary { value, .. } | Expr::Swizzle { value, .. } => non_uniform(value),
        Expr::Binary { left, right, .. } => non_uniform(left).or_else(|| non_uniform(right)),
        Expr::Construct { args, .. } | Expr::Array { elements: args, .. } => {
            args.iter().find_map(non_uniform)
        }
        Expr::Index { value, index, .. } => non_uniform(value).or_else(|| non_uniform(index)),
        Expr::Literal(_) | Expr::Local { .. } | Expr::Param { .. } | Expr::Input { .. } => None,
    }
}

/// Reports every `return` whose value is not a colour.
///
/// Checked over the finished program rather than at each return, so a shader
/// with three branches gets three messages instead of stopping at the first.
fn returned_types(block: &Block, wanted: Type, mut line: u32, diagnostics: &mut Vec<Diagnostic>) {
    for statement in &block.0 {
        match statement {
            Stmt::Line(at) => line = *at,
            Stmt::Return(value) => {
                let ty = value.ty();
                if ty != wanted && !ty.is_poison() {
                    // A fragment returns a colour and a vertex returns a
                    // position, so the message has to say which this is.
                    let diagnostic = if wanted == Type::Vec2 {
                        Diagnostic::new(
                            line,
                            format!("a vertex shader returns a vec2 position, not {ty}"),
                        )
                        .note("it moves a corner; the colour comes from the fragment shader")
                    } else {
                        Diagnostic::new(line, format!("a shader returns a vec4 colour, not {ty}"))
                            .note("widen it: `vec4(value, 1.0)`")
                    };
                    diagnostics.push(diagnostic);
                }
            }
            Stmt::If { arms, otherwise } => {
                for (_, body) in arms {
                    returned_types(body, wanted, line, diagnostics);
                }
                if let Some(body) = otherwise {
                    returned_types(body, wanted, line, diagnostics);
                }
            }
            Stmt::Loop { body, .. } => returned_types(body, wanted, line, diagnostics),
            Stmt::Let { .. }
            | Stmt::Assign { .. }
            | Stmt::Break
            | Stmt::Continue
            | Stmt::Discard => {}
        }
    }
}
