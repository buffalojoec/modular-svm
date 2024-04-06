# The Case for a Modular SVM

An effort is underway at Anza to extract most of the transaction processing
pipeline out of the validator and into what will be known as the Solana Virtual
Machine (SVM).

<https://github.com/solana-labs/solana/issues/34196>

<https://github.com/anza-xyz/agave/issues/389>

Although the official specification of this standalone SVM is still in
development, it's important that we get this right.

An isolated SVM would be a transaction processing pipeline that can operate
independent of any validator. Validators would then run some implementation of
an SVM, and brand-new services could be built on top of custom SVM-compatible
engines.

## Why is a Modular SVM Important?

Having a decoupled SVM with its own well-defined interface unlocks the ability
for teams to build custom SVM implementations, including:

- SVM rollups
- SVM sidechains
- SVM-based off-chain services

Solutions like these can make Solana more performant and more reliable, as well
as expand the landscape of possible products and services that can be built
within its ecosystem.

👉 But let's push the envelope. Imagine if we engineered this new isolated SVM
to be an assembly of **entirely independent modules**. Any SVM implementation
could simply drive these modules through well-defined interfaces.

This further disintegrates the barriers to SVM-compatible projects by requiring
significantly less overhead to architect custom solutions. Teams could simply
implement the modules they care about while using already established
implementations for the others (such as those from Agave or Firedancer).

## How Do We Get There?

We must take this opportunity to break away from library patterns that have
plagued both core and protocol developers for a long time. Some of these issues
include:

- High-level libraries depending on low-level libraries for simple things such
  as types.
- Tightly-coupled libraries using each other's objects instead of interfaces
  and adapters.
- Metrics-capturing strands wired from the highest-level packages all the way to
  the lowest-level.

The modularity goals outlined in the previous section can be obtained by
remedying these issues. Some suggested solutions are as follows:

- Leverage lean, low-level type packages.
- Differentiate between interfaces and implementations.
- Connect implementations using interface adapters.
- Bake metrics into the specification.

## About This Repository

This repository seeks to demonstrate the concepts above by offering two groups
of crates:

- `solana`: The specification-based crates for types and interfaces, to be used
  by implementations.
- `agave`: Anza's Agave client implementations of the `solana` specifications.

The `solana-runtime` specification (grossly over-simplified here) details a
runtime that makes use of an SVM. However, notice that this is all done with
**interfaces**.

https://github.com/buffalojoec/modular-svm/blob/d52d0d34a5ce9e8fcda1153ff45934ab9721a310/solana/runtime/src/specification.rs#L10-L33

Meanwhile, the Agave runtime is now an _implementation_ (`agave-runtime`), and
it simply implements the `solana-runtime` interface, but **without specifying
a specific SVM implementation**.

https://github.com/buffalojoec/modular-svm/blob/d52d0d34a5ce9e8fcda1153ff45934ab9721a310/agave/runtime/src/lib.rs#L19-L52

The beautiful thing here is that any SVM could easily be plugged into Agave's
runtime implementation. Anyone could configure an Agave node, then write an
adapter for some other SVM implementation and plug it in right here.

https://github.com/buffalojoec/modular-svm/blob/d52d0d34a5ce9e8fcda1153ff45934ab9721a310/agave/validator/src/lib.rs#L19-L52

🔑 🔑 A huge advantage with this arrangement is the fact that consensus-breaking
changes would reside in the specification-level, guarded by SIMDs, while
developers could more freely adjust implementation-level code and ship new
versions without worrying about partitioning the network.

Other important notes:

- This demo uses lightweight "leaf node" crates for types
  (ie. `solana-compute-budget`).
- Some leaf node crates are specification-wide (ie. `solana-compute-budget`)
  while others are implementation-specific (ie. `agave-program-cache`).
- Although metrics are not demonstrated here (yet), the idea is that they would
  reside in one's implementation, and be vended back up to the callers.
