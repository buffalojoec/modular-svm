# The Case for a Modular SVM

An effort is underway at Anza to extract most of the transaction processing 
pipeline out of the validator and into what will be known as the Solana Virtual
Machine (SVM). Although the official specification of this standalone SVM is
still in development, it's important that we get this right.

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

But let's push the envelope. Imagine if we engineered this new isolated SVM to 
be an assembly of entirely independent modules. Any SVM implementation could
simply drive these modules through well-defined interfaces.

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
