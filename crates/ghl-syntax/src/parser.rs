use chumsky::prelude::*;
use crate::lexer::{Token, lex};
use crate::ast::*;

pub fn type_parser() -> impl Parser<Token, TypeAnnotation, Error = Simple<Token>> + Clone {
    recursive(|ty| {
        let ident_str = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        };

        ident_str
            .then(
                ty.separated_by(just(Token::Comma))
                    .allow_trailing()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket))
                    .or_not(),
            )
            .map(|(name, args)| match args {
                Some(args) if !args.is_empty() => TypeAnnotation::Generic(name, args),
                _ => TypeAnnotation::Simple(name),
            })
    })
}

pub fn pattern_parser() -> impl Parser<Token, Pattern, Error = Simple<Token>> + Clone {
    select! {
        Token::Underscore => Pattern::Wildcard,
        Token::IntLit(n) => Pattern::Lit(Literal::Int(n)),
        Token::FloatLit(s) => Pattern::Lit(Literal::Float(s.parse::<f64>().unwrap_or(0.0))),
        Token::StringLit(s) => Pattern::Lit(Literal::String(s)),
        Token::True => Pattern::Lit(Literal::Bool(true)),
        Token::False => Pattern::Lit(Literal::Bool(false)),
        Token::NA => Pattern::NA,
        Token::NAReason(r) => Pattern::NAReason(r),
        Token::Ident(id) => Pattern::Ident(id),
    }
}

pub fn fn_param_parser() -> impl Parser<Token, FnParam, Error = Simple<Token>> + Clone {
    let ident_str = select! {
        Token::Ident(name) => name,
        Token::Col => "col".to_string(),
    };

    ident_str
        .then(just(Token::Colon).ignore_then(type_parser()).or_not())
        .map_with_span(|(name, ty), span| FnParam { name, ty, span })
}

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

        // DataFrame literal: dataframe { col_name: [expr, ...], ... }
        let df_entry = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        }
        .then_ignore(just(Token::Colon))
        .then(expr.clone());

        let dataframe_literal = just(Token::DataFrame)
            .ignore_then(
                df_entry
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with_span(|cols, span| Expr::new(ExprKind::DataFrameLit(cols), span));

        // Matrix literal: mat [ 1.0, 2.0 ; 3.0, 4.0 ]
        let mat_row = expr.clone().separated_by(just(Token::Comma)).allow_trailing();
        let matrix_literal = just(Token::Mat)
            .ignore_then(
                mat_row
                    .separated_by(just(Token::Semicolon))
                    .allow_trailing()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket)),
            )
            .map_with_span(|rows, span| Expr::new(ExprKind::MatrixLit { rows }, span));

        // Lambda: \x, y -> expr
        let lambda = just(Token::Backslash)
            .ignore_then(
                select! {
                    Token::Ident(name) => name,
                    Token::Col => "col".to_string(),
                }
                .separated_by(just(Token::Comma))
                .allow_trailing(),
            )
            .then_ignore(just(Token::Arrow))
            .then(expr.clone())
            .map_with_span(|(params, body), span| {
                Expr::new(
                    ExprKind::Lambda {
                        params,
                        body: Box::new(body),
                    },
                    span,
                )
            });

        // Block statement parser (statements terminated by ;)
        let block_stmt = {
            let let_stmt = just(Token::Let)
                .ignore_then(just(Token::Mut).or_not())
                .then(select! {
                    Token::Ident(name) => name,
                    Token::Col => "col".to_string(),
                })
                .then(just(Token::Colon).ignore_then(type_parser()).or_not())
                .then_ignore(just(Token::Eq))
                .then(expr.clone())
                .then_ignore(just(Token::Semicolon))
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

            let return_stmt = just(Token::Return)
                .ignore_then(expr.clone().or_not())
                .then_ignore(just(Token::Semicolon))
                .map_with_span(|e, span| Stmt::new(StmtKind::Return(e), span));

            let expr_stmt = expr
                .clone()
                .then_ignore(just(Token::Semicolon))
                .map_with_span(|e, span| Stmt::new(StmtKind::Expr(e), span));

            let_stmt.or(return_stmt).or(expr_stmt)
        };

        // Block: { stmt*; expr? }
        let block = block_stmt
            .repeated()
            .then(expr.clone().or_not())
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with_span(|(stmts, trailing_expr), span| {
                Expr::new(
                    ExprKind::Block {
                        stmts,
                        expr: trailing_expr.map(Box::new),
                    },
                    span,
                )
            })
            .boxed();

        // If expression: if cond block (else (block | expr))?
        let if_expr = just(Token::If)
            .map_with_span(|_, span| span)
            .then(expr.clone())
            .then(block.clone())
            .then(
                just(Token::Else)
                    .ignore_then(block.clone().or(expr.clone()))
                    .or_not(),
            )
            .map(|(((if_span, cond), then_branch), else_branch)| {
                let start = if_span.start;
                let end = else_branch
                    .as_ref()
                    .map(|e| e.span.end)
                    .unwrap_or(then_branch.span.end);
                Expr::new(
                    ExprKind::If {
                        cond: Box::new(cond),
                        then_branch: Box::new(then_branch),
                        else_branch: else_branch.map(Box::new),
                    },
                    start..end,
                )
            });

        // Match expression: match expr { arm, ... }
        let match_arm = pattern_parser()
            .then(just(Token::If).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::FatArrow))
            .then(expr.clone())
            .then_ignore(just(Token::Comma).or_not())
            .map_with_span(|((pattern, guard), body), span| MatchArm {
                pattern,
                guard,
                body,
                span,
            });

        let match_expr = just(Token::Match)
            .ignore_then(expr.clone())
            .then(
                match_arm
                    .repeated()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with_span(|(matched, arms), span| {
                Expr::new(
                    ExprKind::Match {
                        expr: Box::new(matched),
                        arms,
                    },
                    span,
                )
            });

        let atom = val
            .or(parenthesized)
            .or(vector_literal)
            .or(dataframe_literal)
            .or(matrix_literal)
            .or(lambda)
            .or(block)
            .or(if_expr)
            .or(match_expr)
            .boxed();

        // Call argument: either `name = expr` (named) or a plain positional `expr`.
        let named_arg = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        }
        .then_ignore(just(Token::Eq))
        .then(expr.clone())
        .map_with_span(|(name, value), span| {
            Expr::new(ExprKind::NamedArg { name, value: Box::new(value) }, span)
        });
        let call_arg = named_arg.or(expr.clone());

        // Function call: atom ( arg1, arg2 )
        let call = atom
            .then(
                call_arg
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

        // Unary operators (-, !)
        let op_unary = just(Token::Minus)
            .to(false)
            .or(just(Token::Bang).to(true))
            .repeated()
            .then(call)
            .foldr(|is_not, rhs| {
                let s = rhs.span.clone();
                if is_not {
                    Expr::new(ExprKind::UnaryNot(Box::new(rhs)), s)
                } else {
                    Expr::new(ExprKind::UnaryNeg(Box::new(rhs)), s)
                }
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

        // Logical AND: &&
        let logical_and = comparison
            .clone()
            .then(just(Token::AndAnd).to(BinaryOp::And).then(comparison).repeated())
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

        // Logical OR: ||
        let logical_or = logical_and
            .clone()
            .then(just(Token::OrOr).to(BinaryOp::Or).then(logical_and).repeated())
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
        let pipe = logical_or
            .clone()
            .then(just(Token::Pipe).ignore_then(logical_or).repeated())
            .foldl(|expr, target| {
                let span = expr.span.start..target.span.end;
                Expr::new(
                    ExprKind::Pipe {
                        expr: Box::new(expr),
                        target: Box::new(target),
                    },
                    span,
                )
            })
            .boxed();

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
            })
            .boxed();

        formula
    })
}

pub fn stmt_parser() -> impl Parser<Token, Stmt, Error = Simple<Token>> + Clone {
    let fn_stmt = just(Token::Fn)
        .ignore_then(select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        })
        .then(
            fn_param_parser()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(type_parser()).or_not())
        .then(expr_parser())
        .map_with_span(|(((name, params), ret_ty), body), span| {
            Stmt::new(
                StmtKind::Fn {
                    name,
                    params,
                    ret_ty,
                    body,
                },
                span,
            )
        });

    let let_stmt = just(Token::Let)
        .ignore_then(just(Token::Mut).or_not())
        .then(select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        })
        .then(just(Token::Colon).ignore_then(type_parser()).or_not())
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

    let return_stmt = just(Token::Return)
        .ignore_then(expr_parser().or_not())
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|e, span| Stmt::new(StmtKind::Return(e), span));

    let expr_stmt = expr_parser()
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|expr, span| Stmt::new(StmtKind::Expr(expr), span));

    fn_stmt.or(let_stmt).or(return_stmt).or(expr_stmt)
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

    #[test]
    fn test_parse_fn_declaration() {
        let code = r#"
            fn calculate_beta(x: Matrix[f64], y: Vector[f64]) -> Vector[f64] {
                let xt = x;
                xt \ y
            }
        "#;

        let program = parse(code).expect("Should parse function declaration");
        assert_eq!(program.statements.len(), 1);

        match &program.statements[0].kind {
            StmtKind::Fn {
                name,
                params,
                ret_ty,
                body,
            } => {
                assert_eq!(name, "calculate_beta");
                assert_eq!(params.len(), 2);
                assert_eq!(params[0].name, "x");
                assert_eq!(
                    params[0].ty,
                    Some(TypeAnnotation::Generic(
                        "Matrix".into(),
                        vec![TypeAnnotation::Simple("f64".into())]
                    ))
                );
                assert_eq!(
                    ret_ty,
                    &Some(TypeAnnotation::Generic(
                        "Vector".into(),
                        vec![TypeAnnotation::Simple("f64".into())]
                    ))
                );
                match &body.kind {
                    ExprKind::Block { stmts, expr } => {
                        assert_eq!(stmts.len(), 1);
                        assert!(expr.is_some());
                    }
                    _ => panic!("Expected block body"),
                }
            }
            _ => panic!("Expected fn statement"),
        }
    }

    #[test]
    fn test_parse_dataframe_literal() {
        let code = r#"
            let df = dataframe {
                age: [25, 30, NA],
                income: [50000.0, 75000.5, NA:NoResponse]
            };
        "#;

        let program = parse(code).expect("Should parse dataframe literal");
        assert_eq!(program.statements.len(), 1);

        match &program.statements[0].kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::DataFrameLit(cols) => {
                    assert_eq!(cols.len(), 2);
                    assert_eq!(cols[0].0, "age");
                    assert_eq!(cols[1].0, "income");
                }
                _ => panic!("Expected DataFrame literal"),
            },
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_matrix_literal() {
        let code = "let m = mat [ 1.0, 2.0 ; 3.0, 4.0 ];";
        let program = parse(code).expect("Should parse matrix literal");
        assert_eq!(program.statements.len(), 1);

        match &program.statements[0].kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::MatrixLit { rows } => {
                    assert_eq!(rows.len(), 2);
                    assert_eq!(rows[0].len(), 2);
                    assert_eq!(rows[1].len(), 2);
                }
                _ => panic!("Expected Matrix literal"),
            },
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_if_else_and_block() {
        let code = r#"
            let res = if x > 0 {
                let double = x * 2;
                double
            } else {
                0
            };
        "#;

        let program = parse(code).expect("Should parse if/else expression");
        assert_eq!(program.statements.len(), 1);

        match &program.statements[0].kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::If {
                    cond,
                    then_branch,
                    else_branch,
                } => {
                    assert!(matches!(cond.kind, ExprKind::Binary { .. }));
                    assert!(matches!(then_branch.kind, ExprKind::Block { .. }));
                    assert!(else_branch.is_some());
                }
                _ => panic!("Expected If expression"),
            },
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_match_with_na_reason() {
        let code = r#"
            let label = match val {
                NA => "generic missing",
                NA:SensorDropout => "sensor hardware failure",
                x if x > 100 => "outlier",
                _ => "normal"
            };
        "#;

        let program = parse(code).expect("Should parse match expression");
        assert_eq!(program.statements.len(), 1);

        match &program.statements[0].kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::Match { expr, arms } => {
                    assert_eq!(expr.kind, ExprKind::Ident("val".into()));
                    assert_eq!(arms.len(), 4);
                    assert_eq!(arms[0].pattern, Pattern::NA);
                    assert_eq!(arms[1].pattern, Pattern::NAReason("SensorDropout".into()));
                    assert_eq!(arms[2].pattern, Pattern::Ident("x".into()));
                    assert!(arms[2].guard.is_some());
                    assert_eq!(arms[3].pattern, Pattern::Wildcard);
                }
                _ => panic!("Expected Match expression"),
            },
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_lambda() {
        let code = r#"let double = \x -> x * 2;"#;
        let program = parse(code).expect("Should parse lambda expression");
        assert_eq!(program.statements.len(), 1);

        match &program.statements[0].kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::Lambda { params, body } => {
                    assert_eq!(params, &vec!["x".to_string()]);
                    assert!(matches!(body.kind, ExprKind::Binary { .. }));
                }
                _ => panic!("Expected Lambda expression"),
            },
            _ => panic!("Expected let statement"),
        }
    }
}
