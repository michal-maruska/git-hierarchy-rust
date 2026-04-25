// can I put this into ../Cargo.toml
#![deny(elided_lifetimes_in_paths)]

#[allow(unused)]
use tracing::{debug, info, warn};
use colored::Colorize;

use crate::base::{GIT_HEADS_PATTERN, git_same_ref};

use std::cell::RefCell;
use std::collections::HashSet;

use crate::graph::discover::NodeExpander;

use crate::utils::{concatenate, extract_name};

use std::ops::Try;
use std::ops::ControlFlow;
use crate::collected::{try_collect};

use git2::{Commit, Oid, Reference, Repository, Revwalk, Sort, Error};

// low level sum & segment
const SEGMENT_BASE_PATTERN: &str = "refs/base/";
const SEGMENT_START_PATTERN: &str = "refs/start/";
const SUM_SUMMAND_PATTERN: &str = "refs/sums/";
const SEPARATOR : &str = "/";

#[inline]
// can we return Cow<&str, String> ?
pub fn segment_fmt(s: &str) -> colored::ColoredString //  maybe a method?
{
    s.red().bold().underline()
}

pub fn plain_ref_fmt(s: &str) -> colored::ColoredString //  maybe a method?
{
    s.green().bold().underline()
}


#[inline]
pub fn sum_fmt(s: &str) -> colored::ColoredString //  maybe a method?
{
    s.yellow().bold().italic()
}


fn base_name(name: &str) -> String {
    concatenate(SEGMENT_BASE_PATTERN, name)
}

fn start_name(name: &str) -> String {
    concatenate(SEGMENT_START_PATTERN, name)
}

fn sum_summands<'repo>(repository: &'repo Repository, name: &str) -> Vec<Reference<'repo>> {
    let mut v = Vec::new();

    debug!("searching for sum {}", name);
    if let Ok(ref_iterator) =
        repository.references_glob(&(concatenate(SUM_SUMMAND_PATTERN, name) + "/*"))
    {
        for r in ref_iterator {
            v.push(r.unwrap());
        }
    }

    v
}

pub fn sums(repository: &Repository) -> impl Iterator<Item = String>
{
    let iterator = repository.references_glob(&concatenate(SUM_SUMMAND_PATTERN, "*/*")).unwrap();
    // so .names() is bad api!
    let all =
        iterator.map(move |r| {
            r.unwrap().name().unwrap().strip_prefix(SUM_SUMMAND_PATTERN).unwrap()
                .trim_end_matches(char::is_numeric)
                .strip_suffix("/").unwrap()
                .to_string()
        })
        .collect::<HashSet<_>>();
    all.into_iter()
}

// I want an iterator on strings.
// dyn Iterator<item = >
pub fn segments(repository: &Repository) -> impl Iterator<Item = String>
{
    let iterator = repository.references_glob(&concatenate(SEGMENT_BASE_PATTERN, "*")).unwrap();
    // so .names() is bad api!

    iterator .map(move |r| {
        r.unwrap().name().unwrap().strip_prefix(SEGMENT_BASE_PATTERN).unwrap().to_string()
    })
}

fn branch_name<'a, 'repo>(reference: &'a Reference<'repo>) -> &'a str {
    reference
        .name()
        .unwrap()
        .strip_prefix(GIT_HEADS_PATTERN)
        .unwrap()
}

/// a linear sequence of commits.
pub struct Segment<'repo> {
    name: String,
    pub reference: RefCell<Reference<'repo>>,

    pub base: RefCell<Reference<'repo>>, // I need to call &mut methods
    pub _start: Reference<'repo>,
}

const REBASED_REFLOG: &str = "Rebased";

impl<'repo> Segment<'repo> {
    pub fn create(repository: &'repo Repository,
                  name: &str,
                  // why the same?
                  base: &'_ Reference<'_>,
                  start: Oid,
                  head: Oid)
                  -> Result<Segment<'repo>, Error> {
        info!("create segment: {} base {}", name, base.name().unwrap());
        // .expect("should be a new reference");
        let s = repository.reference(&concatenate(SEGMENT_START_PATTERN, name),
                                         start,
                                         false,
                                         "start").expect("should be a new reference");
        let b = repository.reference_symbolic(&concatenate(SEGMENT_BASE_PATTERN, name),
                                              base.name().unwrap(),
                                              false,
                                              "new segment").expect("should be a new symbolic reference");

        // Branch::name_is_valid()
        let branch = repository.reference(&concatenate("refs/heads/", name), // fixme:
                                          head,
                                          false, // don't overwrite existing.
                                          "create").expect("should be a new reference");
        // unimplemented!();
        // I need to own the reference:
        Ok(Segment::new(branch, b, s))
    }

    pub fn new(
        reference: Reference<'repo>,
        base: Reference<'repo>,
        start: Reference<'repo>,
    ) -> Segment<'repo> {

        Segment::<'repo> {
            name: branch_name(&reference).to_owned(),
            reference: RefCell::new(reference),
            base: RefCell::new(base),
            _start: start,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn uptodate(&self, _repository: &Repository) -> bool {
        debug!("looking at segment: {:?} {:?} {:?}", self.name,
               self._start.target().unwrap(),
               self.base.borrow().peel_to_commit().unwrap().id());
        self.base
            .borrow().peel_to_commit().unwrap().id() == self._start.target().unwrap()
    }

    pub fn empty(&self, repository: &Repository) -> bool {
        git_same_ref(repository, &self.reference.borrow(), &self._start)
    }

    pub fn git_revisions(&self) -> String {
        format!(
            "{}..{}",
            self._start.name().unwrap(),
            self.reference.borrow().name().unwrap()
        )
    }

    // reference to head_oid
    // start to base.
    // todo: reflog message?
    pub fn reset(&self, repository: &'repo Repository, head_oid: Oid) {

        if true {
            let head_reference = self.reference.borrow();
            // I want to refresh this!
            debug!("reset: the head itself? {} with {}",
                   head_reference.name().unwrap(),
                   head_oid);
            drop(head_reference);
        }

        // we cannot extract other references from there.
        self.reference.replace_with(|r|
                                    r.set_target(head_oid, "rebased").unwrap());

        let base = self.base(repository);
        debug!("base to {:?}", base.target());
        // _peel fails!
        let oid = base.target().unwrap();
        self.set_start(repository, oid);
    }

    pub fn set_start(&self, repository: &'repo Repository, oid: Oid) {
        // fixme: what? ref -> name -> ref? b/c &self is not &mut?
        let start_ref_name = self._start.name().unwrap();
        let mut start_ref = repository.find_reference(start_ref_name).unwrap();


        // debug!("reset: {} to {}", self.name(), oid);
        info!("setting {} to {}", start_ref_name, oid);
        if start_ref.set_target(oid, REBASED_REFLOG).is_err() {
            panic!("failed to set start to new base")
        }
    }

    pub fn start(&self) -> Oid {
        self._start.target().expect("start reference should resolve to Oid")
    }

    pub fn base(&self, repository: &'repo Repository) -> Reference<'repo> {
        let reference = repository
            .find_reference(
                self.base.borrow()
                    .symbolic_target()
                    .expect("base should be a symbolic reference"),
            )
            .unwrap();
        debug!("base points at {:?}", reference.name().unwrap());
        reference
    }


    pub fn iter(&self, repository: &'repo Repository) -> Result<Revwalk<'_>, Error> {

        let mut walk = repository.revwalk()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)?;
        // target_peel fails!
        let oid = self.reference.borrow().target().unwrap();
        walk.push(oid)?;
        //_peel
        let oid = self._start.target().unwrap();
        walk.hide(oid)?;

        Ok(walk)
    }

    // todo: -> Result<Reference>
    pub fn set_base(&self, _repository: &'repo Repository, new_base: &'_ Reference<'_>) {
        let _old = self.base.replace_with(
            |x|
            x.symbolic_set_target(new_base.name().expect("provided reference must have name"),
                                  "Changing base")
                    .expect("new base"));
        debug!("old base pointed at {:?}", _old.name().unwrap());
    }
}

/// create "numbered" symbolic references pointing at the summands.
/// sum/name/1 ... sum/name/N symbolic references.
/// but delete on failure!
fn create_summand_refs<'repo,'a>(
    repository: &'repo Repository,
    sum_name: &str,
    counter_start: usize,
    components: impl Iterator<Item = &'a Reference<'repo>>
) -> Result<Vec<Reference<'repo>>, Error>
where 'repo : 'a {
    let summands = try_collect(
        components.enumerate().map(
            |(n, s)| {
                // this starts from 0
                repository.reference_symbolic(
                    &(SUM_SUMMAND_PATTERN.to_string()
                      + SEPARATOR
                      + sum_name
                      + SEPARATOR
                      + &(counter_start + 1 + n).to_string()),
                    // mmc: this panics! todo: Avoid that!
                    s.name().expect("should have name"),
                    false,
                    "start")
            }));

    match summands.branch() {
        ControlFlow::Continue(v) => Ok(v),
        ControlFlow::Break(res) => {
            // res cannot be Infallible
            for mut reference in res.unwrap_err()  {
                // should we ignore these failures?
                reference.delete()?;
            }
            Err(Error::from_str("failed"))
        }
    }
}

pub struct Sum<'repo> {
    name: String,
    pub reference: RefCell<Reference<'repo>>,
    // the symbolic refs:
    pub summands: Vec<Reference<'repo>>,
    // resolved: RefCell<Option<Vec<GitHierarchy<'repo>>>>,
}

impl<'repo> Sum<'repo> {

    pub fn new(
        reference: Reference<'repo>,
        summands: Vec<Reference<'repo>>
    ) -> Sum<'repo> {
        Sum::<'repo> {
            name: branch_name(&reference).to_owned(),
            reference: RefCell::new(reference),
            summands,
        }
    }

    pub fn reset(&self, head_oid: Oid) {
        // we cannot extract other references from there.
        // self.reference.borrow_mut().set_target(new_oid, "re-merge")
        self.reference.replace_with(|r|
                                    r.set_target(head_oid, "re-merged").unwrap());
    }

    pub fn create<'a>(
        repository: &'repo Repository,
        name: &str,
        components: impl Iterator<Item = &'a Reference<'repo>>,
        // Oid
        hint: Option<Commit<'repo>>
    ) -> Result<Sum<'repo>, Error>
        where 'repo : 'a
    {
        info!("create sum: {}", name);
        let summands = create_summand_refs(repository, name, 0, components)?;


        let h = repository.branch(name,
                                  // either at hinted
                                  &hint.unwrap_or(summands[0].peel_to_commit().unwrap()),
                                  // &head.peel_to_commit().unwrap()
                                  // bug: we must drop the symbolic refs again!
                                  false)?; // .expect("should be a new reference");
        Ok(Self::new(h.into_reference(),
                     summands))
    }

    pub fn add_summands<'a>(
        &mut self,
        repository: &'repo Repository,
        // todo: IntoIter
        components: impl Iterator<Item = &'a Reference<'repo>>,
        // I need mut to take ownership of items.
        // Oid
        _hint: Option<Commit<'repo>>
    ) -> Result<(), Error>
    where 'repo : 'a {

        let summands = &self.summands;
        // find:
        let mut max : usize = 0;
        for i in summands {
            eprintln!("summand {}", i.name().unwrap());
            let n = i.name().expect("must have name").strip_prefix(SUM_SUMMAND_PATTERN).expect("must have SUM prefix")
                .strip_prefix(&self.name).expect("owned by the sum")
                .strip_prefix("/").expect("/");
            let index = n.parse::<usize>().expect("should be numeric");
            // eprintln!("summand {}", index);
            if max < index {
                max = index;
            }
        }
        let mut new_summands = create_summand_refs(repository, &self.name, max, components)?;

        self.summands.append(&mut new_summands);
        Ok(())
    }


    // todo: iterator?
    pub fn summands(&self, repository: &'repo Repository) -> Vec<Reference<'repo>> {
        debug!("resolving summands for {:?}", self.name());
        // = Vec::with_capacity(self.summands.len());
        self.summands.iter().map(
            |summand| {
                let symbolic_base = repository.find_reference(
                    summand.symbolic_target().expect("base should be a symbolic reference"),
                ).unwrap();

                debug!("{:?} -> {:?}", summand.name().unwrap(),
                       symbolic_base.name().unwrap());
                symbolic_base
            }).collect()
    }

    pub fn summand_count(&self) -> usize {
        self.reference.borrow().peel_to_commit().unwrap().parent_count()
    }

    pub fn name(&self) -> &str {
        // fixme: same as ....
        // branch_name(&self.reference.borrow());
        &self.name
    }

    pub fn parent_commits(&self) -> Vec<Oid> {
        let commit = self.reference.borrow().peel_to_commit().unwrap();
        commit.parent_ids().collect()
    }
}

pub enum GitHierarchy<'repo> {
    Name(String),

    Segment(Segment<'repo>),
    Sum(Sum<'repo>),

    Reference(Reference<'repo>),
}

impl<'repo> GitHierarchy<'repo> {
    pub fn commit(&self) -> Result<Commit<'_>, git2::Error> {
        let reference: &Reference<'_> = match &self {
            GitHierarchy::Name(x) => {
                eprintln!("trying {x}");
                // fixme:
                panic!("bad state");
                // unimplemented!(),
            }
            GitHierarchy::Segment(s) => &s.reference.borrow(),
            GitHierarchy::Sum(s) => &s.reference.borrow(),
            GitHierarchy::Reference(r) => r,
        };
        reference.peel_to_commit()
    }
}

//  Vertex -> 1st stage children       ..... looked up if already in the graph/queue.
//            1st stage ----(convert)---> Vertices.
// Given GH::Name,
// spreadsheet  Cell -> Formula & references.
pub fn load<'repo>(
    repository: &'repo Repository,
    name: &'_ str,
) -> Result<GitHierarchy<'repo>, git2::Error> {
    let name = extract_name(name);
    let reference = repository.resolve_reference_from_short_name(name)?;

    if let Ok(base) = repository.find_reference(base_name(name).as_str()) {
        if let Ok(start) = repository.find_reference(start_name(name).as_str()) {
            info!("segment detected: {}", name);
            return Ok(GitHierarchy::Segment(Segment::new(reference, base, start)));
        } else {
            return Err(git2::Error::from_str("start not found"));
        };
    }

    let summands = sum_summands(repository, name);
    if !summands.is_empty() {
        info!("sum detected: {}", name);
        return Ok(GitHierarchy::Sum(Sum::new(reference, summands)))
    };

    info!("plain reference: {}", name);
    Ok(GitHierarchy::Reference(reference))
}

// note: trait items always share the visibility of their trait
impl<'a> NodeExpander for GitHierarchy<'a> {
    fn node_identity(&self) -> &str {
        match self {
            Self::Name(x) => x,
            GitHierarchy::Segment(s) => s.name(),
            GitHierarchy::Sum(s) => s.name(),
            GitHierarchy::Reference(r) => r.name().unwrap(),
        }
    }
}
