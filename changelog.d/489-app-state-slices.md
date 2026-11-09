### Internal: useAppState built from its action slices

- Changed: `UseAppStateResult` extends the action slice interfaces (`StagingActions`,
  `BranchActions`, `RemoteTransferActions` and the others) and declares only the members
  `useAppState` defines itself, and the hook spreads the slice objects into its result instead
  of naming each action again. Each action is now declared and documented once, in its slice;
  where the two comments had drifted, the more accurate one was kept. A compile-time check fails
  when two slices share a member name, which the spread would otherwise resolve silently.
  `useAppState.ts` shrinks from 949 to 594 lines. Behaviour is unchanged (#489).
