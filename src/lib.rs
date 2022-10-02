#![feature(auto_traits)]
#![feature(negative_impls)]
#![feature(trait_alias)]

use std::thread::{self, JoinHandle};

mod graph;
mod hierarchical_state;
mod util;

use util::{SafeMutRefComputation, SafeType, SharedMutex};

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

struct DisjointUnion<Left, Right> {
    left: Left,
    right: Right,
}

trait Joinable {
    type Output;
    fn join(self) -> Self::Output;
}

impl<T> Joinable for JoinHandle<T> {
    type Output = T;
    fn join(self) -> T {
        self.join().expect("Uggh error handling")
    }
}

struct JoinUnion<LeftJoinable: Joinable, RightJoinabl: Joinable> {
    left: LeftJoinable,
    right: RightJoinabl,
}
impl<LeftOutput, RightOutput, LeftJoinable, RightJoinabl> Joinable
    for JoinUnion<LeftJoinable, RightJoinabl>
where
    LeftJoinable: Joinable<Output = LeftOutput>,
    RightJoinabl: Joinable<Output = RightOutput>,
{
    type Output = (LeftOutput, RightOutput);

    fn join(self) -> Self::Output {
        (self.left.join(), self.right.join())
    }
}
trait Source {}

trait Graph: Sized {
    type CurrentLock;
    type LockInnerType: Send + Sync;
    type Output;
    type JoinOutput: Joinable;

    fn run(self) -> Self::JoinOutput;

    fn parallel<Other: Graph<LockInnerType = Self::LockInnerType>>(
        self,
        other: Other,
    ) -> DisjointUnion<Self, Other> {
        DisjointUnion {
            left: self,
            right: other,
        }
    }
}

struct StateManipulationLoop<State: SafeType + 'static, F: SafeMutRefComputation<State, bool>> {
    state: SharedMutex<State>,
    transition: F,
    done: bool,
}

impl<State, Left, Right, LeftJoinable, RightJoinable> Graph for DisjointUnion<Left, Right>
where
    State: SafeType,
    Left: Graph<LockInnerType = State, JoinOutput = LeftJoinable>,
    Right: Graph<LockInnerType = State, JoinOutput = RightJoinable>,
    LeftJoinable: Joinable,
    RightJoinable: Joinable,
{
    type CurrentLock = SharedMutex<State>;

    type LockInnerType = State;

    type Output = bool;
    type JoinOutput = JoinUnion<LeftJoinable, RightJoinable>;

    fn run(self) -> Self::JoinOutput {
        JoinUnion {
            left: self.left.run(),
            right: self.right.run(),
        }
    }
}

impl<State: SafeType + 'static, F> Graph for StateManipulationLoop<State, F>
where
    F: SafeMutRefComputation<State, bool>,
{
    type CurrentLock = SharedMutex<State>;
    type LockInnerType = State;
    type Output = bool;
    type JoinOutput = JoinHandle<()>;
    fn run(mut self) -> JoinHandle<()> {
        thread::spawn(move || {
            while !self.done {
                let mut guard = self.state.lock().unwrap();
                let state: &mut State = &mut guard;
                self.done = (self.transition)(state);
            }
        })
    }
}

impl<State: SafeType + 'static, F: SafeMutRefComputation<State, bool>>
    StateManipulationLoop<State, F>
{
    fn new(state: SharedMutex<State>, compute: F) -> Self {
        Self {
            state,
            transition: compute,
            done: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const LEN: usize = 100000;
    use util::new_shared;

    #[test]
    fn drain_vec_parallel() {
        let input_data = (1..LEN).collect::<Vec<_>>();
        let output_data = Vec::<u64>::with_capacity(LEN);

        let state = new_shared((input_data, output_data));
        let initial = StateManipulationLoop::new(
            state.clone(),
            |(input_vec, output_vec): &mut (Vec<usize>, Vec<u64>)| {
                match input_vec.pop() {
                    Some(i) => {
                        // this doesn't compile
                        // let (_, return_vec): &mut (Vec<_>, Vec<_>) = state_copy.lock().unwrap().borrow_mut();
                        // return_vec.push(i as u64);
                        output_vec.push(i as u64);
                        false
                    }
                    None => true,
                }
            },
        );
        let parallel = StateManipulationLoop::new(
            state.clone(),
            |(input_vec, output_vec): &mut (Vec<_>, Vec<_>)| {
                if let Some(i) = input_vec.pop() {
                    // this doesn't compile
                    // let (_, return_vec): &mut (Vec<_>, Vec<_>) = state_copy.lock().unwrap().borrow_mut();
                    // return_vec.push(i as u64);
                    output_vec.push(i as u64);
                    false
                } else {
                    true
                }
            },
        );
        let last = StateManipulationLoop::new(
            state.clone(),
            |(input_vec, output_vec): &mut (Vec<_>, Vec<_>)| {
                if let Some(i) = output_vec.pop() {
                    println!("Element {i}");
                    false
                } else {
                    input_vec.is_empty()
                }
            },
        );
        let _b = initial.parallel(parallel).parallel(last).run().join();
        let (in_state, out_state) = &*state.lock().unwrap();
        assert_eq!(in_state.len(), 0);
        assert_eq!(out_state.len(), 0);
    }

    #[test]
    fn drain_vec_sync() {
        let mut vec: Vec<_> = (1..LEN).collect();
        while let Some(i) = vec.pop() {
            println!("Element {i}");
        }
        assert!(vec.is_empty())
    }
}
