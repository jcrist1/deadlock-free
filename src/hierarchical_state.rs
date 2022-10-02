use std::{marker::PhantomData, sync::MutexGuard};

use frunk::{hlist::HList, HCons, HNil};

use crate::util::{SafeType, SharedMutex};
trait HierarchicalState {}

pub struct True;
pub struct False;
pub trait TypeBool {
    type Or<T: TypeBool>: TypeBool;
    type And<T: TypeBool>: TypeBool;
    fn bool() -> bool;
}
impl TypeBool for True {
    type Or<T: TypeBool> = True;
    type And<T: TypeBool> = T;
    fn bool() -> bool {
        true
    }
}
impl TypeBool for False {
    type Or<T: TypeBool> = T;
    type And<T: TypeBool> = False;
    fn bool() -> bool {
        false
    }
}

fn or<T: TypeBool>(_t: T) -> True {
    True
}

trait InductiveStateSubset {}

pub trait Counter: HList {}

impl Counter for HNil {}
impl<Tail: Counter> Counter for HCons<(), Tail> {}

pub trait Countable: HList {
    type Count: Counter;
}

impl Countable for HNil {
    type Count = HNil;
}

impl<Head, Tail: Countable> Countable for HCons<Head, Tail> {
    type Count = HCons<(), <Tail as Countable>::Count>;
}

pub trait SharedState: Countable {
    fn clone(&self) -> Self;
}

pub trait Filter: Countable {}

impl Filter for HNil {}

impl<H: TypeBool, T: Filter> Filter for HCons<H, T> {}

trait BoolAlg<Other: Filter>: Filter {
    type Or: Filter;
    type And: Filter;
}

impl BoolAlg<HNil> for HNil {
    type Or = HNil;
    type And = HNil;
}

impl<H1: TypeBool, T1: Filter + BoolAlg<T2>, H2: TypeBool, T2: Filter> BoolAlg<HCons<H2, T2>>
    for HCons<H1, T1>
{
    type Or = HCons<H1::Or<H2>, T1::Or>;
    type And = HCons<H1::And<H2>, T1::And>;
}

trait Lock<FilterType: Filter>: 'static {
    type LockType<'a>;
    type InnerType;
    fn lock<'a>(&'a self) -> Self::LockType<'a>;
}

impl Lock<HNil> for HNil {
    type LockType<'a> = HNil;
    type InnerType = HNil;
    fn lock<'a>(&'a self) -> HNil {
        HNil
    }
}

trait POSet<Other> {
    type IsSubset: TypeBool;

    fn is_subset(&self) -> bool {
        Self::IsSubset::bool()
    }
}

impl POSet<HNil> for HNil {
    type IsSubset = True;
}

impl<H1: TypeBool, T1: Filter + BoolAlg<T2> + POSet<T2>, H2: TypeBool, T2: Filter>
    POSet<HCons<H2, T2>> for HCons<H1, T1>
{
    type IsSubset =
        <<H2 as TypeBool>::And<<H1 as TypeBool>::Or<H2>> as TypeBool>::And<T1::IsSubset>;
}

impl<S: SafeType + 'static, TailFilter: Filter, TailState: Lock<TailFilter>>
    Lock<HCons<True, TailFilter>> for HCons<SharedMutex<S>, TailState>
{
    type LockType<'a> = HCons<MutexGuard<'a, S>, TailState::LockType<'a>>;

    type InnerType = HCons<S, TailState::InnerType>;

    fn lock<'a>(&'a self) -> Self::LockType<'a> {
        let HCons { head, tail } = self;
        let head = head.lock().unwrap();
        let tail = tail.lock();
        HCons { head, tail }
    }
}

impl<S: SafeType + 'static, TailFilter: Filter, TailState: Lock<TailFilter>>
    Lock<HCons<False, TailFilter>> for HCons<SharedMutex<S>, TailState>
{
    type LockType<'a> = TailState::LockType<'a>;

    type InnerType = TailState::InnerType;

    fn lock<'a>(&'a self) -> Self::LockType<'a> {
        let HCons { head, tail } = self;
        tail.lock()
    }
}

#[cfg(test)]
mod tests {
    use frunk::{hlist, hlist_pat, HList};

    use super::*;
    use crate::util::new_shared;

    // need to uncomment #[test]
    fn test_build() {
        let state_1 = new_shared(1u32);
        let state_2 = new_shared(Some(String::from("boop")));
        let state_3 = new_shared(0.32);
        let state = hlist!(state_1, state_2, state_3);
        println!("Starting first lock");
        let hlist_pat![_guard_1, _guard_2, _guard_3] =
            Lock::<HList![True, True, True]>::lock(&state);
        println!("Starting second lock");
        let hlist_pat![_guard_1, _guard_2] = Lock::<HList![True, True, False]>::lock(&state);
        let hlist_pat![_guard_1, _guard_2, _guard_3] = Lock::<
            <HList![True, True, False] as BoolAlg<HList![False, True, True]>>::Or,
        >::lock(&state);
        // this doesn't compile
        // let hlist_pat![guard_1, guard_2] = Lock::<HList![Used, Unused, Unused]>::lock(&state);
    }
}
