### Internal: worker command policy table

- Changed: the worker command policy table test no longer compiles while a command has no row,
  as `policy::of` already doesn't without a policy; before, only a comment asked for the row
  (#518).
