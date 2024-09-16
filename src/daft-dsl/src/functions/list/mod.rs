mod count;
mod get;
mod max;
mod mean;
mod min;
mod slice;
mod sum;

use count::CountEvaluator;
use daft_core::count_mode::CountMode;
use get::GetEvaluator;
use max::MaxEvaluator;
use mean::MeanEvaluator;
use min::MinEvaluator;
use serde::{Deserialize, Serialize};
use slice::SliceEvaluator;
use sum::SumEvaluator;

use crate::{Expr, ExprRef};

use super::FunctionEvaluator;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ListExpr {
    Count(CountMode),
    Get,
    Sum,
    Mean,
    Min,
    Max,
    Slice,
}

impl ListExpr {
    #[inline]
    pub fn get_evaluator(&self) -> &dyn FunctionEvaluator {
        use ListExpr::*;
        match self {
            Count(_) => &CountEvaluator {},
            Get => &GetEvaluator {},
            Sum => &SumEvaluator {},
            Mean => &MeanEvaluator {},
            Min => &MinEvaluator {},
            Max => &MaxEvaluator {},
            Slice => &SliceEvaluator {},
        }
    }
}

pub fn count(input: ExprRef, mode: CountMode) -> ExprRef {
    Expr::Function {
        func: super::FunctionExpr::List(ListExpr::Count(mode)),
        inputs: vec![input],
    }
    .into()
}

pub fn get(input: ExprRef, idx: ExprRef, default: ExprRef) -> ExprRef {
    Expr::Function {
        func: super::FunctionExpr::List(ListExpr::Get),
        inputs: vec![input, idx, default],
    }
    .into()
}

pub fn sum(input: ExprRef) -> ExprRef {
    Expr::Function {
        func: super::FunctionExpr::List(ListExpr::Sum),
        inputs: vec![input],
    }
    .into()
}

pub fn mean(input: ExprRef) -> ExprRef {
    Expr::Function {
        func: super::FunctionExpr::List(ListExpr::Mean),
        inputs: vec![input],
    }
    .into()
}

pub fn min(input: ExprRef) -> ExprRef {
    Expr::Function {
        func: super::FunctionExpr::List(ListExpr::Min),
        inputs: vec![input],
    }
    .into()
}

pub fn max(input: ExprRef) -> ExprRef {
    Expr::Function {
        func: super::FunctionExpr::List(ListExpr::Max),
        inputs: vec![input],
    }
    .into()
}

pub fn slice(input: ExprRef, start: ExprRef, end: ExprRef) -> ExprRef {
    Expr::Function {
        func: super::FunctionExpr::List(ListExpr::Slice),
        inputs: vec![input, start, end],
    }
    .into()
}
