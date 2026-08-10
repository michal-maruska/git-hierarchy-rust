use std::collections::HashSet;
use std::hash::Hash;
use tracing_subscriber::{FmtSubscriber,self};
use std::borrow::Borrow;

pub fn concatenate(prefix: &str, suffix: &str) -> String {
    let mut s = String::from(prefix);
    s.push_str(suffix);
    s
}

pub fn extract_name(refname: &str) -> &str {
    let mut a = refname.strip_prefix("ref: ").unwrap_or(refname);
    a = a.strip_prefix("refs/").unwrap_or(a);
    a = a.strip_prefix("heads/").unwrap_or(a);
    a
}

// Return: iter2 - hash(iter1)
pub fn iterator_difference<T, U, I1, I2>(iter1: I1, iter2: I2) -> impl Iterator<Item = U>
where
    T: Hash + Eq + 'static,
    U: Borrow<T> + Clone,
    I1: IntoIterator<Item = U>,
    I2: IntoIterator<Item = T>,
{
    let hashed_set: HashSet<T> = iter2.into_iter().collect();

    iter1.into_iter()
        .filter(move |item|
                !hashed_set.contains(item.borrow()))
}


// returns: I1 - I2,  I2 - I1
// but the order is different
pub fn iterator_symmetric_difference<T, U, I1, I2>(iter1: I1, iter2: I2) -> (Vec<T>, Vec<U>)
where
    T: Hash + Eq,
    U: Borrow<T> + Clone,
    I1: IntoIterator<Item = T>,
    I2: IntoIterator<Item = U>,
{
    let mut hashed_set: HashSet<T> = iter1.into_iter().collect();
    let mut not_found = Vec::<U>::new();

    iter2.into_iter()
        .for_each(|item|
                  if hashed_set.contains(item.borrow()) {
                      hashed_set.remove(item.borrow());
                  } else {
                      not_found.push(item);
                  }
        );

    (hashed_set.drain().collect(),
     not_found)
}

// why  IntoIter ? we take ownership?
pub fn iterator_symmetric_difference_indirect<T, K, U, I1, I2, MappingFn>(iter1: I1, iter2: I2, mapping: MappingFn)
 -> (Vec<T>, Vec<K>)
where
    T: Hash + Eq,
    I1: IntoIterator<Item = T>,
    I2: IntoIterator<Item = K>,
    MappingFn: Fn(K) -> U,
    K: Clone,
    U: Borrow<T>
{
    let mut hashed_set: HashSet<T> = iter1.into_iter().collect();
    let mut not_found = Vec::<K>::new();

    iter2.into_iter()
        .for_each(|item|
                  // Entry api ... has not remove/delete.
                  if !hashed_set.remove(mapping(item.clone()).borrow()) {
                      not_found.push(item);
                  }
        );
    (hashed_set.drain().collect(), not_found)
}




pub fn init_tracing(verbose: u8) {
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        tracing::subscriber::set_global_default(
            FmtSubscriber::builder().with_env_filter(rust_log).finish(),
        ).expect("tracing setup failed");
    } else {
        if verbose > 1 {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .init();
        } else if verbose == 1 {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .init();
        } else {
            tracing_subscriber::fmt::init();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_concatenate() {
        assert_eq!("Hello World", concatenate("Hello ", "World"));
    }

    #[test]
    fn test_extract_name() {
        assert_eq!("name", extract_name("name"));
        assert_eq!("name", extract_name("heads/name"));
        assert_eq!("name", extract_name("refs/heads/name"));
        assert_eq!("feature/branch", extract_name("ref: refs/heads/feature/branch"));
        assert_eq!("master", extract_name("ref: master"));
    }

    #[test]
    fn test_iterator_symmetric_difference() {
        let real = [1, 2, 10, 16];
        let selected = [0, 2, 5, 6];

        let (mut unselected, missing) = iterator_symmetric_difference(
            real.iter(),
            selected.iter()
            );

        // found is only &
        unselected.sort();
        assert_eq!(
            &unselected,
            // not found in first, which *are* in 2nd
            &[&1,&10,&16]
        );

        assert_eq!(
            &missing,
            // not found in first, which *are* in 2nd
            &[&0,&5,&6]
        )
    }

    #[test]
    fn test_iterator_symmetric_difference_indirect() {
        #[derive(Clone)]
        struct Wrapper(i32);

        let set1 = vec![1, 2, 10, 16];
        let set2 = vec![Wrapper(0), Wrapper(2), Wrapper(5), Wrapper(6)];

        let (mut unselected, missing) = iterator_symmetric_difference_indirect(
            set1,
            set2,
            |w: Wrapper| w.0,
        );

        unselected.sort();
        assert_eq!(unselected, vec![1, 10, 16]);

        let missing_vals: Vec<i32> = missing.into_iter().map(|w| w.0).collect();
        assert_eq!(missing_vals, vec![0, 5, 6]);
    }

    #[test]
    fn test_iterator_difference () {
        let real = [1, 2, 10, 16];
        let selected = [0, 2, 5, 6];

        let mut minus  = iterator_difference(
            selected.iter(),
            real.into_iter(),
            );

        // found is only &
        assert_eq!(
            minus.next(),
            Some(&0));
        assert_eq!(
            minus.next(),
            Some(&5));

        assert_eq!(
            minus.next(),
            Some(&6));

        assert_eq!(
            minus.next(),
            None);
    }
}
