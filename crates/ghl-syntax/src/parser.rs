use chumsky::prelude::*;
use crate::lexer::{Token, lex};
use crate::ast::*;

pub fn type_parser() -> impl Parser<Token, TypeAnnotation, Error = Simple<Token>> + Clone {
    recursive(|ty| {
        let single_name = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
            Token::IntLit(n) => n.to_string(),
            Token::Underscore => "_".to_string(),
            Token::SelfType => "Self".to_string(),
        };

        let path_name = single_name
            .then(just(Token::PathSep).ignore_then(single_name).repeated())
            .map(|(first, rest)| {
                if rest.is_empty() {
                    first
                } else {
                    let mut s = first;
                    for r in rest {
                        s.push_str("::");
                        s.push_str(&r);
                    }
                    s
                }
            });

        let ref_prefix = just(Token::Amp)
            .ignore_then(just(Token::Mut).or_not())
            .map(|is_mut| if is_mut.is_some() { "&mut " } else { "&" })
            .or_not();

        ref_prefix
            .then(path_name)
            .then(
                ty.separated_by(just(Token::Comma))
                    .allow_trailing()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket))
                    .or_not(),
            )
            .map(|((prefix, name), args)| {
                let full_name = match prefix {
                    Some(p) => format!("{}{}", p, name),
                    None => name,
                };
                match args {
                    Some(args) if !args.is_empty() => TypeAnnotation::Generic(full_name, args),
                    _ => TypeAnnotation::Simple(full_name),
                }
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
    let self_tok = select! {
        Token::SelfValue => "self".to_string(),
        Token::Ident(name) if name == "self" => name,
    };

    let self_param = just(Token::Amp)
        .ignore_then(just(Token::Mut).or_not())
        .then(self_tok.clone())
        .map_with_span(|(is_mut, _name), span| {
            let ref_str = if is_mut.is_some() { "&mut self" } else { "&self" };
            FnParam {
                name: ref_str.to_string(),
                ty: Some(TypeAnnotation::Simple("Self".to_string())),
                span,
            }
        })
        .or(
            self_tok
                .map_with_span(|name, span| FnParam {
                    name,
                    ty: Some(TypeAnnotation::Simple("Self".to_string())),
                    span,
                })
        );

    let ident_str = select! {
        Token::Ident(name) => name,
        Token::Col => "col".to_string(),
    };

    let regular_param = ident_str
        .then(just(Token::Colon).ignore_then(type_parser()).or_not())
        .map_with_span(|(name, ty), span| FnParam { name, ty, span });

    self_param.or(regular_param)
}

pub fn expr_parser() -> impl Parser<Token, Expr, Error = Simple<Token>> + Clone {
    recursive(|expr| {
        let path_segment = select! {
            Token::Ident(id) => id,
            Token::Col => "col".to_string(),
            Token::DataFrame => "dataframe".to_string(),
            Token::Mat => "mat".to_string(),
        };

        let multi_segment_path = path_segment
            .then(just(Token::PathSep).ignore_then(path_segment).repeated().at_least(1))
            .map_with_span(|(first, rest), span| {
                let mut segments = vec![first];
                segments.extend(rest);
                Expr::new(ExprKind::Path(segments), span)
            });

        let single_ident = select! {
            Token::Ident(id) => ExprKind::Ident(id),
            Token::Col => ExprKind::Ident("col".into()),
            Token::SelfValue => ExprKind::Ident("self".into()),
        }
        .map_with_span(Expr::new);

        let ident_or_path = multi_segment_path.or(single_ident);

        let lit_val = select! {
            Token::IntLit(n) => ExprKind::Lit(Literal::Int(n)),
            Token::FloatLit(s) => ExprKind::Lit(Literal::Float(s.parse::<f64>().unwrap_or(0.0))),
            Token::StringLit(s) => ExprKind::Lit(Literal::String(s)),
            Token::True => ExprKind::Lit(Literal::Bool(true)),
            Token::False => ExprKind::Lit(Literal::Bool(false)),
            Token::NA => ExprKind::Lit(Literal::NA(None)),
            Token::NAReason(r) => ExprKind::Lit(Literal::NA(Some(r))),
            Token::Underscore => ExprKind::Placeholder,
        }
        .map_with_span(Expr::new);

        let struct_entry = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        }
        .then_ignore(just(Token::Colon))
        .then(expr.clone());

        let struct_literal = select! {
            Token::Ident(name) => name,
        }
        .then(
            struct_entry
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with_span(|(name, fields), span| Expr::new(ExprKind::StructLit { name, fields }, span));

        let val = lit_val.or(struct_literal).or(ident_or_path);

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

        // Record literal: { field_1: expr, field_2: expr, ... } (RFC 01 §3.4)
        let record_entry = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        }
        .then_ignore(just(Token::Colon))
        .then(expr.clone());

        let record_literal = record_entry
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with_span(|fields, span| Expr::new(ExprKind::RecordLit(fields), span));

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

        // Lambda: \x, y -> expr OR |x, y| expr OR || expr
        let lambda_params = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        }
        .separated_by(just(Token::Comma))
        .allow_trailing();

        let slash_lambda = just(Token::Backslash)
            .ignore_then(lambda_params.clone())
            .then_ignore(just(Token::Arrow))
            .then(expr.clone());

        let pipe_lambda = just(Token::VBar)
            .ignore_then(lambda_params)
            .then_ignore(just(Token::VBar))
            .then(expr.clone());

        let pipe_lambda_empty = just(Token::OrOr)
            .ignore_then(expr.clone())
            .map(|body| (Vec::new(), body));

        let lambda = slash_lambda
            .or(pipe_lambda)
            .or(pipe_lambda_empty)
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

            // `name = value;` -- reassignment, distinct from `let name = value;`. Must
            // be tried before `expr_stmt` below: a bare identifier also parses as a
            // valid (if pointless as a statement) expression, so without this ordering
            // `expr_stmt` would consume the identifier and then choke on the `=`.
            let assign_stmt = select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Eq))
                .then(expr.clone())
                .then_ignore(just(Token::Semicolon))
                .map_with_span(|(name, value), span| Stmt::new(StmtKind::Assign { name, value }, span));

            let expr_stmt = expr
                .clone()
                .then_ignore(just(Token::Semicolon))
                .map_with_span(|e, span| Stmt::new(StmtKind::Expr(e), span));

            let use_stmt = use_stmt_parser();

            use_stmt.or(let_stmt).or(return_stmt).or(assign_stmt).or(expr_stmt)
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

        // While expression: while cond block
        let while_expr = just(Token::While)
            .map_with_span(|_, span| span)
            .then(expr.clone())
            .then(block.clone())
            .map(|((while_span, cond), body)| {
                let start = while_span.start;
                let end = body.span.end;
                Expr::new(
                    ExprKind::While {
                        cond: Box::new(cond),
                        body: Box::new(body),
                    },
                    start..end,
                )
            });

        // For-range expression: for ident in expr { block }
        let for_expr = just(Token::For)
            .map_with_span(|_, span| span)
            .then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(block.clone())
            .map(|(((for_span, var), iter_expr), body)| {
                let start_idx = for_span.start;
                let end_idx = body.span.end;
                match iter_expr.kind {
                    ExprKind::Range { start, end, inclusive: _ } => {
                        Expr::new(
                            ExprKind::For {
                                var,
                                start,
                                end,
                                body: Box::new(body),
                            },
                            start_idx..end_idx,
                        )
                    }
                    _ => {
                        Expr::new(
                            ExprKind::For {
                                var,
                                start: Box::new(Expr::new(ExprKind::Lit(Literal::Int(0)), 0..0)),
                                end: Box::new(iter_expr),
                                body: Box::new(body),
                            },
                            start_idx..end_idx,
                        )
                    }
                }
            });

        let atom = val
            .or(parenthesized)
            .or(vector_literal)
            .or(dataframe_literal)
            .or(matrix_literal)
            .or(lambda)
            .or(record_literal)
            .or(block)
            .or(if_expr)
            .or(match_expr)
            .or(while_expr)
            .or(for_expr)
            .boxed();

        // Call argument: either `name = expr` / `name: expr` (named) or a plain positional `expr`.
        let named_arg = select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        }
        .then_ignore(just(Token::Eq).or(just(Token::Colon)))
        .then(expr.clone())
        .map_with_span(|(name, value), span| {
            Expr::new(ExprKind::NamedArg { name, value: Box::new(value) }, span)
        });
        let call_arg = named_arg.or(expr.clone());

        #[derive(Clone)]
        enum Postfix {
            Call(Vec<Expr>, Span),
            Field(String, Span),
            Index(Vec<IndexSpec>, Span),
        }

        let postfix_call = call_arg
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with_span(Postfix::Call);

        let postfix_field = just(Token::Dot)
            .ignore_then(select! {
                Token::Ident(name) => name,
                Token::Col => "col".to_string(),
            })
            .map_with_span(Postfix::Field);

        let colon_wildcard = just(Token::Colon).to(IndexSpec::All);
        let range_starts_with_dotdot_eq = just(Token::DotDotEq)
            .ignore_then(expr.clone())
            .map(|e| IndexSpec::Range {
                start: None,
                end: Some(Box::new(e)),
                inclusive: true,
            });
        let range_starts_with_dotdot = just(Token::DotDot)
            .ignore_then(expr.clone().or_not())
            .map(|opt_e| match opt_e {
                Some(e) => IndexSpec::Range {
                    start: None,
                    end: Some(Box::new(e)),
                    inclusive: false,
                },
                None => IndexSpec::All,
            });
        let index_from_expr = expr.clone()
            .then(
                just(Token::DotDotEq).ignore_then(expr.clone()).map(|e| Some((true, Some(e))))
                    .or(just(Token::DotDot).ignore_then(expr.clone().or_not()).map(|opt_e| Some((false, opt_e))))
                    .or(empty().to(None))
            )
            .map(|(first, opt_range)| match opt_range {
                None => match first.kind {
                    ExprKind::Range { start, end, inclusive } => IndexSpec::Range {
                        start: Some(start),
                        end: Some(end),
                        inclusive,
                    },
                    _ => IndexSpec::Expr(first),
                },
                Some((inclusive, end)) => IndexSpec::Range {
                    start: Some(Box::new(first)),
                    end: end.map(Box::new),
                    inclusive,
                },
            });

        let index_spec = colon_wildcard
            .or(range_starts_with_dotdot_eq)
            .or(range_starts_with_dotdot)
            .or(index_from_expr);

        let postfix_index = index_spec
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .at_least(1)
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map_with_span(Postfix::Index);

        let postfix = postfix_call.or(postfix_field).or(postfix_index);

        // Postfix operations: calls `(args...)`, field accesses `.field`, and bracket indexing `[indices...]`
        let call = atom
            .then(postfix.repeated())
            .foldl(|target, post| match post {
                Postfix::Call(args, span) => {
                    let start = target.span.start;
                    let end = span.end;
                    Expr::new(
                        ExprKind::Call {
                            callee: Box::new(target),
                            args,
                        },
                        start..end,
                    )
                }
                Postfix::Field(field, span) => {
                    let start = target.span.start;
                    let end = span.end;
                    Expr::new(
                        ExprKind::FieldAccess {
                            target: Box::new(target),
                            field,
                        },
                        start..end,
                    )
                }
                Postfix::Index(indices, span) => {
                    let start = target.span.start;
                    let end = span.end;
                    Expr::new(
                        ExprKind::Index {
                            target: Box::new(target),
                            indices,
                        },
                        start..end,
                    )
                }
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

        // Range: start..end or start..=end
        let range_expr = comparison
            .clone()
            .then(
                just(Token::DotDotEq).to(true)
                    .or(just(Token::DotDot).to(false))
                    .then(comparison.clone())
                    .or_not()
            )
            .map(|(start, opt_end)| {
                match opt_end {
                    Some((inclusive, end)) => {
                        let span = start.span.start..end.span.end;
                        Expr::new(
                            ExprKind::Range {
                                start: Box::new(start),
                                end: Box::new(end),
                                inclusive,
                            },
                            span,
                        )
                    }
                    None => start,
                }
            });

        // Logical AND: &&
        let logical_and = range_expr
            .clone()
            .then(just(Token::AndAnd).to(BinaryOp::And).then(range_expr).repeated())
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

pub fn use_stmt_parser() -> impl Parser<Token, Stmt, Error = Simple<Token>> + Clone {
    let ident_name = select! {
        Token::Ident(name) => name,
        Token::Col => "col".to_string(),
        Token::DataFrame => "dataframe".to_string(),
        Token::Mat => "mat".to_string(),
    };

    let use_item = ident_name
        .then(just(Token::As).ignore_then(ident_name).or_not())
        .map(|(name, alias)| UseItem { name, alias });

    let group_or_glob = just(Token::Star)
        .map(|_| UseKind::Glob)
        .or(
            use_item
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .map(UseKind::Items),
        );

    // Form 1: use a::b::{c, d} or use a::b::*
    let complex_use = ident_name
        .separated_by(just(Token::PathSep))
        .at_least(1)
        .then_ignore(just(Token::PathSep))
        .then(group_or_glob);

    // Form 2: use a::b::c (as alias)?
    let simple_use = ident_name
        .separated_by(just(Token::PathSep))
        .at_least(1)
        .then(just(Token::As).ignore_then(ident_name).or_not())
        .map(|(mut segments, alias)| {
            let item_name = segments.pop().expect("at_least(1)");
            (segments, UseKind::Items(vec![UseItem { name: item_name, alias }]))
        });

    just(Token::Use)
        .ignore_then(complex_use.or(simple_use))
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|(path, kind), span| {
            Stmt::new(StmtKind::Use(UseStmt { path, kind }), span)
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

    // See the same rule in `expr_parser()`'s block-statement grammar for why this must
    // be tried before `expr_stmt`.
    let assign_stmt = select! { Token::Ident(name) => name }
        .then_ignore(just(Token::Eq))
        .then(expr_parser())
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|(name, value), span| Stmt::new(StmtKind::Assign { name, value }, span));

    let expr_stmt = expr_parser()
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|expr, span| Stmt::new(StmtKind::Expr(expr), span));

    let struct_field = select! {
        Token::Ident(name) => name,
        Token::Col => "col".to_string(),
    }
    .then_ignore(just(Token::Colon))
    .then(type_parser())
    .map_with_span(|(name, ty), span| StructField { name, ty, span });

    let struct_stmt = just(Token::Struct)
        .ignore_then(select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        })
        .then(
            struct_field
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|(name, fields), span| {
            Stmt::new(StmtKind::Struct(StructDecl { name, fields }), span)
        });

    let trait_assoc_type = just(Token::Type)
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Semicolon).or_not())
        .map(TraitItem::AssociatedType);

    let trait_method = just(Token::Fn)
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
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|((name, params), ret_ty), span| {
            TraitItem::Method(TraitMethodSig {
                name,
                params,
                ret_ty,
                span,
            })
        });

    let trait_stmt = just(Token::Trait)
        .ignore_then(select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        })
        .then(
            trait_assoc_type
                .or(trait_method)
                .repeated()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|(name, items), span| {
            Stmt::new(StmtKind::Trait(TraitDecl { name, items }), span)
        });

    let impl_assoc_type = just(Token::Type)
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Eq))
        .then(type_parser())
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|(name, ty), span| ImplItem::AssociatedType { name, ty, span });

    let impl_method = just(Token::Fn)
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
            ImplItem::Method {
                name,
                params,
                ret_ty,
                body,
                span,
            }
        });

    let impl_stmt = just(Token::Impl)
        .ignore_then(select! {
            Token::Ident(name) => name,
            Token::Col => "col".to_string(),
        })
        .then(
            just(Token::For)
                .ignore_then(select! {
                    Token::Ident(name) => name,
                    Token::Col => "col".to_string(),
                })
                .or_not(),
        )
        .then(
            impl_assoc_type
                .or(impl_method)
                .repeated()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .then_ignore(just(Token::Semicolon).or_not())
        .map_with_span(|((first, target), items), span| {
            let (trait_name, target_type) = match target {
                Some(tgt) => (Some(first), tgt),
                None => (None, first),
            };
            Stmt::new(
                StmtKind::Impl(ImplDecl {
                    trait_name,
                    target_type,
                    items,
                }),
                span,
            )
        });

    let use_stmt = use_stmt_parser();

    use_stmt
        .or(struct_stmt)
        .or(trait_stmt)
        .or(impl_stmt)
        .or(fn_stmt)
        .or(let_stmt)
        .or(return_stmt)
        .or(assign_stmt)
        .or(expr_stmt)
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

    #[test]
    fn test_parse_pipe_lambda_and_arena_scope() {
        let code = r#"
            let res = arena::scope(|arena| {
                let v = arena.alloc_vector(10, 0.0);
                v
            });
            let no_args = || 42;
        "#;
        let program = parse(code).expect("Should parse pipe lambda and arena::scope");
        assert_eq!(program.statements.len(), 2);

        match &program.statements[0].kind {
            StmtKind::Let { name, init, .. } => {
                assert_eq!(name, "res");
                match &init.kind {
                    ExprKind::Call { callee, args } => {
                        assert!(matches!(&callee.kind, ExprKind::Path(p) if p == &vec!["arena", "scope"]));
                        assert_eq!(args.len(), 1);
                        match &args[0].kind {
                            ExprKind::Lambda { params, body } => {
                                assert_eq!(params, &vec!["arena".to_string()]);
                                assert!(matches!(body.kind, ExprKind::Block { .. }));
                            }
                            _ => panic!("Expected Lambda arg to arena::scope"),
                        }
                    }
                    _ => panic!("Expected Call expression"),
                }
            }
            _ => panic!("Expected let statement"),
        }

        match &program.statements[1].kind {
            StmtKind::Let { name, init, .. } => {
                assert_eq!(name, "no_args");
                match &init.kind {
                    ExprKind::Lambda { params, body } => {
                        assert!(params.is_empty());
                        assert!(matches!(body.kind, ExprKind::Lit(Literal::Int(42))));
                    }
                    _ => panic!("Expected empty Lambda expression"),
                }
            }
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_use_statements() {
        let code = r#"
            use std::dataframe::read_parquet;
            use std::stats::distributions::{random_normal, normal_pdf};
            use std::linalg::*;
            use std::linalg::transpose as t;
            let y = std::math::sqrt(16.0);
        "#;
        let program = parse(code).expect("Should parse use statements and paths");
        assert_eq!(program.statements.len(), 5);

        // 1. Single item
        match &program.statements[0].kind {
            StmtKind::Use(use_stmt) => {
                assert_eq!(use_stmt.path, vec!["std", "dataframe"]);
                match &use_stmt.kind {
                    UseKind::Items(items) => {
                        assert_eq!(items.len(), 1);
                        assert_eq!(items[0].name, "read_parquet");
                        assert_eq!(items[0].alias, None);
                    }
                    _ => panic!("Expected items"),
                }
            }
            _ => panic!("Expected use statement"),
        }

        // 2. Grouped items
        match &program.statements[1].kind {
            StmtKind::Use(use_stmt) => {
                assert_eq!(use_stmt.path, vec!["std", "stats", "distributions"]);
                match &use_stmt.kind {
                    UseKind::Items(items) => {
                        assert_eq!(items.len(), 2);
                        assert_eq!(items[0].name, "random_normal");
                        assert_eq!(items[1].name, "normal_pdf");
                    }
                    _ => panic!("Expected items"),
                }
            }
            _ => panic!("Expected use statement"),
        }

        // 3. Glob
        match &program.statements[2].kind {
            StmtKind::Use(use_stmt) => {
                assert_eq!(use_stmt.path, vec!["std", "linalg"]);
                assert_eq!(use_stmt.kind, UseKind::Glob);
            }
            _ => panic!("Expected use statement"),
        }

        // 4. Alias
        match &program.statements[3].kind {
            StmtKind::Use(use_stmt) => {
                assert_eq!(use_stmt.path, vec!["std", "linalg"]);
                match &use_stmt.kind {
                    UseKind::Items(items) => {
                        assert_eq!(items.len(), 1);
                        assert_eq!(items[0].name, "transpose");
                        assert_eq!(items[0].alias, Some("t".to_string()));
                    }
                    _ => panic!("Expected items"),
                }
            }
            _ => panic!("Expected use statement"),
        }

        // 5. Qualified path in expression
        match &program.statements[4].kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::Call { callee, args } => {
                    assert_eq!(
                        callee.kind,
                        ExprKind::Path(vec!["std".into(), "math".into(), "sqrt".into()])
                    );
                    assert_eq!(args.len(), 1);
                }
                _ => panic!("Expected Call expression"),
            },
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_record_literal_and_field_access() {
        let code = r#"
            let record = { sample_id: "SMP-001", replicates: 4, p_value: 0.0042 };
            let s_id = record.sample_id;
            let res = compute(record.p_value);
        "#;
        let program = parse(code).expect("Should parse record literal and field access");
        assert_eq!(program.statements.len(), 3);

        match &program.statements[0].kind {
            StmtKind::Let { name, init, .. } => {
                assert_eq!(name, "record");
                match &init.kind {
                    ExprKind::RecordLit(fields) => {
                        assert_eq!(fields.len(), 3);
                        assert_eq!(fields[0].0, "sample_id");
                        assert_eq!(fields[1].0, "replicates");
                        assert_eq!(fields[2].0, "p_value");
                    }
                    _ => panic!("Expected RecordLit, got {:?}", init.kind),
                }
            }
            _ => panic!("Expected let statement"),
        }

        match &program.statements[1].kind {
            StmtKind::Let { name, init, .. } => {
                assert_eq!(name, "s_id");
                match &init.kind {
                    ExprKind::FieldAccess { target, field } => {
                        assert_eq!(field, "sample_id");
                        assert_eq!(target.kind, ExprKind::Ident("record".into()));
                    }
                    _ => panic!("Expected FieldAccess, got {:?}", init.kind),
                }
            }
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_bracket_indexing_and_slicing() {
        let p_a = parse("let a = v[0];").expect("v[0] ok");
        if let StmtKind::Let { init, .. } = &p_a.statements[0].kind {
            match &init.kind {
                ExprKind::Index { target, indices } => {
                    assert_eq!(target.kind, ExprKind::Ident("v".into()));
                    assert_eq!(indices.len(), 1);
                    assert!(matches!(&indices[0], IndexSpec::Expr(_)));
                }
                _ => panic!("expected Index"),
            }
        }

        let p_b = parse("let b = v[0..5];").expect("v[0..5] ok");
        if let StmtKind::Let { init, .. } = &p_b.statements[0].kind {
            match &init.kind {
                ExprKind::Index { indices, .. } => {
                    assert_eq!(indices.len(), 1);
                    match &indices[0] {
                        IndexSpec::Range { start, end, inclusive } => {
                            assert!(!inclusive);
                            assert!(start.is_some());
                            assert!(end.is_some());
                        }
                        _ => panic!("expected half-open Range"),
                    }
                }
                _ => panic!("expected Index"),
            }
        }

        let p_c = parse("let c = v[0..=5];").expect("v[0..=5] ok");
        if let StmtKind::Let { init, .. } = &p_c.statements[0].kind {
            match &init.kind {
                ExprKind::Index { indices, .. } => {
                    assert_eq!(indices.len(), 1);
                    match &indices[0] {
                        IndexSpec::Range { start, end, inclusive } => {
                            assert!(*inclusive);
                            assert!(start.is_some());
                            assert!(end.is_some());
                        }
                        _ => panic!("expected inclusive Range"),
                    }
                }
                _ => panic!("expected Index"),
            }
        }

        let p_d = parse("let d = v[..];").expect("v[..] ok");
        if let StmtKind::Let { init, .. } = &p_d.statements[0].kind {
            match &init.kind {
                ExprKind::Index { indices, .. } => {
                    assert_eq!(indices.len(), 1);
                    assert_eq!(indices[0], IndexSpec::All);
                }
                _ => panic!("expected Index"),
            }
        }

        let p_e = parse("let e = m[0..2, :];").expect("m[0..2, :] ok");
        if let StmtKind::Let { init, .. } = &p_e.statements[0].kind {
            match &init.kind {
                ExprKind::Index { indices, .. } => {
                    assert_eq!(indices.len(), 2);
                    assert!(matches!(&indices[0], IndexSpec::Range { inclusive: false, .. }));
                    assert_eq!(indices[1], IndexSpec::All);
                }
                _ => panic!("expected Index"),
            }
        }

        let p_f = parse("let f = m[:, 1];").expect("m[:, 1] ok");
        if let StmtKind::Let { init, .. } = &p_f.statements[0].kind {
            match &init.kind {
                ExprKind::Index { indices, .. } => {
                    assert_eq!(indices.len(), 2);
                    assert_eq!(indices[0], IndexSpec::All);
                    assert!(matches!(&indices[1], IndexSpec::Expr(_)));
                }
                _ => panic!("expected Index"),
            }
        }

        let p_g = parse("let g = v[v > 0.0];").expect("v[v > 0.0] ok");
        if let StmtKind::Let { init, .. } = &p_g.statements[0].kind {
            match &init.kind {
                ExprKind::Index { indices, .. } => {
                    assert_eq!(indices.len(), 1);
                    assert!(matches!(&indices[0], IndexSpec::Expr(_)));
                }
                _ => panic!("expected Index"),
            }
        }

        let p_h = parse("let h = v[[0, 2, 4]];").expect("v[[0, 2, 4]] ok");
        if let StmtKind::Let { init, .. } = &p_h.statements[0].kind {
            match &init.kind {
                ExprKind::Index { indices, .. } => {
                    assert_eq!(indices.len(), 1);
                    assert!(matches!(&indices[0], IndexSpec::Expr(_)));
                }
                _ => panic!("expected Index"),
            }
        }
    }

    #[test]
    fn test_parse_struct_trait_and_impl() {
        let code = r#"
            struct NormalDistribution {
                mean: f64,
                std_dev: f64,
            }

            trait Distribution {
                type Output;
                fn sample(&self, rng: &mut RNG) -> Self::Output;
                fn log_pdf(&self, x: Self::Output) -> f64;
                fn cdf(&self, x: Self::Output) -> f64;
            }

            impl Distribution for NormalDistribution {
                type Output = f64;
                fn log_pdf(&self, x: f64) -> f64 {
                    let diff = (x - self.mean) / self.std_dev;
                    -0.5 * diff * diff
                }
            }

            let dist = NormalDistribution { mean: 0.0, std_dev: 1.0 };
            let val = dist.log_pdf(0.5);
        "#;
        let program = parse(code).expect("syntax must parse correctly");
        assert_eq!(program.statements.len(), 5);
        match &program.statements[0].kind {
            StmtKind::Struct(s) => {
                assert_eq!(s.name, "NormalDistribution");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "mean");
            }
            _ => panic!("expected Struct"),
        }
        match &program.statements[1].kind {
            StmtKind::Trait(t) => {
                assert_eq!(t.name, "Distribution");
                assert_eq!(t.items.len(), 4);
            }
            _ => panic!("expected Trait"),
        }
        match &program.statements[2].kind {
            StmtKind::Impl(i) => {
                assert_eq!(i.trait_name, Some("Distribution".into()));
                assert_eq!(i.target_type, "NormalDistribution");
                assert_eq!(i.items.len(), 2);
            }
            _ => panic!("expected Impl"),
        }
        match &program.statements[3].kind {
            StmtKind::Let { init, .. } => {
                match &init.kind {
                    ExprKind::StructLit { name, fields } => {
                        assert_eq!(name, "NormalDistribution");
                        assert_eq!(fields.len(), 2);
                    }
                    _ => panic!("expected StructLit"),
                }
            }
            _ => panic!("expected Let"),
        }
    }

    #[test]
    fn test_parse_range_and_par_iter() {
        let code = r#"
            let r = 0..10;
            let res = (0..100).par_iter().map(|x| x * 2).collect();
            for i in 1..=5 {
                let y = i;
            }
        "#;
        let program = parse(code).expect("syntax ok");
        assert_eq!(program.statements.len(), 3);

        match &program.statements[0].kind {
            StmtKind::Let { init, .. } => {
                assert!(matches!(init.kind, ExprKind::Range { inclusive: false, .. }));
            }
            _ => panic!("expected Let"),
        }

        match &program.statements[1].kind {
            StmtKind::Let { init, .. } => {
                assert!(matches!(init.kind, ExprKind::Call { .. }));
            }
            _ => panic!("expected Let"),
        }

        match &program.statements[2].kind {
            StmtKind::Expr(e) => {
                assert!(matches!(e.kind, ExprKind::For { ref var, .. } if var == "i"));
            }
            _ => panic!("expected Expr(For)"),
        }
    }
}

