// we have a vector of elements
// we have an order (permutation)   5 6 3....
// which means the permutation   a[i] -> i
// and we want to apply this permutation by swap()

pub fn reorder_by_permutation<T>(vec: &mut [T], permutation: &[usize]) {
    assert_eq!(
        vec.len(),
        permutation.len(),
        "Vector and permutation must have the same length"
    );

    let mut visited = vec![false; vec.len()];

    for start in 0..vec.len() {
        if visited[start] {
            continue;
        }

        // For each cycle, we need to rotate elements
        // If we have cycle a -> b -> c -> a, we do: swap(a,b), swap(a,c)
        let mut current = start;
        let mut next = permutation[current];

        while next != start {
            vec.swap(current, next);
            visited[current] = true;
            current = next;
            next = permutation[current];
        }
        visited[current] = true;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_simple() {
        let string = "Hello World";

        let mut characters: Vec<char> = string.chars().collect();
        let mut permutation: Vec<usize> = (0..string.len()).collect();

        permutation.swap(0, 6);
        reorder_by_permutation(&mut characters, &permutation);

        let s: String = characters.into_iter().collect();
        assert_eq!(s, "Wello Horld");
    }

    #[test]
    fn test_identity_permutation() {
        let mut data = vec!["a", "b", "c", "d"];
        let perm = vec![0, 1, 2, 3];
        reorder_by_permutation(&mut data, &perm);
        assert_eq!(data, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_rotation_cycle() {
        // [10, 20, 30] -> permutation [1, 2, 0] means element at 0 moves to 1, 1 to 2, 2 to 0
        let mut data = vec![10, 20, 30];
        let perm = vec![1, 2, 0];
        reorder_by_permutation(&mut data, &perm);
        assert_eq!(data, vec![30, 10, 20]);
    }

    #[test]
    fn test_multiple_disjoint_cycles() {
        let mut data = vec![10, 20, 30, 40];
        let perm = vec![1, 0, 3, 2];
        reorder_by_permutation(&mut data, &perm);
        assert_eq!(data, vec![20, 10, 40, 30]);
    }

    #[test]
    fn test_empty_and_single() {
        let mut empty: Vec<i32> = vec![];
        reorder_by_permutation(&mut empty, &[]);
        assert!(empty.is_empty());

        let mut single = vec![42];
        reorder_by_permutation(&mut single, &[0]);
        assert_eq!(single, vec![42]);
    }

    #[test]
    #[should_panic(expected = "Vector and permutation must have the same length")]
    fn test_mismatched_lengths_panics() {
        let mut data = vec![1, 2];
        reorder_by_permutation(&mut data, &[0, 1, 2]);
    }
}
