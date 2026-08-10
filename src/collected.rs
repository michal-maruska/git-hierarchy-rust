use std::ops::{ControlFlow, FromResidual, Try};
use core::convert::Infallible;

// I want a new method on Iterator, which ... returning a Try type,
// I will collect up to the first Residual/Failure all the Outputs.
//

// ── The control-flow enum ─────────────────────────────────────────────────────
#[derive(Debug, PartialEq, Eq)]
pub enum Collected<V> {
    Ok(V),
    Fail(V),   // V is the partial accumulator at the point of failure
}

impl<V> Try for Collected<V> {
    type Output   = V;
    type Residual = Result<Infallible,V>;

    fn from_output(v: V) -> Self { Collected::Ok(v) }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self {
            Collected::Ok(v)   => ControlFlow::Continue(v),
            Collected::Fail(v) => ControlFlow::Break(Err(v)),  // ← Err(v) wraps V
        }
    }
}

impl<V> FromResidual<Result<Infallible, V>> for Collected<V> {
    fn from_residual(r: Result<Infallible, V>) -> Self {
        match r {
            Err(v) => Collected::Fail(v),
            Ok(i)  => match i {},  // Infallible: this arm is unreachable
        }
    }
}

// ── try_collect: generic over any T: Try ─────────────────────────────────────
//
// We call branch() ourselves so acc is never moved before we decide
// what to do with it.

pub fn try_collect<T>(mut iter: impl Iterator<Item = T>) -> Collected<Vec<T::Output>>
where
    T: Try,
{
    iter.try_fold(Vec::new(), |mut acc, item| {
        match item.branch() {
            ControlFlow::Continue(v) => { acc.push(v); Collected::Ok(acc) }
            ControlFlow::Break(_)    =>                Collected::Fail(acc),
        }
    })
}

// ── demo ──────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_simple() {
        let input = vec!["1", "2", "3"];
        let res = try_collect(input.iter().map(|s| s.parse::<i32>()));
        assert_eq!(res, Collected::Ok(vec![1, 2, 3]));

        let input = vec!["1", "2", "oops", "4"];
        let res = try_collect(input.iter().map(|s| s.parse::<i32>()));
        assert_eq!(res, Collected::Fail(vec![1, 2]));
    }

    #[test]
    fn test_empty_input() {
        let input: Vec<&str> = vec![];
        let res = try_collect(input.iter().map(|s| s.parse::<i32>()));
        assert_eq!(res, Collected::Ok(vec![]));
    }

    #[test]
    fn test_collected_try_trait() {
        let ok_c = Collected::from_output(42);
        assert_eq!(ok_c, Collected::Ok(42));
        assert!(matches!(ok_c.branch(), ControlFlow::Continue(42)));

        let fail_c = Collected::Fail(10);
        assert!(matches!(fail_c.branch(), ControlFlow::Break(Err(10))));

        let residual: Result<Infallible, i32> = Err(99);
        let from_res = Collected::from_residual(residual);
        assert_eq!(from_res, Collected::Fail(99));
    }
}
