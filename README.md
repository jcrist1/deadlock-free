# Can we build deadlock free computational graphs on shared mutable state?
About a month ago [I was wondering](
https://www.reddit.com/r/rust/comments/w7g8js/lets_fix_buffered_streams/ihjs75p/?utm_source=share&utm_medium=ios_app&utm_name=iossmf&context=3
) if it would be type theoretically possible to guarantee deadlock
free computation on shared state. This a first stab at a solution. The basic
idea is to build "simple" computational units that are performed while holding
a lock. Importantaly we forbid mutexes or locks inside the computational unit 
which should ensure there is never a deadlock. To forbid mutexes we implement an 
auto trait `MutexFree`:
```rust
auto trait MutexFree {}
impl<T> ! MutexFree for Mutex<T> {}
impl<'a, T> ! MutexFree for MutexGuard<T> {}
```
Then we apply this restriction to the Fn types we want to represent our 
computations. For now I'm just being super conservative and insisting on 
```
F: Fn(&mut State) -> Y + Send + Sync + MutexFree + 'static
```
With the Send and 'static restriction I would be guaranteed to not have any
captured MutexGuards so probably don't need the MutexFree trait on those.

In my linked code I'm only implemented a simple unit that operates on the state
of the graph, but it would be straight-forward to implement computational units 
with inputs and outputs with channels. Now the following code which would 
otherwise deadlock, won't compile.
    
**this doesn't compile**
```rust
let state_copy = state.clone()
let parallel = StateManipulation::new(state.clone(), move |(input_vec, _)| {
    if let Some(i) = input_vec.pop() {
        let (_, return_vec): &mut (Vec<_>, Vec<_>) = &mut state_copy.lock().unwrap();
        return_vec.push(i as u64);
        false
    } else {
        true
    }
});
```

**this does**
```rust
let parallel = StateManipulation::new(state.clone(), move |(input_vec, return_vec)| {
    if let Some(i) = input_vec.pop() {
        return_vec.push(i as u64);
        false
    } else {
        true
    }
});
```

I've also implemented a struct to run multiple computations in parallel, but no
composition yet. 

In addition to those, it would be good to implement some kind of state 
segregation, e.g. combinging multiple mutexes and only locking those needed for 
the current stage of the computation. I think this would be quite interesting, 
and feel like there would be hidden deadlocks in a naive implementation. There 
would probably have to be a hierarchy of state, and you couldn't lock lower 
level state if a dependent computational step needs to locks higher level state. 

Finally `async`? I have no idea. Big can of worms there. But I guess I'd even 
prefer to implement this just in async.

It would also be good to separate computation that doesn't need the state, nor
any threading.

## Sources for inspirations
After an interesting [thread](
    https://www.reddit.com/r/rust/comments/wy84oh/can_we_create_deadlock_free_computation_in_the/
) I was given a lot of sources for potential inspiration, or places where 
something similar is being done:
* [Pony](https://www.ponylang.io/) – uses an actor model but forbids locks, and
doesn't allow mutable state
* [RTIC](https://rtic.rs/1/book/en/) – a real-time focused library that is able
to guarantee no deadlocks
* [Claro](
    https://github.com/JasonSteving99/claro-lang/blob/faa3ed2c4dde1702f0cacf5124f85fbecf36ec72/src/java/com/claro/claro_programs/graphs.claro#L20-L48
) a hobby language that has some interesting methods of ensuring non-blocking

Then I also got pointed out some good literature
* Start with the wikipedia page on [deadlocks](
    https://en.wikipedia.org/wiki/Deadlock
).
* [Static Deadlock Analysis](
    https://www.researchgate.net/publication/226054631_Static_Deadlock_Analysis_for_CSP-Type_Communications
)
* [Linear temporal logic](https://en.wikipedia.org/wiki/Linear_temporal_logic)
* [Bounded LTL model checking with stable models](
    https://www.cambridge.org/core/journals/theory-and-practice-of-logic-programming/article/abs/bounded-ltl-model-checking-with-stable-models/CFECB9A9830B12BFABB0C80C938DB41B
). Arxiv version [here](https://arxiv.org/abs/cs/0305040).

# WARNING!
I have no idea if this will work. I haven't proven anything about this, and 
haven't tested anything. I just thought this would be an interesting idea.
