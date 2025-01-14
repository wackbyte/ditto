mod common;
mod env;
pub mod pre_ast;
mod scheme;
mod state;
mod substitution;
#[cfg(test)]
mod tests;

pub use common::*;
pub use env::*;
use pre_ast as pre;
pub use scheme::*;
pub use state::*;
use substitution::*;

use crate::{
    kindchecker::{self, TypeReferences},
    result::{Result, TypeError, Warning, Warnings},
    supply::Supply,
};
use ditto_ast::{unqualified, Argument, Expression, FunctionBinder, PrimType, Span, Type};
use ditto_cst as cst;
use std::collections::HashSet;

#[cfg(test)]
pub fn typecheck(
    cst_type_annotation: Option<cst::TypeAnnotation>,
    cst_expression: cst::Expression,
) -> Result<(
    Expression,
    ValueReferences,
    ConstructorReferences,
    TypeReferences,
    Warnings,
    Supply,
)> {
    typecheck_with(
        &kindchecker::Env::default(),
        &Env::default(),
        Supply::default(),
        cst_type_annotation,
        cst_expression,
    )
}

pub fn typecheck_with(
    kindchecker_env: &kindchecker::Env,
    env: &Env,
    supply: Supply,
    cst_type_annotation: Option<cst::TypeAnnotation>,
    cst_expression: cst::Expression,
) -> Result<(
    Expression,
    ValueReferences,
    ConstructorReferences,
    TypeReferences,
    Warnings,
    Supply,
)> {
    if let Some(type_annotation) = cst_type_annotation {
        let (expr, expected, mut warnings, type_references, supply) =
            pre::Expression::from_cst_annotated(
                kindchecker_env,
                supply,
                type_annotation,
                cst_expression,
            )?;

        let mut state = State {
            supply,
            ..State::default()
        };
        let expression = check(env, &mut state, expected, expr)?;
        let State {
            substitution,
            warnings: more_warnings,
            value_references,
            constructor_references,
            supply,
            ..
        } = state;
        warnings.extend(more_warnings);
        let expression = substitution.apply_expression(expression);
        Ok((
            expression,
            value_references,
            constructor_references,
            type_references,
            warnings,
            supply,
        ))
    } else {
        let (expr, mut warnings, type_references, supply) =
            pre::Expression::from_cst(kindchecker_env, supply, cst_expression)?;

        let mut state = State {
            supply,
            ..State::default()
        };
        let expression = infer(env, &mut state, expr)?;
        let State {
            substitution,
            warnings: more_warnings,
            value_references,
            constructor_references,
            supply,
            ..
        } = state;
        warnings.extend(more_warnings);
        let expression = substitution.apply_expression(expression);
        Ok((
            expression,
            value_references,
            constructor_references,
            type_references,
            warnings,
            supply,
        ))
    }
}

pub fn infer(env: &Env, state: &mut State, expr: pre::Expression) -> Result<Expression> {
    match expr {
        pre::Expression::True { span } => Ok(Expression::True { span }),
        pre::Expression::False { span } => Ok(Expression::False { span }),
        pre::Expression::Unit { span } => Ok(Expression::Unit { span }),
        pre::Expression::String { span, value } => Ok(Expression::String { span, value }),
        pre::Expression::Int { span, value } => Ok(Expression::Int { span, value }),
        pre::Expression::Float { span, value } => Ok(Expression::Float { span, value }),
        pre::Expression::Array { span, elements } => {
            if let Some((head, tail)) = split_first_owned(elements) {
                let head = infer(env, state, head)?;
                let element_type = head.get_type();
                let mut elements = vec![head];
                for element in tail {
                    let element = check(env, state, element_type.clone(), element)?;
                    elements.push(element);
                }
                Ok(Expression::Array {
                    span,
                    element_type,
                    elements,
                })
            } else {
                let element_type = state.supply.fresh_type();
                let elements = Vec::new();
                Ok(Expression::Array {
                    span,
                    element_type,
                    elements,
                })
            }
        }
        pre::Expression::Variable { span, variable } => {
            if let Some(count) = state.value_references.get_mut(&variable) {
                *count += 1
            } else {
                state.value_references.insert(variable.clone(), 1);
            }
            env.values
                .get(&variable)
                .map(|value| value.to_expression(span, &mut state.supply))
                .ok_or_else(|| {
                    let names_in_scope = env.values.keys().cloned().collect();
                    TypeError::UnknownVariable {
                        span,
                        variable,
                        names_in_scope,
                    }
                })
        }
        pre::Expression::Constructor { span, constructor } => {
            if let Some(count) = state.constructor_references.get_mut(&constructor) {
                *count += 1
            } else {
                state.constructor_references.insert(constructor.clone(), 1);
            }
            env.constructors
                .get(&constructor)
                .map(|constructor| constructor.to_expression(span, &mut state.supply))
                .ok_or_else(|| {
                    let ctors_in_scope = env.constructors.keys().cloned().collect();
                    TypeError::UnknownConstructor {
                        span,
                        constructor,
                        ctors_in_scope,
                    }
                })
        }
        pre::Expression::If {
            span,
            box condition,
            box true_clause,
            box false_clause,
        } => {
            let condition = check(env, state, Type::PrimConstructor(PrimType::Bool), condition)?;
            let true_clause = infer(env, state, true_clause)?;
            let true_type = state.substitution.apply(true_clause.get_type());
            let false_clause = check(env, state, true_type.clone(), false_clause)?;
            Ok(Expression::If {
                span,
                output_type: true_type,
                condition: Box::new(condition),
                true_clause: Box::new(true_clause),
                false_clause: Box::new(false_clause),
            })
        }
        pre::Expression::Call {
            span,
            box function,
            arguments,
        } => {
            let function = infer(env, state, function)?;
            let function_type = state.substitution.apply(function.get_type());

            match function_type {
                Type::Function {
                    parameters,
                    return_type: box call_type,
                } => {
                    let arguments_len = arguments.len();
                    let parameters_len = parameters.len();
                    if arguments_len != parameters_len {
                        return Err(TypeError::ArgumentLengthMismatch {
                            function_span: function.get_span(),
                            wanted: parameters_len,
                            got: arguments_len,
                        });
                    }
                    let arguments = arguments
                        .into_iter()
                        .zip(parameters.into_iter())
                        .map(|(arg, expected)| match arg {
                            pre::Argument::Expression(expr) => {
                                check(env, state, expected, expr).map(Argument::Expression)
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;

                    Ok(Expression::Call {
                        span,
                        call_type,
                        function: Box::new(function),
                        arguments,
                    })
                }
                type_variable @ Type::Variable { .. } => {
                    let arguments = arguments
                        .into_iter()
                        .map(|arg| match arg {
                            pre::Argument::Expression(expr) => {
                                infer(env, state, expr).map(Argument::Expression)
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;

                    let parameters = arguments.iter().map(|arg| arg.get_type()).collect();

                    let call_type = state.supply.fresh_type();

                    let constraint = Constraint {
                        expected: Type::Function {
                            parameters,
                            return_type: Box::new(call_type.clone()),
                        },
                        actual: type_variable,
                    };
                    unify(state, function.get_span(), constraint)?;

                    Ok(Expression::Call {
                        span,
                        call_type,
                        function: Box::new(function),
                        arguments,
                    })
                }
                _ => Err(TypeError::NotAFunction {
                    span: function.get_span(),
                    actual_type: function_type,
                }),
            }
        }
        pre::Expression::Function {
            span,
            binders: pre_binders,
            return_type_annotation,
            box body,
        } => {
            let mut binders = Vec::new();

            let mut env_values = env.values.clone();

            let mut original_value_references = ValueReferences::new();

            for binder in pre_binders {
                match binder {
                    pre_ast::FunctionBinder::Name {
                        span,
                        type_annotation,
                        value,
                    } => {
                        // Check this binder doesn't conflict with existing binders
                        let conflict = binders.iter().find_map(|binder| match binder {
                            FunctionBinder::Name {
                                span: found_span,
                                value: found_value,
                                ..
                            } if value == *found_value => Some(*found_span),
                            _ => None,
                        });

                        if let Some(previous_binder) = conflict {
                            return Err(TypeError::DuplicateFunctionBinder {
                                previous_binder,
                                duplicate_binder: span,
                            });
                        }

                        let binder_type =
                            type_annotation.unwrap_or_else(|| state.supply.fresh_type());

                        let qualified_name = unqualified(value.clone());

                        if let Some(count) = state.value_references.remove(&qualified_name) {
                            original_value_references.insert(qualified_name.clone(), count);
                            state.value_references.insert(qualified_name.clone(), 0);
                        }

                        env_values.insert(
                            qualified_name,
                            EnvValue::ModuleValue {
                                span,
                                variable_scheme: Scheme {
                                    forall: HashSet::new(),
                                    signature: binder_type.clone(),
                                },
                                variable: value.clone(),
                            },
                        );

                        binders.push(FunctionBinder::Name {
                            span,
                            binder_type,
                            value,
                        });
                    }
                }
            }
            let env = Env {
                values: env_values,
                constructors: env.constructors.clone(),
            };
            let body = if let Some(expected) = return_type_annotation {
                check(&env, state, expected, body)?
            } else {
                infer(&env, state, body)?
            };

            // Check for unused binders
            for FunctionBinder::Name { span, value, .. } in binders.iter() {
                let qualified_name = unqualified(value.clone());
                if !state.value_references.contains_key(&qualified_name) {
                    state
                        .warnings
                        .push(Warning::UnusedFunctionBinder { span: *span });
                } else {
                    state.value_references.remove(&qualified_name);
                }
            }

            // Restore shadowed reference counts
            state.value_references.extend(original_value_references);

            Ok(Expression::Function {
                span,
                binders,
                body: Box::new(body),
            })
        }
    }
}

pub fn check(
    env: &Env,
    state: &mut State,
    expected: Type,
    expr: pre::Expression,
) -> Result<Expression> {
    let expression = infer(env, state, expr)?;
    unify(
        state,
        expression.get_span(),
        Constraint {
            expected,
            actual: expression.get_type(),
        },
    )?;
    Ok(expression)
}

#[derive(Debug)]
pub struct Constraint {
    expected: Type,
    actual: Type,
}

impl Substitution {
    pub fn apply_constraint(&self, Constraint { expected, actual }: Constraint) -> Constraint {
        Constraint {
            expected: self.apply(expected),
            actual: self.apply(actual),
        }
    }
}

fn unify(state: &mut State, span: Span, constraint: Constraint) -> Result<()> {
    unify_else(state, span, constraint, None)
}

fn unify_else(
    state: &mut State,
    span: Span,
    constraint: Constraint,
    err: Option<&TypeError>,
) -> Result<()> {
    match state.substitution.apply_constraint(constraint) {
        // An explicitly named type variable (named in the source) will only unify
        // with another type variable with the same name, or an anonymous type
        // variable.
        //
        // For example, the following shouldn't typecheck
        //    five : a = 5;
        //
        Constraint {
            expected:
                Type::Variable {
                    source_name: Some(expected),
                    ..
                },
            actual:
                Type::Variable {
                    source_name: Some(actual),
                    ..
                },
        } if expected == actual => Ok(()),

        // Anonymous variables are bound to new types
        Constraint {
            expected:
                Type::Variable {
                    source_name: None,
                    var,
                    ..
                },
            actual: t,
        } => bind(state, span, var, t),
        Constraint {
            expected: t,
            actual:
                Type::Variable {
                    source_name: None,
                    var,
                    ..
                },
        } => bind(state, span, var, t),

        Constraint {
            expected:
                Type::Constructor {
                    canonical_value: expected,
                    ..
                },
            actual:
                Type::Constructor {
                    canonical_value: actual,
                    ..
                },
        } if expected == actual => Ok(()),

        Constraint {
            expected: Type::PrimConstructor(expected),
            actual: Type::PrimConstructor(actual),
        } if expected == actual => Ok(()),

        Constraint {
            expected:
                Type::Call {
                    function: box expected_function,
                    arguments: expected_arguments,
                },
            actual:
                Type::Call {
                    function: box actual_function,
                    arguments: actual_arguments,
                },
        } => {
            let err = TypeError::TypesNotEqual {
                span,
                expected: Type::Call {
                    function: Box::new(expected_function.clone()),
                    arguments: expected_arguments.clone(),
                },
                actual: Type::Call {
                    function: Box::new(actual_function.clone()),
                    arguments: actual_arguments.clone(),
                },
            };
            unify_else(
                state,
                span,
                Constraint {
                    expected: expected_function,
                    actual: actual_function,
                },
                Some(&err),
            )?;
            let arguments = expected_arguments
                .into_iter()
                .zip(actual_arguments.into_iter());

            for (expected_arg, actual_arg) in arguments {
                unify_else(
                    state,
                    span,
                    Constraint {
                        expected: expected_arg.clone(),
                        actual: actual_arg.clone(),
                    },
                    Some(&err),
                )?;
            }

            Ok(())
        }
        Constraint {
            expected:
                Type::Function {
                    parameters: expected_parameters,
                    return_type: box expected_return_type,
                },
            actual:
                Type::Function {
                    parameters: actual_parameters,
                    return_type: box actual_return_type,
                },
        } => {
            let err = TypeError::TypesNotEqual {
                span,
                expected: Type::Function {
                    parameters: expected_parameters.clone(),
                    return_type: Box::new(expected_return_type.clone()),
                },
                actual: Type::Function {
                    parameters: actual_parameters.clone(),
                    return_type: Box::new(actual_return_type.clone()),
                },
            };
            let parameters = expected_parameters
                .into_iter()
                .zip(actual_parameters.into_iter());

            for (expected_param, actual_param) in parameters {
                unify_else(
                    state,
                    span,
                    Constraint {
                        expected: expected_param.clone(),
                        actual: actual_param.clone(),
                    },
                    Some(&err),
                )?;
            }
            unify_else(
                state,
                span,
                Constraint {
                    expected: expected_return_type,
                    actual: actual_return_type,
                },
                Some(&err),
            )?;

            Ok(())
        }

        // BANG
        Constraint { expected, actual } => Err(err.cloned().unwrap_or(TypeError::TypesNotEqual {
            span,
            expected,
            actual,
        })),
    }
}

fn bind(state: &mut State, span: Span, var: usize, t: Type) -> Result<()> {
    if let Type::Variable { var: var_, .. } = t {
        if var == var_ {
            return Ok(());
        }
    }
    occurs_check(span, var, &t)?;
    state.substitution.insert(var, t);
    Ok(())
}

fn occurs_check(span: Span, var: usize, t: &Type) -> Result<()> {
    if type_variables(t).contains(&var) {
        return Err(TypeError::InfiniteType {
            span,
            var,
            infinite_type: t.clone(),
        });
    }
    Ok(())
}

// move to a common utils module?
fn split_first_owned<T>(xs: Vec<T>) -> Option<(T, impl Iterator<Item = T>)> {
    let mut iter = xs.into_iter();
    iter.next().map(|head| (head, iter))
}
