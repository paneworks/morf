//! The compile-time half of the loop work cap.
//!
//! Every loop is bounded at run time by the shared per-invocation budget the
//! emitter threads through it. This walk refuses nests whose constant trip
//! counts already multiply past that budget, since they could only ever run
//! cut short. It runs over the finished program rather than during lowering,
//! because a helper is lowered once and may then be called from inside any
//! number of loops: only the call graph knows how deep its loops really sit.

use crate::diagnostics::Diagnostic;
use crate::ir::*;
use crate::limits::MAX_LOOP_WORK;

pub(crate) fn check(program: &Program, diagnostics: &mut Vec<Diagnostic>) {
    let mut walk = Walk {
        helpers: &program.helpers,
        diagnostics,
        reported: false,
    };
    walk.block(&program.entry.body, 1, 1);
}

struct Walk<'p, 'd> {
    helpers: &'p [crate::lower::Instance],
    diagnostics: &'d mut Vec<Diagnostic>,
    reported: bool,
}

impl Walk<'_, '_> {
    fn block(&mut self, block: &Block, work: u64, mut line: u32) {
        for statement in &block.0 {
            match statement {
                Stmt::Line(at) => line = *at,
                Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Return(value) => {
                    self.expression(value, work);
                }
                Stmt::If { arms, otherwise } => {
                    for (condition, body) in arms {
                        self.expression(condition, work);
                        self.block(body, work, line);
                    }
                    if let Some(body) = otherwise {
                        self.block(body, work, line);
                    }
                }
                Stmt::Loop {
                    guard,
                    counted,
                    body,
                    continuing,
                } => {
                    let turns = if *counted { u64::from(*guard) } else { 1 };
                    let inner = work.saturating_mul(turns);
                    if inner > MAX_LOOP_WORK {
                        self.report(line, inner);
                        continue;
                    }
                    self.block(body, inner, line);
                    self.block(continuing, inner, line);
                }
                Stmt::Break | Stmt::Continue | Stmt::Discard => {}
            }
        }
    }

    /// Follows every helper call into the helper's body at the caller's work.
    fn expression(&mut self, expression: &Expr, work: u64) {
        match expression {
            Expr::Call { builtin, args, .. } => {
                if *builtin == Builtin::Helper
                    && let Some(Expr::Local { name, .. }) = args.first()
                    && let Some(helper) = self.helpers.iter().find(|h| h.name == *name)
                {
                    self.block(&helper.body, work, 1);
                }
                for arg in args {
                    self.expression(arg, work);
                }
            }
            Expr::Unary { value, .. } | Expr::Swizzle { value, .. } => {
                self.expression(value, work);
            }
            Expr::Binary { left, right, .. } => {
                self.expression(left, work);
                self.expression(right, work);
            }
            Expr::Construct { args, .. } | Expr::Array { elements: args, .. } => {
                for arg in args {
                    self.expression(arg, work);
                }
            }
            Expr::Index { value, index, .. } => {
                self.expression(value, work);
                self.expression(index, work);
            }
            Expr::Literal(_) | Expr::Local { .. } | Expr::Param { .. } | Expr::Input { .. } => {}
        }
    }

    fn report(&mut self, line: u32, work: u64) {
        if self.reported {
            return;
        }
        self.reported = true;
        self.diagnostics.push(
            Diagnostic::new(
                line,
                format!(
                    "these nested loops run {work} times per pixel, over the {MAX_LOOP_WORK} limit"
                ),
            )
            .note("lower the loop counts or nest fewer loops"),
        );
    }
}
