#![feature(auto_traits)]
#![feature(negative_impls)]
#![feature(trait_alias)]
use std::{
    borrow::BorrowMut,
    marker::PhantomData,
    sync::{Arc, Mutex, MutexGuard},
    thread::{self, JoinHandle},
};

trait SafeComputation<I, O> = Fn(I) -> O + Send + Sync + MutexFree + 'static;

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

trait DeadlockFreeGraph: Sized {
    type CurrentLock;
    type LockInnerType: Send + Sync;
    type Output;
    type JoinOutput: Joinable;

    fn run(self) -> Self::JoinOutput;

    fn union<Other: DeadlockFreeGraph<LockInnerType = Self::LockInnerType>>(
        self,
        other: Other,
    ) -> DisjointUnion<Self, Other> {
        DisjointUnion {
            left: self,
            right: other,
        }
    }
}

type SharedMutex<T> = Arc<Mutex<T>>;

fn new_shared<T>(t: T) -> SharedMutex<T> {
    Arc::new(Mutex::new(t))
}

struct StateManipulation<State, F: Fn(&mut State) -> bool + MutexFree> {
    state: SharedMutex<State>,
    transition: F,
    done: bool,
}

impl<State, Left, Right, LeftJoinable, RightJoinable> DeadlockFreeGraph
    for DisjointUnion<Left, Right>
where
    State: Send + Sync + MutexFree + 'static,
    Left: DeadlockFreeGraph<LockInnerType = State, JoinOutput = LeftJoinable>,
    Right: DeadlockFreeGraph<LockInnerType = State, JoinOutput = RightJoinable>,
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

auto trait MutexFree {}
impl<T> !MutexFree for Mutex<T> {}
impl<'a, T> !MutexFree for MutexGuard<'a, T> {}

impl<
        State: Send + Sync + 'static,
        F: Fn(&mut State) -> bool + MutexFree + Send + Sync + 'static,
    > DeadlockFreeGraph for StateManipulation<State, F>
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

impl<State: 'static, F: Fn(&mut State) -> bool + MutexFree + Send + Sync + 'static>
    StateManipulation<State, F>
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
    const LEN: usize = 10000000;

    #[test]
    fn drain_vec_parallel() {
        let input_data = (1..LEN).collect::<Vec<_>>();
        let output_data = Vec::<u64>::with_capacity(LEN);

        let state = new_shared((input_data, output_data));
        let initial = StateManipulation::new(state.clone(), |(input_vec, output_vec)| {
            if let Some(i) = input_vec.pop() {
                // this doesn't compile
                // let (_, return_vec): &mut (Vec<_>, Vec<_>) = state_copy.lock().unwrap().borrow_mut();
                // return_vec.push(i as u64);
                output_vec.push(i as u64);
                false
            } else {
                true
            }
        });
        let parallel = StateManipulation::new(state.clone(), |(input_vec, output_vec)| {
            if let Some(i) = input_vec.pop() {
                // this doesn't compile
                // let (_, return_vec): &mut (Vec<_>, Vec<_>) = state_copy.lock().unwrap().borrow_mut();
                // return_vec.push(i as u64);
                output_vec.push(i as u64);
                false
            } else {
                true
            }
        });
        let last = StateManipulation::new(state.clone(), |(input_vec, output_vec)| {
            if let Some(i) = output_vec.pop() {
                println!("Element {i}");
                false
            } else {
                input_vec.is_empty()
            }
        });
        let _b = initial.union(parallel).union(last).run().join();
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
