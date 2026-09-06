use chumsky::prelude::*;
use crate::lexer::{Token, lex};
use crate::ast::*;

pub fn expr_parser() -> impl Parser<Token, Expr, Error = Simple<Token>> + Clone {
    recursive(|expr| {
        let val = select! {
            Token::IntLit(n) => ExprKind::Lit(Literal::Int(n)),
            Token::FloatLit(s) => ExprKind::Lit(Literal::Float(s.parse::<f64>().unwrap_or(0.0))),
            Token::StringLit(s) => ExprKind::Lit(Literal::String(s)),
            Token::True => ExprKind::Lit(Literal::Bool(true)),
            Token::False => ExprKind::Lit(Literal::Bool(false)),
            Token::NA => ExprKind::Lit(Literal::NA(None)),
            Token::NAReason(r) => ExprKind::Lit(Literal::NA(Some(r))),
            Token::Underscore => ExprKind::Placeholder,
            Token::Col => ExprKind::Ident("col".into()),
            Token::Ident(id) => ExprKind::Ident(id),
        }
        .map_with_span(Expr::new);

        let parenthesized = expr
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let vector_literal = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map_with_span(|items, span| Expr::new(ExprKind::VectorLit(items), span));

        let atom = val.or(parenthesized).or(vector_literal);

        // Function call: atom ( arg1, arg2 )
        let call = atom
            .then(
                expr.clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .delimited_by(just(Token::LParen), just(Token::RParen))
                    .repeated(),
            )
            .foldl(|callee, args| {
                let start = callee.span.start;
                let end = if let Some(last) = args.last() {
                    last.span.end + 1
                } else {
                    callee.span.end + 2
                };
                Expr::new(
                    ExprKind::Call {
                        callee: Box::new(callee),
                        args,
                    },
                    start..end,
                )
            });

        // Unary operators
        let op_unary = just(Token::Minus)
            .map_with_span(|_, span: std::ops::Range<usize>| span)
            .repeated()
            .then(call)
            .foldr(|_span, rhs| {
                let s = rhs.span.clone();
                Expr::new(ExprKind::UnaryNeg(Box::new(rhs)), s)
            });

        // Binary operators: Product (*, /, %, .*, ./, \)
        let op_mul = just(Token::Star)
            .to(BinaryOp::Mul)
            .or(just(Token::Slash).to(BinaryOp::Div))
            .or(just(Token::Percent).to(BinaryOp::Mod))
            .or(just(Token::DotStar).to(BinaryOp::DotMul))
            .or(just(Token::DotSlash).to(BinaryOp::DotDiv))
            .or(just(Token::Backslash).to(BinaryOp::MatSolve));

        let product = op_unary
            .clone()
            .then(op_mul.then(op_unary).repeated())
            .foldl(|lhs, (op, rhs)| {
                let span = lhs.span.start..rhs.span.end;
                Expr::new(
                    ExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    span,
                )
            });

        // Binary operators: Sum (+, -, .+, .-)
        let op_add = just(Token::Plus)
            .to(BinaryOp::Add)
            .or(just(Token::Minus).to(BinaryOp::Sub))
            .or(just(Token::DotPlus).to(BinaryOp::DotAdd))
            .or(just(Token::DotMinus).to(BinaryOp::DotSub));

        let sum = product
            .clone()
            .then(op_add.then(product).repeated())
            .foldl(|lhs, (op, rhs)| {
                let span = lhs.span.start..rhs.span.end;
                Expr::new(
                    ExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    span,
                )
            });

        // Comparison (<, <=, >, >=, ==, !=)
        let op_cmp = just(Token::EqEq)
            .to(BinaryOp::Eq)
            .or(just(Token::NotEq).to(BinaryOp::NotEq))
            .or(just(Token::Lt).to(BinaryOp::Lt))
            .or(just(Token::LtEq).to(BinaryOp::LtEq))
            .or(just(Token::Gt).to(BinaryOp::Gt))
            .or(just(Token::GtEq).to(BinaryOp::GtEq));

        let comparison = sum
            .clone()
            .then(op_cmp.then(sum).repeated())
            .foldl(|lhs, (op, rhs)| {
                let span = lhs.span.start..rhs.span.end;
                Expr::new(
                    ExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    span,
                )
            });

        // Pipe operator: |>
        let pipe = comparison
            .clone()
            .then(just(Token::Pipe).ignore_then(comparison).repeated())
            .foldl(|expr, target| {
                let span = expr.span.start..target.span.end;
                Expr::new(
                    ExprKind::Pipe {
                        expr: Box::new(expr),
                        target: Box::new(target),
                    },
                    span,
                )
            });

        // Modeling formula operator: ~
        let formula = pipe
            .clone()
            .then(just(Token::Tilde).ignore_then(pipe).repeated())
            .foldl(|lhs, rhs| {
                let span = lhs.span.start..rhs.span.end;
                Expr::new(
                    ExprKind::Formula {
                        response: Box::new(lhs),
                        terms: vec![rhs],
                    },
                    span,
                )
            });

        formula
    })
}

pub fn stmt_parser() -> impl Parser<Token, Stmt, Error = Simple<Token>> + Clone {
    let let_stmt = just(Token::Let)
        .ignore_then(just(Token::Mut).or_not())
        .then(select! { Token::Ident(name) => name })
        .then(just(Token::Colon).ignore_then(select! { Token::Ident(ty) => ty }).or_not())
        .then_ignore(just(Token::Eq))
        .then(expr_parser())
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|(((is_mut, name), ty), init), span| {
            Stmt::new(
                StmtKind::Let {
                    name,
                    is_mut: is_mut.is_some(),
                    ty,
                    init,
                },
                span,
            )
        });

    let expr_stmt = expr_parser()
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|expr, span| Stmt::new(StmtKind::Expr(expr), span));

    let_stmt.or(expr_stmt)
}

pub fn program_parser() -> impl Parser<Token, Program, Error = Simple<Token>> {
    stmt_parser()
        .repeated()
        .then_ignore(end())
        .map(|statements| Program { statements })
}

pub fn parse(source: &str) -> Result<Program, Vec<String>> {
    let tokens_raw = lex(source);
    let mut tokens = Vec::new();
    let mut lex_errors = Vec::new();

    for (res, span) in tokens_raw {
        match res {
            Ok(tok) => tokens.push((tok, span)),
            Err(()) => lex_errors.push(format!("Unrecognized token at {:?}", span)),
        }
    }

    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }

    let end_span = source.len()..source.len();
    let stream = chumsky::Stream::from_iter(end_span, tokens.into_iter());

    program_parser()
        .parse(stream)
        .map_err(|errs| errs.into_iter().map(|e| format!("{}", e)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_let_and_pipe() {
        let code = r#"
            let data = [1.0, 2.0, NA, NA:NotObservable];
            let summary = data |> mean();
        "#;

        let program = parse(code).expect("Should parse successfully");
        assert_eq!(program.statements.len(), 2);

        match &program.statements[0].kind {
            StmtKind::Let { name, init, .. } => {
                assert_eq!(name, "data");
                match &init.kind {
                    ExprKind::VectorLit(items) => {
                        assert_eq!(items.len(), 4);
                        assert_eq!(items[2].kind, ExprKind::Lit(Literal::NA(None)));
                        assert_eq!(
                            items[3].kind,
                            ExprKind::Lit(Literal::NA(Some("NotObservable".into())))
                        );
                    }
                    _ => panic!("Expected vector literal"),
                }
            }
            _ => panic!("Expected let statement"),
        }

        match &program.statements[1].kind {
            StmtKind::Let { name, init, .. } => {
                assert_eq!(name, "summary");
                match &init.kind {
                    ExprKind::Pipe { expr, target } => {
                        assert_eq!(expr.kind, ExprKind::Ident("data".into()));
                        match &target.kind {
                            ExprKind::Call { callee, .. } => {
                                assert_eq!(callee.kind, ExprKind::Ident("mean".into()));
                            }
                            _ => panic!("Expected call target"),
                        }
                    }
                    _ => panic!("Expected pipe expression"),
                }
            }
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_formula() {
        let code = "let spec = y ~ x1 + x2;";
        let program = parse(code).expect("Should parse formula");
        assert_eq!(program.statements.len(), 1);
    }
}
