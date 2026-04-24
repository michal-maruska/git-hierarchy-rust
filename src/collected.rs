use std::ops::{ControlFlow, FromResidual, Try};
use core::convert::Infallible;

// I want a new method on Iterator, which ... returning a Try type,
// I will collect up to the first Residual/Failure all the Outputs.
//

// ── The control-flow enum ─────────────────────────────────────────────────────
#[derive(Debug)]
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
        println!("{:?}", try_collect(input.iter().map(|s| s.parse::<i32>())));
        // ↳  Ok([1, 2, 3])

        let input = vec!["1", "2", "oops", "4"];
        println!("{:?}", try_collect(input.iter().map(|s| s.parse::<i32>())));
        // ↳  Fail([1, 2])   — "4" never visited
    }
}
