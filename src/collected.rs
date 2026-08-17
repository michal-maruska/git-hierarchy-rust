// ── The control-flow enum ─────────────────────────────────────────────────────
#[derive(Debug, PartialEq, Eq)]
pub enum Collected<V> {
    Ok(V),
    Fail(V),   // V is the partial accumulator at the point of failure
}

// ── try_collect: stable implementation for Result-producing iterators ────────
pub fn try_collect<T, E>(iter: impl Iterator<Item = Result<T, E>>) -> Collected<Vec<T>> {
    let mut acc = Vec::new();
    for item in iter {
        match item {
            Ok(v) => acc.push(v),
            Err(_) => return Collected::Fail(acc),
        }
    }
    Collected::Ok(acc)
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
}
