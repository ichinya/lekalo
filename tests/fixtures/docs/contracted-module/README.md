# Synthetic contracted tutorial variant

Derived exclusively from the existing synthetic `contracted/planner-slice` corpus for issue #105. The Model omits that corpus's deliberately unimplemented count query; the declaration omits its absent support artifact. Maintained TS imports use explicit `.ts` paths/type-only imports for Node 24 tests; declaration fingerprints bind those exact authored bytes. No original fixture was changed and no private application was used.

Two named native tests execute the maintained focus/list functions. The tutorial records their external IDs after execution, then runs conformance. That checks declared signatures/effects/shapes, fingerprints and coverage presence; it does not prove database effects, all scenarios or production execution. The independent greenfield planner lane owns broader native evidence.
