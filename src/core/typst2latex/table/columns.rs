//! Column inference and the existing table allocation limit.
use typst_syntax::{ast, ast::AstNode};

pub(crate) const MAX_TABLE_COLUMNS: usize = 1000;

/// Infer track count without expanding repeated arrays.
pub(crate) fn infer_table_columns(value: &str) -> Option<usize> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.bytes().all(|b| b.is_ascii_digit()) {
        return Some(value.parse::<usize>().unwrap_or(usize::MAX).max(1));
    }
    let root = typst_syntax::parse_code(value);
    if root.erroneous() {
        return None;
    }
    let expr = root.children().find_map(|node| node.cast::<ast::Expr>())?;
    infer_column_expr(expr)
}

fn infer_column_expr(expr: ast::Expr<'_>) -> Option<usize> {
    #[derive(Clone, Copy)]
    enum Value {
        Integer(usize),
        Array(usize),
        Track,
        Number(f64),
        Null,
    }
    use ast::BinOp;
    use Value::*;

    fn scalar_track(expr: ast::Expr<'_>) -> Option<Value> {
        match expr {
            ast::Expr::Auto(_) => Some(Track),
            ast::Expr::Numeric(numeric) => match numeric.get().1 {
                ast::Unit::Deg | ast::Unit::Rad => None,
                _ => Some(Track),
            },
            _ => None,
        }
    }

    // Avoid recursive evaluation of nested expressions.
    let mut pending = vec![(expr, false)];
    let mut values: Vec<Option<Value>> = Vec::new();
    while let Some((expr, visited)) = pending.pop() {
        let result = match expr {
            ast::Expr::Parenthesized(group) => {
                pending.push((group.expr(), false));
                continue;
            }
            ast::Expr::Unary(unary) if unary.op() == ast::UnOp::Pos => {
                pending.push((unary.expr(), false));
                continue;
            }
            ast::Expr::Int(integer) if integer.get() >= 0 => Some(Integer(
                usize::try_from(integer.get()).unwrap_or(usize::MAX),
            )),
            ast::Expr::Float(number) => Some(Number(number.get())),
            ast::Expr::None(_) => Some(Null),
            ast::Expr::Auto(_) | ast::Expr::Numeric(_) => scalar_track(expr),
            ast::Expr::ContentBlock(block) => {
                // MiniEval serializes unevaluated track lengths as content.
                let text = block.body().to_untyped().clone().into_text();
                let parsed = typst_syntax::parse_code(&text);
                parsed
                    .children()
                    .find_map(|node| node.cast::<ast::Expr>())
                    .and_then(scalar_track)
            }
            ast::Expr::Array(array) if !visited => {
                pending.push((expr, true));
                for item in array.items() {
                    if let ast::ArrayItem::Spread(spread) = item {
                        pending.push((spread.expr(), false));
                    }
                }
                continue;
            }
            ast::Expr::Array(array) => {
                let mut count = 0usize;
                for item in array.items() {
                    match item {
                        ast::ArrayItem::Pos(_) => count = count.saturating_add(1),
                        ast::ArrayItem::Spread(_) => match values.pop().flatten() {
                            Some(Array(length)) => count = count.saturating_add(length),
                            Some(Null) => {}
                            _ => return None,
                        },
                    }
                }
                Some(Array(count))
            }
            ast::Expr::Binary(binary) if !visited => {
                pending.push((expr, true));
                pending.push((binary.rhs(), false));
                pending.push((binary.lhs(), false));
                continue;
            }
            ast::Expr::Binary(binary) => {
                let rhs = values.pop().flatten();
                let lhs = values.pop().flatten();
                match (binary.op(), lhs, rhs) {
                    (BinOp::Mul, Some(Array(n)), Some(Integer(times)))
                    | (BinOp::Mul, Some(Integer(times)), Some(Array(n))) => {
                        Some(Array(n.saturating_mul(times)))
                    }
                    (BinOp::Add, Some(Array(a)), Some(Array(b))) => {
                        Some(Array(a.saturating_add(b)))
                    }
                    (BinOp::Mul, Some(Integer(a)), Some(Integer(b))) => {
                        Some(Integer(a.saturating_mul(b)))
                    }
                    (BinOp::Add, Some(Integer(a)), Some(Integer(b))) => {
                        Some(Integer(a.saturating_add(b)))
                    }
                    (BinOp::Sub, Some(Integer(a)), Some(Integer(b))) => {
                        a.checked_sub(b).map(Integer)
                    }
                    (BinOp::Add | BinOp::Sub, Some(Track), Some(Track))
                    | (BinOp::Mul, Some(Track), Some(Integer(_) | Number(_)))
                    | (BinOp::Mul, Some(Integer(_) | Number(_)), Some(Track)) => Some(Track),
                    (BinOp::Div, Some(Track), Some(Integer(n))) if n > 0 => Some(Track),
                    (BinOp::Div, Some(Track), Some(Number(n))) if n > 0.0 => Some(Track),
                    _ => None,
                }
            }
            _ => None,
        };
        values.push(result);
    }
    match values.pop().flatten()? {
        Integer(count) | Array(count) => Some(count.max(1)),
        Track => Some(1),
        Number(_) | Null => None,
    }
}

/// Detect static overflow before MiniEval expands table arguments.
pub(crate) fn exceeds_column_limit(args: ast::Args<'_>) -> bool {
    args.items().any(|arg| match arg {
        ast::Arg::Named(named) if named.name().get() == "columns" => {
            let expr = named.expr();
            let columns = if matches!(expr, ast::Expr::Int(_) | ast::Expr::Float(_))
                || expr.to_untyped().erroneous()
            {
                // Keep numeric-literal overflow handling on the text path.
                let value = expr.to_untyped().clone().into_text();
                infer_table_columns(&value)
            } else {
                infer_column_expr(expr)
            };
            columns.is_some_and(|n| n > MAX_TABLE_COLUMNS)
        }
        _ => false,
    })
}
