ordered-locks
=============
A rust libary for assigning levels to locks at compiletime in order to avoid deadlocks.

[![Crates.io](https://img.shields.io/crates/v/ordered-locks.svg)](https://crates.io/crates/ordered-locks)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)
[![Build status](https://img.shields.io/github/workflow/status/antialize/ordered-locks/RUST%20Continuous%20integration)](https://github.com/antialize/ordered-locks/actions)
[![Docs](https://img.shields.io/badge/docs-latest-blue.svg)](https://docs.rs/ordered-locks)

```rust
use ordered_locks::{L1, L2, Mutex, CleanLockToken};
// Create value at lock level 0, this lock cannot be acquired while a level1 lock is heldt
let v1 = Mutex::<L1, _>::new(42);
// Create value at lock level 1
let v2 = Mutex::<L2, _>::new(43);
// Construct a token indicating that this thread does not hold any locks
let mut token = unsafe {CleanLockToken::new()};
 
{
    // We can aquire the locks for v1 and v2 at the same time
    let mut g1 = v1.lock(token.token());
    let (g1, token) = g1.token_split();
    let mut g2 = v2.lock(token);
    *g2 = 11;
    *g1 = 12;
}
// Once the guards are dropped we can acquire other things
*v2.lock(token.token()) = 13;

// One cannot acquire locks in the wrong order
let mut g2 = v2.lock(token);
let (g2, token) = g2.token_split();
let mut g1 = v1.lock(token); // shouldn't compile!
*g2 = 11;
*g1 = 12;
```