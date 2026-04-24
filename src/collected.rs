#![feature(try_trait_v2)]

use std::ops::{ControlFlow, FromResidual, Try};

// I want a new method on Iterator, which ... returning a Try type,
// I will collect up to the first Residual/Failure all the Outputs.
//

// ── The control-flow enum ─────────────────────────────────────────────────────
#[derive(Debug)]
enum Collected<V> {
    Ok(V),
    Fail(V),   // V is the partial accumulator at the point of failure
}

impl<V> Try for Collected<V> {
    type Output   = V;
    type Residual = Collected<V>;

    fn from_output(v: V) -> Self          { Collected::Ok(v) }

    fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
        match self {
            Collected::Ok(v)   => ControlFlow::Continue(v),
            Collected::Fail(v) => ControlFlow::Break(Collected::Fail(v)),
        }
    }
}

impl<V> FromResidual for Collected<V> {
    fn from_residual(r: Collected<V>) -> Self { r }
}

// ── try_collect built entirely on try_fold ────────────────────────────────────

fn try_collect<A, E>(
    iter: impl Iterator<Item = Result<A, E>>, // could be Try, right?
) -> Collected<Vec<A>> {
    iter.try_fold(Vec::new(), |mut acc, item| match item {
        Ok(a)  => { acc.push(a); Collected::Ok(acc) }
        Err(_) =>                Collected::Fail(acc),  // short-circuit
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
