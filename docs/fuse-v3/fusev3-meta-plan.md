# Fuse3
This document aims to collection learning from fuse2 development so a new guide line and plan are developed based on what has been done and learnt so far.

Fuse3 will be developed in Go because of the following reasons.
1. Built-in system tools.
2. Internal SSA, native Intermediate Representation (IR)
3. Low dependency that go has.
4. high AI agents reliability.
5. Fast iteration speed (near-instant compilation)
6. Good portability.


Anything that was not done from scratch in Fuse2 will be put in guide, planed and developed from scratch based on what we have learnt. These include but not limited to the following:
- concurrency
- maps
- memory management for threads

Rules should be put in every document to ensure alignment by AI agents.

Documents will be written in the following order.
- Language guide. The language same philosophy and three pillars still holds.
- Implementation plan. This will be really extensive without forgetting all the issues we faced when implementing Fuse2, these will be included in the right place.
- Repository layout.